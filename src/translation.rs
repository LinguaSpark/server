use crate::{AppError, AppState};
use isolang::Language;
use std::sync::Arc;
use whichlang::Lang;

pub fn parse_language_code(code: &str) -> Result<Language, AppError> {
    let normalized = code.split('-').next().unwrap_or(code).to_ascii_lowercase();
    Language::from_639_1(&normalized)
        .ok_or_else(|| {
            AppError::TranslationError(format!(
                "Invalid language code: '{}'. Please use ISO 639-1 format.",
                code
            ))
        })
}

pub fn language_code(language: &Language) -> &'static str {
    language.to_639_1().expect("language has no ISO 639-1 code")
}

pub fn detect_language_code(text: &str) -> Result<&'static str, AppError> {
    Ok(language_code(&detect_language(text)?))
}

pub fn normalize_language_code(code: &str) -> Result<&'static str, AppError> {
    Ok(language_code(&parse_language_code(code)?))
}

fn from_whichlang(language: Lang) -> Language {
    match language {
        Lang::Ara => Language::Ara,
        // whichlang uses "cmn" for Chinese, but only Zho has the ISO 639-1 code "zh"
        Lang::Cmn => Language::Zho,
        Lang::Deu => Language::Deu,
        Lang::Eng => Language::Eng,
        Lang::Fra => Language::Fra,
        Lang::Hin => Language::Hin,
        Lang::Ita => Language::Ita,
        Lang::Jpn => Language::Jpn,
        Lang::Kor => Language::Kor,
        Lang::Nld => Language::Nld,
        Lang::Por => Language::Por,
        Lang::Rus => Language::Rus,
        Lang::Spa => Language::Spa,
        Lang::Swe => Language::Swe,
        Lang::Tur => Language::Tur,
        Lang::Vie => Language::Vie,
    }
}

#[expect(clippy::unnecessary_wraps, reason = "detector may become fallible")]
fn detect_language(text: &str) -> Result<Language, AppError> {
    Ok(from_whichlang(whichlang::detect_language(text)))
}

fn resolve_source_language(
    state: &AppState,
    text: &str,
    from_lang: Option<&str>,
    target_lang: Language,
) -> Result<Language, AppError> {
    let explicit = from_lang.filter(|code| !code.is_empty() && !code.eq_ignore_ascii_case("auto"));

    match explicit {
        Some(code) => parse_language_code(code),
        None => match state.sole_language_pair {
            Some((source, target)) if target == target_lang => Ok(source),
            _ => detect_language(text),
        },
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

    let from_code = language_code(&source_lang);
    let to_code = language_code(&target_lang);

    // If source and target languages are the same, return the original text
    if source_lang == target_lang {
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
    let source_code = language_code(&source_lang).to_string();

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
