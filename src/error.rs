use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("API Error: {0}")]
    ApiError(#[from] reqwest::Error),

    #[error("Database Error: {0}")]
    DbError(#[from] sqlx::Error),

    #[error("Environment Error: {0}")]
    EnvError(#[from] std::env::VarError),

    #[error("I/O Error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Parsing Error: {0}")]
    ParsingError(String),

    #[error("CLI Error: {0}")]
    CliError(String),

    #[error("Configuration Error: {0}")]
    ConfigError(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
