use crate::{AppError, translation::parse_language_code};
use flate2::read::GzDecoder;
use isolang::Language;
use linguaspark::{DecodeOptions, Executor, Model, ModelAssets, VocabularyAssets};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use tokio::sync::{Mutex as AsyncMutex, mpsc};

struct ExecutorPool {
    available_tx: mpsc::UnboundedSender<Executor>,
    available_rx: AsyncMutex<mpsc::UnboundedReceiver<Executor>>,
}

impl ExecutorPool {
    fn new(num_workers: usize) -> Result<Self, AppError> {
        let (available_tx, available_rx) = mpsc::unbounded_channel();
        for worker in 0..num_workers {
            let executor = Executor::new().map_err(|error| {
                AppError::ConfigError(format!(
                    "Failed to create inference executor {}: {}",
                    worker + 1,
                    error
                ))
            })?;
            available_tx.send(executor).map_err(|error| {
                AppError::ConfigError(format!("Failed to initialize executor pool: {error}"))
            })?;
        }
        Ok(Self {
            available_tx,
            available_rx: AsyncMutex::new(available_rx),
        })
    }

    async fn acquire(&self) -> Result<ExecutorLease, AppError> {
        let executor = self.available_rx.lock().await.recv().await.ok_or_else(|| {
            AppError::InferenceError("Inference executor pool was closed".to_string())
        })?;
        Ok(ExecutorLease {
            executor: Some(executor),
            available_tx: self.available_tx.clone(),
        })
    }

    async fn execute<R, F>(&self, operation: F) -> Result<R, AppError>
    where
        R: Send + 'static,
        F: FnOnce(&mut Executor) -> Result<R, AppError> + Send + 'static,
    {
        let mut lease = self.acquire().await?;
        let task = tokio::task::spawn_blocking(move || operation(lease.executor_mut()));

        match task.await {
            Ok(result) => result,
            Err(error) if error.is_panic() => {
                tracing::error!("Inference executor panicked: {error}");
                Err(AppError::InferenceError(
                    "Inference executor panicked".to_string(),
                ))
            }
            Err(error) => Err(AppError::InferenceError(format!(
                "Inference task failed: {error}"
            ))),
        }
    }
}

struct ExecutorLease {
    executor: Option<Executor>,
    available_tx: mpsc::UnboundedSender<Executor>,
}

impl ExecutorLease {
    fn executor_mut(&mut self) -> &mut Executor {
        self.executor
            .as_mut()
            .expect("executor lease must contain an executor")
    }
}

impl Drop for ExecutorLease {
    fn drop(&mut self) {
        if let Some(executor) = self.executor.take()
            && let Err(error) = self.available_tx.send(executor)
        {
            tracing::debug!("Executor pool closed while returning executor: {error}");
        }
    }
}

pub struct InferenceEngine {
    models: HashMap<(Language, Language), Model>,
    executors: ExecutorPool,
}

impl InferenceEngine {
    pub fn load(models_dir: &Path, num_workers: usize) -> Result<Self, AppError> {
        if num_workers == 0 {
            return Err(AppError::ConfigError(
                "NUM_WORKERS must be at least 1".to_string(),
            ));
        }

        let mut directories = fs::read_dir(models_dir)?.collect::<Result<Vec<_>, _>>()?;
        directories.sort_by_key(|entry| entry.file_name());

        let mut models = HashMap::new();
        for entry in directories {
            if !entry.path().is_dir() {
                continue;
            }
            let path = entry.path();
            let directory_name = entry.file_name().to_string_lossy().into_owned();
            let (source, target) = parse_language_pair(&directory_name)?;
            let source_code = iso_code(&source)?;
            let target_code = iso_code(&target)?;
            let key = (source, target);
            if models.contains_key(&key) {
                return Err(AppError::ConfigError(format!(
                    "Duplicate model for language pair '{}-{}'",
                    source_code, target_code
                )));
            }

            tracing::info!(
                "Loading model '{}-{}' from {}",
                source_code,
                target_code,
                path.display()
            );
            let assets = discover_model_assets(&path)?;
            let model = Model::from_assets(assets).map_err(|error| {
                AppError::ConfigError(format!(
                    "Failed to load model '{}-{}' from '{}': {}",
                    source_code,
                    target_code,
                    path.display(),
                    error
                ))
            })?;
            models.insert(key, model);
        }

        if models.is_empty() {
            return Err(AppError::ConfigError(format!(
                "No model directories found in '{}'",
                models_dir.display()
            )));
        }

        tracing::info!("Creating {num_workers} global inference executor(s)");
        Ok(Self {
            models,
            executors: ExecutorPool::new(num_workers)?,
        })
    }

