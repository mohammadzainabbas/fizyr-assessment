mod api;
mod cli;
mod db;
mod error;
mod models;

use clap::Parser;
use cli::{App, Cli};
use error::Result;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    info!("Starting air quality analysis CLI");

    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize and run the application
    match App::new().await {
        Ok(app) => {
            if let Err(e) = app.run(cli).await {
                error!("Application error: {:?}", e);
                return Err(e);
            }
        },
        Err(e) => {
            error!("Failed to initialize application: {:?}", e);
            return Err(e);
        },
    }

    info!("Application completed successfully");
    Ok(())
}
