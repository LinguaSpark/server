pub mod translation;
pub mod endpoint;

// Re-export everything from translation module
pub use translation::*;

// Re-export main types for testing
pub use isolang::Language;
pub use linguaspark::Translator;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Translation error: {0}")]
    TranslationError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Translator error: {0}")]
    TranslatorError(#[from] linguaspark::TranslatorError),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

pub struct AppState {
    pub translator: Translator,
    pub models: Vec<(Language, Language)>,
}
