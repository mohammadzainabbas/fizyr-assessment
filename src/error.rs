use std::sync::Arc; // Import Arc
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum AppError {
    #[error("API Error: {0}")]
    Api(Arc<reqwest::Error>), // Renamed

    #[error("Database Error: {0}")]
    Db(Arc<sqlx::Error>), // Renamed

    #[error("Environment Error: {0}")]
    Env(#[from] std::env::VarError), // Renamed

    #[error("I/O Error: {0}")]
    Io(Arc<std::io::Error>), // Renamed

    #[error("CLI Error: {0}")]
    Cli(String), // Renamed

    #[error("Configuration Error: {0}")]
    Config(String), // Renamed

    #[error("Dialoguer Error: {0}")]
    Dialoguer(Arc<dialoguer::Error>), // Renamed

    #[error("Progress Style Template Error: {0}")]
    Template(Arc<indicatif::style::TemplateError>), // Renamed
}

pub type Result<T> = std::result::Result<T, AppError>;

// Manual From implementations for Arc-wrapped errors (Update variant names)

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::Api(Arc::new(err)) // Updated variant name
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Db(Arc::new(err)) // Updated variant name
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(Arc::new(err)) // Updated variant name
    }
}

impl From<dialoguer::Error> for AppError {
    fn from(err: dialoguer::Error) -> Self {
        AppError::Dialoguer(Arc::new(err)) // Updated variant name
    }
}

impl From<indicatif::style::TemplateError> for AppError {
    fn from(err: indicatif::style::TemplateError) -> Self {
        AppError::Template(Arc::new(err)) // Updated variant name
    }
}