    pub fn sole_language_pair(&self) -> Option<(Language, Language)> {
        let mut pairs = self.models.keys();
        let pair = *pairs.next()?;
        pairs.next().is_none().then_some(pair)
    }

    pub async fn translate(
        &self,
        from: Language,
        to: Language,
        text: &str,
    ) -> Result<String, AppError> {
        let mut translations = self
            .translate_batch(from, to, vec![text.to_string()])
            .await?;
        translations.pop().ok_or_else(|| {
            AppError::InferenceError("Single-input batch returned no translation".to_string())
        })
    }

    pub async fn translate_batch(
        &self,
        from: Language,
        to: Language,
        texts: Vec<String>,
    ) -> Result<Vec<String>, AppError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        if let Some(model) = self.models.get(&(from, to)).cloned() {
            return self.execute_batch(model, texts).await;
        }

        let first = self
            .models
            .get(&(from, Language::Eng))
            .cloned()
            .ok_or_else(|| unsupported_pair(from, to))?;
        let second = self
            .models
            .get(&(Language::Eng, to))
            .cloned()
            .ok_or_else(|| unsupported_pair(from, to))?;
        let intermediate = self.execute_batch(first, texts).await?;
        self.execute_batch(second, intermediate).await
    }

    async fn execute_batch(
        &self,
        model: Model,
        texts: Vec<String>,
    ) -> Result<Vec<String>, AppError> {
        self.executors
            .execute(move |executor| {
                executor
                    .translate_batch(&model, &texts, &DecodeOptions::default())
                    .map(|translations| {
                        translations
                            .into_iter()
                            .map(|translation| translation.text)
                            .collect()
                    })
                    .map_err(|error| AppError::InferenceError(error.to_string()))
            })
            .await
    }
}

fn unsupported_pair(from: Language, to: Language) -> AppError {
    AppError::TranslationError(format!(
        "Translation from '{}' to '{}' is not supported",
        display_code(&from),
        display_code(&to)
    ))
}

fn parse_language_pair(name: &str) -> Result<(Language, Language), AppError> {
    let (source, target) = if name.len() == 4 && name.is_ascii() {
        (&name[..2], &name[2..])
    } else {
        let mut parts = name.split('-');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(source), Some(target), None) => (source, target),
            _ => {
                return Err(AppError::ConfigError(format!(
                    "Invalid model directory '{}'; expected 'enzh' or 'en-zh'",
                    name
                )));
            }
        }
    };
    Ok((parse_language_code(source)?, parse_language_code(target)?))
}

fn iso_code(language: &Language) -> Result<&'static str, AppError> {
    if language.to_639_3() == "cmn" {
        return Ok("zh");
    }
    language.to_639_1().ok_or_else(|| {
        AppError::ConfigError(format!(
            "Language '{}' does not have an ISO 639-1 code",
            language
        ))
    })
}

fn display_code(language: &Language) -> &'static str {
    if language.to_639_3() == "cmn" {
        "zh"
    } else {
        language.to_639_1().unwrap_or_else(|| language.to_639_3())
    }
}

