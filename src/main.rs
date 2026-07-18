use anyhow::Context;
use axum::{
    Router,
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use isolang::Language;
use std::{
    fs, io,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};
use tokio::{net::TcpListener, signal};
use tower_http::{
    cors::{
        AllowCredentials, AllowHeaders, AllowMethods, AllowOrigin, AllowPrivateNetwork, CorsLayer,
    },
    trace::TraceLayer,
};
use tracing::{debug, info};

mod endpoint;
mod inference;
mod translation;

const ENV_MODELS_PATH: &str = "MODELS_DIR";
// Internal tuning override, intentionally not documented for regular users.
// When unset, the server adapts to the system's available CPU parallelism.
const ENV_NUM_WORKERS: &str = "NUM_WORKERS";
const ENV_SERVER_IP: &str = "IP";
const ENV_SERVER_PORT: &str = "PORT";
const ENV_API_KEY: &str = "API_KEY";
const ENV_LOG_LEVEL: &str = "RUST_LOG";

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("Translation error: {0}")]
    TranslationError(String),

    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Inference error: {0}")]
    InferenceError(String),

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
            AppError::InferenceError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::ConfigError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

struct AppState {
    inference: inference::InferenceEngine,
    sole_language_pair: Option<(Language, Language)>,
    api_key: Option<String>,
}

fn resolve_num_workers(
    value: Option<&str>,
    available_parallelism: Option<usize>,
) -> Result<usize, AppError> {
    match value.filter(|value| !value.is_empty()) {
        Some(value) => value
            .parse::<usize>()
            .ok()
            .filter(|&workers| workers > 0)
            .ok_or_else(|| {
                AppError::ConfigError(format!(
                    "NUM_WORKERS must be a positive integer, got '{value}'"
                ))
            }),
        None => Ok(available_parallelism.unwrap_or(1)),
    }
}

async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, AppError> {
    if let Some(expected_key) = state.api_key.as_deref() {
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

        if header_key != Some(expected_key) && query_key != Some(expected_key) {
            debug!("Invalid API key");
            return Err(AppError::Unauthorized);
        }
    }
    Ok(next.run(request).await)
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
    if std::env::var(ENV_LOG_LEVEL).is_err() {
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

    let configured_workers = std::env::var(ENV_NUM_WORKERS).ok();
    let available_parallelism = if configured_workers.as_deref().is_none_or(str::is_empty) {
        std::thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .map_err(|error| {
                tracing::warn!(
                    "Failed to detect available CPU parallelism: {error}; using 1 worker"
                );
                error
            })
            .ok()
    } else {
        None
    };
    let num_workers = resolve_num_workers(configured_workers.as_deref(), available_parallelism)?;

    let server_ip = std::env::var(ENV_SERVER_IP).unwrap_or_else(|_| "127.0.0.1".to_string());
    let server_ip: IpAddr = server_ip
        .parse()
        .context(format!("Invalid {} value: '{}'", ENV_SERVER_IP, server_ip))?;
    let server_port = match std::env::var(ENV_SERVER_PORT) {
        Ok(port) => port
            .parse::<u16>()
            .context(format!("Invalid {} value: '{}'", ENV_SERVER_PORT, port))?,
        Err(std::env::VarError::NotPresent) => 3000,
        Err(error) => return Err(error).context(format!("Failed to read {ENV_SERVER_PORT}")),
    };
    let addr = SocketAddr::new(server_ip, server_port);
    let api_key = std::env::var(ENV_API_KEY)
        .ok()
        .filter(|key| !key.is_empty());

    info!("Loading translation models from {}", models_dir.display());
    let inference = inference::InferenceEngine::load(&models_dir, num_workers)
        .context("Failed to load translation models")?;
    let sole_language_pair = inference.sole_language_pair();

    let app_state = Arc::new(AppState {
        inference,
        sole_language_pair,
        api_key,
    });

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_credentials(AllowCredentials::yes())
        .allow_methods(AllowMethods::mirror_request())
        .allow_headers(AllowHeaders::mirror_request())
        .allow_private_network(AllowPrivateNetwork::yes());

    let protected_routes = Router::new()
        .route("/translate", post(endpoint::translate))
        .route("/kiss", post(endpoint::translate_kiss))
        .route("/imme", post(endpoint::translate_immersive))
        .route("/hcfy", post(endpoint::translate_hcfy))
        .route("/deeplx", post(endpoint::translate_deeplx))
        .route("/detect", post(endpoint::detect_language))
        .route_layer(middleware::from_fn_with_state(
            Arc::clone(&app_state),
            auth_middleware,
        ));

    let app = Router::new()
        .route(
            "/health",
            get(async || {
                Json(serde_json::json!({
                    "status": "ok",
                }))
            }),
        )
        .merge(protected_routes)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(app_state);

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
