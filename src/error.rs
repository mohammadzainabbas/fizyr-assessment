//! Defines the application's primary error type `AppError` and a convenience `Result` alias.
//!
//! Uses the `thiserror` crate for ergonomic error definition and provides `From`
//! implementations to convert common external errors into `AppError` variants.
//! Errors that do not implement `Clone` are wrapped in `Arc` to allow `AppError` to be cloneable.

use std::sync::Arc;
use thiserror::Error;

/// The primary error enumeration for all application-specific errors.
#[derive(Error, Debug, Clone)]
pub enum AppError {
    /// Error originating from the OpenAQ API client (`reqwest`).
    #[error("API Error: {0}")]
    Api(Arc<reqwest::Error>),

    /// Error originating from database operations (`sqlx`).
    #[error("Database Error: {0}")]
    Db(Arc<sqlx::Error>),

    /// Error during JSON parsing (`serde_json`). Wrapped in Arc as serde_json::Error is not Clone.
    #[error("JSON Parsing Error: {0}")]
    JsonParse(Arc<serde_json::Error>), // Wrapped in Arc

    /// Error related to accessing environment variables.
    #[error("Environment Error: {0}")]
    Env(#[from] std::env::VarError),

    /// Error related to standard I/O operations.
    #[error("I/O Error: {0}")]
    Io(Arc<std::io::Error>),

    /// Error specific to CLI logic or argument handling.
    #[error("CLI Error: {0}")]
    Cli(String),

    /// Error originating from user interaction prompts (`dialoguer`).
    #[error("Dialoguer Error: {0}")]
    Dialoguer(Arc<dialoguer::Error>),

    /// Error related to progress bar style templating (`indicatif`).
    #[error("Progress Style Template Error: {0}")]
    Template(Arc<indicatif::style::TemplateError>),
}

/// A specialized `Result` type using the application's `AppError`.
pub type Result<T> = std::result::Result<T, AppError>;

// --- From implementations ---
// These allow easy conversion from external error types into AppError
// using the `?` operator. Arc is used for non-Clone error types.

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

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::JsonParse(Arc::new(err))
    }
}
// Removed nested impl block from here
