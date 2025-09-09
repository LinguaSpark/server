use anyhow::Context;
use axum::{
    Router,
    extract::Json,
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use isolang::Language;
use linguaspark::Translator;
use std::{fs, io, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{net::TcpListener, signal};
use tower_http::{
    cors::{
        AllowCredentials, AllowHeaders, AllowMethods, AllowOrigin, AllowPrivateNetwork, Any,
        CorsLayer,
    },
    trace::TraceLayer,
};
use tracing::{debug, error, info};

mod endpoint;
mod translation;

const ENV_MODELS_PATH: &str = "MODELS_DIR";
const ENV_NUM_WORKERS: &str = "NUM_WORKERS";
const ENV_SERVER_IP: &str = "IP";
const ENV_SERVER_PORT: &str = "PORT";
const ENV_API_KEY: &str = "API_KEY";
const ENV_LOG_LEVEL: &str = "RUST_LOG";

// Environment variable aliases
const ENV_API_KEY_ALIAS: &str = "CORE_API_TOKEN";
const ENV_SERVER_PORT_ALIAS: &str = "CORE_PORT";
const ENV_LOG_LEVEL_ALIAS: &str = "CORE_LOG_LEVEL";
const ENV_NUM_WORKERS_ALIAS: &str = "CORE_NUM_WORKERS";

/// Helper function to get environment variable with alias support
fn get_env_with_alias(primary: &str, alias: &str) -> Option<String> {
    std::env::var(primary)
        .ok()
        .or_else(|| std::env::var(alias).ok())
}

/// Helper function to get environment variable with default value and alias support
fn get_env_with_alias_or_default(primary: &str, alias: &str, default: &str) -> String {
    get_env_with_alias(primary, alias).unwrap_or_else(|| default.to_string())
}

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("Translation error: {0}")]
    TranslationError(String),

    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Translator error: {0}")]
    TranslatorError(#[from] linguaspark::TranslatorError),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::TranslationError(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::IoError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "Invalid or missing API key".to_string(),
            ),
            AppError::TranslatorError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::ConfigError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

struct AppState {
    translator: Translator,
    models: Vec<(Language, Language)>,
}

async fn auth_middleware(
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, AppError> {
    let expected_key = get_env_with_alias(ENV_API_KEY, ENV_API_KEY_ALIAS).unwrap_or_default();

    if !expected_key.is_empty() {
        let header_key = headers
            .get("Authorization")
            .and_then(|header| header.to_str().ok())
            .and_then(|auth| auth.strip_prefix("Bearer "));

        let query_key = request.uri().query().and_then(|query| {
            query.split('&').find_map(|pair| {
                let mut parts = pair.split('=');
                if let Some("token") = parts.next() {
                    parts.next()
                } else {
                    None
                }
            })
        });

        if header_key != Some(&expected_key) && query_key != Some(&expected_key) {
            debug!("Invalid API key");
            return Err(AppError::Unauthorized);
        }
    }
    Ok(next.run(request).await)
}

fn load_models_manually(
    translator: &Translator,
    models_dir: &PathBuf,
) -> Result<Vec<(Language, Language)>, AppError> {
    let mut models = Vec::new();

    for entry in fs::read_dir(models_dir)? {
        let entry = entry?;
        let model_dir_path = entry.path();
        let language_pair = entry.file_name().to_string_lossy().into_owned();

        info!("Looking for models in {}", model_dir_path.display());
        translator.load_model(&language_pair, model_dir_path)?;

        if language_pair.len() >= 4 {
            let from_lang = translation::parse_language_code(&language_pair[0..2])?;
            let to_lang = translation::parse_language_code(&language_pair[2..4])?;
            models.push((from_lang, to_lang));
        } else {
            return Err(AppError::ConfigError(format!(
                "Invalid language pair format: '{}'. Expected format like 'enzh', 'jpen'",
                language_pair
            )));
        }

        info!("Loaded model for language pair '{}'", language_pair);
    }

    Ok(models)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, shutting down gracefully...");
        },
        _ = terminate => {
            info!("Received SIGTERM, shutting down gracefully...");
        },
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Check for log level with alias support
    let log_level = get_env_with_alias(ENV_LOG_LEVEL, ENV_LOG_LEVEL_ALIAS);
    if log_level.is_none() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();
    } else {
        tracing_subscriber::fmt::init();
    }

    let models_dir = std::env::var(ENV_MODELS_PATH)
        .map(PathBuf::from)
        .context(format!(
            "Failed to get environment variable {}",
            ENV_MODELS_PATH
        ))
        .unwrap_or_else(|_| {
            let default_dir = PathBuf::from("models");
            if !default_dir.exists() {
                fs::create_dir_all(&default_dir)
                    .expect("Failed to create default models directory");
            }
            default_dir
        });

    let num_workers = get_env_with_alias(ENV_NUM_WORKERS, ENV_NUM_WORKERS_ALIAS)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);

    let server_ip = std::env::var(ENV_SERVER_IP).unwrap_or_else(|_| "127.0.0.1".to_string());
    let server_port = get_env_with_alias(ENV_SERVER_PORT, ENV_SERVER_PORT_ALIAS)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(3000);

    let server_address = format!("{}:{}", server_ip, server_port);

    info!("Initializing translator with {} workers", num_workers);
    let translator = Translator::new(num_workers).context("Failed to initialize translator")?;

    info!("Loading translation models from {}", models_dir.display());
    let models = load_models_manually(&translator, &models_dir)
        .context("Failed to load translation models")?;

    let app_state = Arc::new(AppState { translator, models });

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_credentials(AllowCredentials::yes())
        .allow_methods(AllowMethods::mirror_request())
        .allow_headers(AllowHeaders::mirror_request())
        .allow_private_network(AllowPrivateNetwork::yes());

    // Routes that require authentication
    let authenticated_routes = Router::new()
        .route("/translate", post(endpoint::translate))
        .route("/translate/batch", post(endpoint::translate_batch))
        .route("/languages", get(endpoint::languages))
        .route("/language/translate/v2", post(endpoint::translate_google))
        .route("/kiss", post(endpoint::translate_kiss))
        .route("/imme", post(endpoint::translate_immersive))
        .route("/hcfy", post(endpoint::translate_hcfy))
        .route("/deeplx", post(endpoint::translate_deeplx))
        .route("/detect", post(endpoint::detect_language))
        .route_layer(middleware::from_fn(auth_middleware));

    // Routes that don't require authentication
    let public_routes = Router::new()
        .route("/version", get(endpoint::version))
        .route(
            "/health",
            get(async || {
                Json(serde_json::json!({
                    "status": "ok",
                }))
            }),
        )
        .route("/__heartbeat__", get(endpoint::heartbeat))
        .route("/__lbheartbeat__", get(endpoint::lbheartbeat));

    let app = Router::new()
        .merge(authenticated_routes)
        .merge(public_routes)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(app_state);

    let addr: SocketAddr = server_address.parse().context(format!(
        "Failed to parse server address: {}",
        server_address
    ))?;
    info!(
        "Starting server on {} (IP: {}, Port: {})",
        addr, server_ip, server_port
    );
    let listener = TcpListener::bind(addr)
        .await
        .context(format!("Failed to bind to address: {}", addr))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

    info!("Server has been shut down gracefully");
    Ok(())
}