fn discover_model_assets(model_dir: &Path) -> Result<ModelAssets, AppError> {
    let mut model_path = None;
    let mut shortlist_path = None;
    let mut shared_vocab_path = None;
    let mut source_vocab_path = None;
    let mut target_vocab_path = None;

    for entry in fs::read_dir(model_dir)? {
        let entry = entry?;
        if !entry.path().is_file() {
            continue;
        }
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let uncompressed_name = file_name.strip_suffix(".gz").unwrap_or(&file_name);

        if uncompressed_name.ends_with(".s2t.bin") {
            set_unique(&mut shortlist_path, path, "shortlist", model_dir)?;
        } else if uncompressed_name.ends_with(".bin") && uncompressed_name.contains(".intgemm") {
            set_unique(&mut model_path, path, "model", model_dir)?;
        } else if uncompressed_name.ends_with(".spm") {
            if uncompressed_name.starts_with("srcvocab") {
                set_unique(&mut source_vocab_path, path, "source vocabulary", model_dir)?;
            } else if uncompressed_name.starts_with("trgvocab") {
                set_unique(&mut target_vocab_path, path, "target vocabulary", model_dir)?;
            } else if uncompressed_name.starts_with("vocab") {
                set_unique(&mut shared_vocab_path, path, "shared vocabulary", model_dir)?;
            }
        }
    }

    let required = |path: Option<PathBuf>, kind: &str| {
        path.ok_or_else(|| {
            AppError::ConfigError(format!(
                "Missing {} in model directory '{}'",
                kind,
                model_dir.display()
            ))
        })
    };
    let model = read_asset(&required(model_path, "model")?)?;
    let shortlist = read_asset(&required(shortlist_path, "shortlist")?)?;
    let vocabularies = match (shared_vocab_path, source_vocab_path, target_vocab_path) {
        (Some(shared), None, None) => VocabularyAssets::Shared(read_asset(&shared)?),
        (None, Some(source), Some(target)) => VocabularyAssets::Separate {
            source: read_asset(&source)?,
            target: read_asset(&target)?,
        },
        _ => {
            return Err(AppError::ConfigError(format!(
                "Model directory '{}' must contain either one shared vocabulary or one source and one target vocabulary",
                model_dir.display()
            )));
        }
    };

    Ok(ModelAssets {
        model,
        vocabularies,
        shortlist,
    })
}

fn read_asset(path: &Path) -> Result<Vec<u8>, AppError> {
    let bytes = fs::read(path)?;
    if path.extension().is_some_and(|extension| extension == "gz") {
        let mut decoded = Vec::new();
        GzDecoder::new(bytes.as_slice()).read_to_end(&mut decoded)?;
        Ok(decoded)
    } else {
        Ok(bytes)
    }
}

fn set_unique(
    slot: &mut Option<PathBuf>,
    path: PathBuf,
    kind: &str,
    model_dir: &Path,
) -> Result<(), AppError> {
    if slot.replace(path).is_some() {
        return Err(AppError::ConfigError(format!(
            "Multiple {} files found in model directory '{}'",
            kind,
            model_dir.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ExecutorPool;
    use crate::AppError;
    use std::sync::Arc;

    #[tokio::test]
    async fn executor_returns_after_success_and_error() {
        let pool = ExecutorPool::new(1).unwrap();
        assert_eq!(pool.execute(|_| Ok(42)).await.unwrap(), 42);

        let error = pool
            .execute::<(), _>(|_| Err(AppError::InferenceError("expected".to_string())))
            .await;
        assert!(error.is_err());
        assert_eq!(pool.execute(|_| Ok(7)).await.unwrap(), 7);
    }

    #[tokio::test]
    async fn executor_returns_after_panic() {
        let pool = ExecutorPool::new(1).unwrap();
        let error = pool
            .execute::<(), _>(|_| panic!("expected test panic"))
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::InferenceError(_)));
        assert_eq!(pool.execute(|_| Ok(11)).await.unwrap(), 11);
    }

    #[tokio::test]
    async fn executor_pool_enforces_global_concurrency_limit() {
        let pool = Arc::new(ExecutorPool::new(1).unwrap());
        let lease = pool.acquire().await.unwrap();
        let second_pool = Arc::clone(&pool);
        let mut second = Box::pin(second_pool.execute(|_| Ok(())));

        tokio::select! {
            biased;
            result = &mut second => panic!("second task started without an available executor: {result:?}"),
            _ = std::future::ready(()) => {}
        }

        drop(lease);
        second.await.unwrap();
    }
}
