use crate::{
    AppError, AppState,
    translation::{detect_language_code, perform_translation},
};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::SystemTime};

const VERSION: &str = "v3.0.0";

// Version endpoint
#[derive(Debug, Serialize)]
pub struct VersionResponse {
    version: String,
}

pub async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: VERSION.to_string(),
    })
}

// Languages endpoint
#[derive(Debug, Serialize)]
pub struct LanguagesResponse {
    languages: Vec<String>,
}

pub async fn languages(State(state): State<Arc<AppState>>) -> Json<LanguagesResponse> {
    let mut language_codes = std::collections::HashSet::new();

    for (from_lang, to_lang) in &state.models {
        if let Some(from_code) = from_lang.to_639_1() {
            language_codes.insert(from_code.to_string());
        }
        if let Some(to_code) = to_lang.to_639_1() {
            language_codes.insert(to_code.to_string());
        }
    }

    let mut languages: Vec<String> = language_codes.into_iter().collect();
    languages.sort();

    Json(LanguagesResponse { languages })
}

// Heartbeat endpoints
pub async fn heartbeat() -> &'static str {
    "Ready"
}

pub async fn lbheartbeat() -> &'static str {
    "Ready"
}

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

// Modified translate endpoint
#[derive(Debug, Deserialize)]
pub struct TranslationRequest {
    text: String,
    from: Option<String>,
    to: String,
}

#[derive(Debug, Serialize)]
pub struct TranslationResponse {
    text: String,
    from: String,
    to: String,
    result: String,
}

pub async fn translate(
    State(state): State<Arc<AppState>>,
    Json(request): Json<TranslationRequest>,
) -> Result<Json<TranslationResponse>, AppError> {
    let (text, from_lang, to_lang) =
        perform_translation(&state, &request.text, request.from, &request.to).await?;

    Ok(Json(TranslationResponse {
        text: text.clone(),
        from: from_lang,
        to: to_lang,
        result: text,
    }))
}

// Batch translate endpoint
#[derive(Debug, Deserialize)]
pub struct BatchTranslationRequest {
    texts: Vec<String>,
    from: Option<String>,
    to: String,
}

#[derive(Debug, Serialize)]
pub struct BatchTranslationResponse {
    results: Vec<String>,
}

pub async fn translate_batch(
    State(state): State<Arc<AppState>>,
    Json(request): Json<BatchTranslationRequest>,
) -> Result<Json<BatchTranslationResponse>, AppError> {
    let mut results = Vec::with_capacity(request.texts.len());

    for text in request.texts {
        let (translated_text, _from_lang, _to_lang) =
            perform_translation(&state, &text, request.from.clone(), &request.to).await?;
        results.push(translated_text);
    }

    Ok(Json(BatchTranslationResponse { results }))
}

// Google Translate compatible endpoint
#[derive(Debug, Deserialize)]
pub struct GoogleTranslateRequest {
    q: String,
    source: Option<String>,
    target: String,
    format: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GoogleTranslateItem {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

#[derive(Debug, Serialize)]
pub struct GoogleTranslateData {
    translations: Vec<GoogleTranslateItem>,
}

#[derive(Debug, Serialize)]
pub struct GoogleTranslateResponse {
    data: GoogleTranslateData,
}

pub async fn translate_google(
    State(state): State<Arc<AppState>>,
    Json(request): Json<GoogleTranslateRequest>,
) -> Result<Json<GoogleTranslateResponse>, AppError> {
    let (translated_text, _from_lang, _to_lang) =
        perform_translation(&state, &request.q, request.source, &request.target).await?;

    Ok(Json(GoogleTranslateResponse {
        data: GoogleTranslateData {
            translations: vec![GoogleTranslateItem { translated_text }],
        },
    }))
}

// Original endpoint handlers for backward compatibility
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
    let mut translations = Vec::with_capacity(request.text_list.len());

    for text in request.text_list {
        let (translated_text, from_lang, _) = perform_translation(
            &state,
            &text,
            request.source_lang.clone(),
            &request.target_lang,
        )
        .await?;

        translations.push(ImmersiveTranslationItem {
            detected_source_lang: from_lang,
            text: translated_text,
        });
    }

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
        &[("中文(简体)", "zh"), ("英语", "en"), ("日语", "jp")];

    fn convert_language_name(lang: &str) -> String {
        LANGUAGE_CODE_MAP
            .iter()
            .find(|&&(name, _)| name == lang)
            .map(|&(_, code)| code)
            .unwrap_or_else(|| lang)
            .to_string()
    }

    fn get_language_name(code: &str) -> String {
        LANGUAGE_CODE_MAP
            .iter()
            .find(|&&(_, c)| c == code)
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
    source_lang: String,
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
        Some(request.source_lang.to_lowercase()),
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
