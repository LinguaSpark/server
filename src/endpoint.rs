use crate::{
    AppError, AppState,
    translation::{
        detect_language_code, normalize_language_code, perform_batch_translation,
        perform_translation,
    },
};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::SystemTime};

#[derive(Debug, Deserialize)]
pub struct DetectLanguageRequest {
    text: String,
}

#[derive(Debug, Serialize)]
pub struct DetectLanguageResponse {
    language: String,
}

pub async fn detect_language(
    Json(request): Json<DetectLanguageRequest>,
) -> Result<Json<DetectLanguageResponse>, AppError> {
    Ok(Json(DetectLanguageResponse {
        language: detect_language_code(&request.text)?.to_owned(),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TranslationInput {
    Single(String),
    Batch(Vec<String>),
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum TranslationOutput {
    Single(String),
    Batch(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub struct TranslationRequest {
    text: TranslationInput,
    from: Option<String>,
    to: String,
}

#[derive(Debug, Serialize)]
pub struct TranslationResponse {
    text: TranslationOutput,
    from: String,
    to: String,
}

pub async fn translate(
    State(state): State<Arc<AppState>>,
    Json(request): Json<TranslationRequest>,
) -> Result<Json<TranslationResponse>, AppError> {
    let (text, from_lang, to_lang) = match request.text {
        TranslationInput::Single(text) => {
            let (text, from, to) =
                perform_translation(&state, &text, request.from, &request.to).await?;
            (TranslationOutput::Single(text), from, to)
        }
        TranslationInput::Batch(texts) => {
            if texts.is_empty() {
                return Err(AppError::TranslationError(
                    "Translation batch must contain at least one text".to_string(),
                ));
            }
            let to = normalize_language_code(&request.to)?.to_string();
            let translations =
                perform_batch_translation(&state, texts, request.from, &request.to).await?;
            let from = translations
                .first()
                .map(|(_, from)| from.clone())
                .expect("non-empty translation batch must return source language");
            let texts = translations.into_iter().map(|(text, _)| text).collect();
            (TranslationOutput::Batch(texts), from, to)
        }
    };

    Ok(Json(TranslationResponse {
        text,
        from: from_lang,
        to: to_lang,
    }))
}

#[derive(Debug, Deserialize)]
pub struct KissTranslationRequest {
    text: String,
    from: Option<String>,
    to: String,
}

#[derive(Debug, Serialize)]
pub struct KissTranslationResponse {
    text: String,
    from: String,
    to: String,
}

pub async fn translate_kiss(
    State(state): State<Arc<AppState>>,
    Json(request): Json<KissTranslationRequest>,
) -> Result<Json<KissTranslationResponse>, AppError> {
    let (text, from_lang, to_lang) =
        perform_translation(&state, &request.text, request.from, &request.to).await?;

    Ok(Json(KissTranslationResponse {
        text,
        from: from_lang,
        to: to_lang,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ImmersiveTranslationRequest {
    source_lang: Option<String>,
    target_lang: String,
    text_list: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ImmersiveTranslationItem {
    detected_source_lang: String,
    text: String,
}

#[derive(Debug, Serialize)]
pub struct ImmersiveTranslationResponse {
    translations: Vec<ImmersiveTranslationItem>,
}

pub async fn translate_immersive(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ImmersiveTranslationRequest>,
) -> Result<Json<ImmersiveTranslationResponse>, AppError> {
    let translations = perform_batch_translation(
        &state,
        request.text_list,
        request.source_lang,
        &request.target_lang,
    )
    .await?
    .into_iter()
    .map(|(text, detected_source_lang)| ImmersiveTranslationItem {
        detected_source_lang,
        text,
    })
    .collect();

    Ok(Json(ImmersiveTranslationResponse { translations }))
}

#[derive(Debug, Deserialize)]
pub struct HcfyTranslationRequest {
    text: String,
    source: Option<String>,
    destination: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct HcfyTranslationResponse {
    text: String,
    from: String,
    to: String,
    result: Vec<String>,
}

pub async fn translate_hcfy(
    State(state): State<Arc<AppState>>,
    Json(request): Json<HcfyTranslationRequest>,
) -> Result<Json<HcfyTranslationResponse>, AppError> {
    const LANGUAGE_CODE_MAP: &[(&str, &str)] =
        &[("中文(简体)", "zh"), ("英语", "en"), ("日语", "ja")];

    fn convert_language_name(lang: &str) -> String {
        LANGUAGE_CODE_MAP
            .iter()
            .find(|&&(name, _)| name == lang)
            .map(|&(_, code)| code)
            .unwrap_or(lang)
            .to_string()
    }

    fn get_language_name(code: &str) -> String {
        LANGUAGE_CODE_MAP
            .iter()
            .find(|&&(_, mapped_code)| mapped_code == code)
            .map(|&(name, _)| name)
            .unwrap_or(code)
            .to_string()
    }

    let source_lang = request.source.as_deref().map(convert_language_name);

    let target_lang = match (
        request.destination.first(),
        source_lang.as_deref(),
        request.destination.get(1),
    ) {
        (None, _, _) => "en".to_string(),
        (Some(first), Some(src), Some(second)) if convert_language_name(first) == src => {
            convert_language_name(second)
        }
        (Some(first), _, _) => convert_language_name(first),
    };

    let (translated_text, detected_source, _) =
        perform_translation(&state, &request.text, source_lang, &target_lang).await?;

    Ok(Json(HcfyTranslationResponse {
        text: request.text,
        from: get_language_name(&detected_source),
        to: get_language_name(&target_lang),
        result: vec![translated_text],
    }))
}

#[derive(Debug, Deserialize)]
pub struct DeeplxTranslationRequest {
    text: String,
    source_lang: Option<String>,
    target_lang: String,
}

#[derive(Debug, Serialize)]
pub struct DeeplxTranslationResponse {
    code: u32,
    id: u128,
    data: String,
    alternatives: Vec<String>,
    source_lang: String,
    target_lang: String,
    method: String,
}

pub async fn translate_deeplx(
    State(state): State<Arc<AppState>>,
    Json(request): Json<DeeplxTranslationRequest>,
) -> Result<Json<DeeplxTranslationResponse>, AppError> {
    let (text, from_lang, to_lang) = perform_translation(
        &state,
        &request.text,
        request.source_lang.map(|lang| lang.to_lowercase()),
        &request.target_lang.to_lowercase(),
    )
    .await?;

    Ok(Json(DeeplxTranslationResponse {
        code: 200,
        id: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        data: text,
        alternatives: vec![],
        source_lang: from_lang.to_uppercase(),
        target_lang: to_lang.to_uppercase(),
        method: "Free".to_owned(),
    }))
}
