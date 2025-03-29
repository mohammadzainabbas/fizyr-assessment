//! Defines the custom error type `AppError` for the application and a `Result` alias.
//!
//! Uses `thiserror` for convenient error definition and provides `From` implementations
//! for common error types encountered in the application, wrapping them in `Arc`
//! where necessary to allow cloning (e.g., for error handling in async contexts).

use std::sync::Arc;
use thiserror::Error;

/// The primary error type for the application.
#[derive(Error, Debug, Clone)]
pub enum AppError {
    /// Error originating from the OpenAQ API client (reqwest).
    #[error("API Error: {0}")]
    Api(Arc<reqwest::Error>),

    /// Error originating from database operations (sqlx).
    #[error("Database Error: {0}")]
    Db(Arc<sqlx::Error>),

    /// Error related to environment variable access.
    #[error("Environment Error: {0}")]
    Env(#[from] std::env::VarError),

    /// Error related to standard I/O operations.
    #[error("I/O Error: {0}")]
    Io(Arc<std::io::Error>),

    /// Error specific to CLI argument parsing or command logic.
    #[error("CLI Error: {0}")]
    Cli(String),

    /// Error originating from user interaction prompts (dialoguer).
    #[error("Dialoguer Error: {0}")]
    Dialoguer(Arc<dialoguer::Error>),

    /// Error related to progress bar style templating (indicatif).
    #[error("Progress Style Template Error: {0}")]
    Template(Arc<indicatif::style::TemplateError>),
}

/// A specialized `Result` type for the application, using `AppError`.
pub type Result<T> = std::result::Result<T, AppError>;

// From implementations to convert underlying errors into AppError variants.
// Arc is used for error types that don't implement Clone themselves.

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::Api(Arc::new(err))
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Db(Arc::new(err))
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(Arc::new(err))
    }
}

impl From<dialoguer::Error> for AppError {
    fn from(err: dialoguer::Error) -> Self {
        AppError::Dialoguer(Arc::new(err))
    }
}

impl From<indicatif::style::TemplateError> for AppError {
    fn from(err: indicatif::style::TemplateError) -> Self {
        AppError::Template(Arc::new(err))
    }
}
