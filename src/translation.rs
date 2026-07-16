use crate::{AppError, AppState};
use isolang::Language;
use std::sync::Arc;

pub fn parse_language_code(code: &str) -> Result<Language, AppError> {
    Language::from_639_1(code.split('-').next().unwrap_or(code)).ok_or_else(|| {
        AppError::TranslationError(format!(
            "Invalid language code: '{}'. Please use ISO 639-1 format.",
            code
        ))
    })
}

fn get_iso_code(lang: &Language) -> Result<&'static str, AppError> {
    if lang.to_639_3() == "cmn" {
        // whichlang uses "cmn" for Chinese, but we want to return "zh" for ISO 639-1
        return Ok("zh");
    }
    lang.to_639_1().ok_or_else(|| {
        AppError::TranslationError(format!(
            "Language '{}' doesn't have an ISO 639-1 code",
            lang
        ))
    })
}

pub fn detect_language_code(text: &str) -> Result<&'static str, AppError> {
    get_iso_code(&detect_language(text)?)
}

fn detect_language(text: &str) -> Result<Language, AppError> {
    Language::from_639_3(whichlang::detect_language(text).three_letter_code()).ok_or_else(|| {
        AppError::TranslationError(format!("Failed to identify language for text: '{}'", text))
    })
}

fn resolve_source_language(
    state: &AppState,
    text: &str,
    from_lang: Option<&str>,
    target_lang: Language,
) -> Result<Language, AppError> {
    match from_lang {
        None | Some("") | Some("auto") => match state.sole_language_pair {
            Some((source, target)) if target == target_lang => Ok(source),
            _ => detect_language(text),
        },
        Some(code) => parse_language_code(code),
    }
}

pub async fn perform_translation(
    state: &Arc<AppState>,
    text: &str,
    from_lang: Option<String>,
    to_lang: &str,
) -> Result<(String, String, String), AppError> {
    let target_lang = parse_language_code(to_lang)?;

    let source_lang = resolve_source_language(state, text, from_lang.as_deref(), target_lang)?;

    let from_code = get_iso_code(&source_lang)?;
    let to_code = get_iso_code(&target_lang)?;

    // If source and target languages are the same, return the original text
    if from_code == to_code {
        return Ok((text.to_string(), from_code.to_string(), to_code.to_string()));
    }

    let translated_text = state
        .inference
        .translate(source_lang, target_lang, text)
        .await?;

    Ok((translated_text, from_code.to_string(), to_code.to_string()))
}

pub async fn perform_batch_translation(
    state: &Arc<AppState>,
    texts: Vec<String>,
    from_lang: Option<String>,
    to_lang: &str,
) -> Result<Vec<(String, String)>, AppError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let target_lang = parse_language_code(to_lang)?;
    let source_lang = resolve_source_language(state, &texts[0], from_lang.as_deref(), target_lang)?;
    let source_code = get_iso_code(&source_lang)?.to_string();

    if source_lang == target_lang {
        return Ok(texts
            .into_iter()
            .map(|text| (text, source_code.clone()))
            .collect());
    }

    let input_count = texts.len();
    let translations = state
        .inference
        .translate_batch(source_lang, target_lang, texts)
        .await?;

    if translations.len() != input_count {
        return Err(AppError::InferenceError(format!(
            "Batch translation returned {} result(s) for {} input(s)",
            translations.len(),
            input_count
        )));
    }

    Ok(translations
        .into_iter()
        .map(|translation| (translation, source_code.clone()))
        .collect())
}
