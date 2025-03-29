//! Main entry point for the Air Quality Analysis CLI application.
//!
//! This module sets up logging, initializes the application state,
//! and runs the main interactive command loop, handling user input
//! and dispatching commands to the appropriate modules.

mod api;
mod cli;
mod db;
mod error;
mod models;

use cli::{App, AppState, AverageArgs, Commands, MeasurementsArgs};
use colored::*;
use dialoguer::{theme::ColorfulTheme, Select};
use error::Result;
use tracing::{error, info, Level};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

#[tokio::main]
async fn main() -> Result<()> {
    // --- File Logging Setup ---
    let file_appender = tracing_appender::rolling::daily("logs", "app.log");
    let (non_blocking_appender, _guard) = tracing_appender::non_blocking(file_appender);

    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let env_filter =
        EnvFilter::try_new(&log_level).unwrap_or_else(|_| EnvFilter::new(Level::INFO.to_string()));

    // Layer for file logging (INFO and above)
    let file_layer = fmt::layer()
        .with_writer(non_blocking_appender)
        .with_ansi(false);

    // Layer for console output (disabled for normal operation, only shows panics)
    let console_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_filter(EnvFilter::new("off")); // Use "off" directive to disable console logging

    // Combine layers and initialize
    tracing_subscriber::registry()
        .with(env_filter) // Global filter (applies to file layer)
        .with(file_layer)
        .with(console_layer) // Console layer filtered to OFF
        .init();
    // --- End File Logging Setup ---

    info!("Initializing air quality analysis app...");

    let app = match App::new().await {
        Ok(app) => {
            info!("Application initialized successfully.");
            app
        },
        Err(e) => {
            error!("Failed to initialize application: {:?}", e); // Goes to file and console (if WARN/ERROR)
            println!(
                "{}",
                "Error: Failed to initialize application. Check logs.".red() // User-facing error
            );
            return Err(e);
        },
    };

    println!(
        "{}",
        "Welcome to the Air Quality Analysis CLI!".cyan().bold()
    );

    // Main interactive loop
    loop {
        // Get current state
        let current_state = app.get_state().await;
        info!("Current state for menu: {:?}", current_state);

        // Build options dynamically based on state
        let mut options = Vec::new();
        match current_state {
            AppState::Uninitialized => {
                options.push("Initialize Database Schema");
            },
            AppState::DbInitialized => {
                options.push("Re-initialize Database Schema");
                options.push("Import Data");
            },
            AppState::DataImported => {
                options.push("Re-initialize Database Schema");
                options.push("Re-import Data");
                options.push("Find Most Polluted Country");
                options.push("Calculate Average Air Quality");
                options.push("Get Measurements by City");
            },
        }
        options.push("Exit");

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What would you like to do?")
            .items(&options)
            .default(0)
            .interact_opt()?
            .unwrap_or(options.len() - 1);

        println!("\n---\n");

        // Map selection back to command based on current state
        let command_to_run = match current_state {
            AppState::Uninitialized => match selection {
                0 => Some(Commands::InitDb),
                1 => None, // Exit
                _ => unreachable!(),
            },
            AppState::DbInitialized => match selection {
                0 => Some(Commands::InitDb), // Re-initialize
                1 => match cli::prompt_days() {
                    Ok(days) => Some(Commands::Import { days }),
                    Err(e) => {
                        println!("{} {}", "Failed to get input:".red(), e);
                        None
                    },
                },
                2 => None, // Exit
                _ => unreachable!(),
            },
            AppState::DataImported => match selection {
                0 => Some(Commands::InitDb), // Re-initialize
                1 => match cli::prompt_days() {
                    Ok(days) => Some(Commands::Import { days }),
                    Err(e) => {
                        println!("{} {}", "Failed to get input:".red(), e);
                        None
                    },
                },
                2 => Some(Commands::MostPolluted),
                3 => {
                    let country = match cli::prompt_country() {
                        Ok(c) => c,
                        Err(e) => {
                            println!("{} {}", "Failed to get country:".red(), e);
                            continue;
                        },
                    };
                    // Remove prompt_days() call and days field
                    Some(Commands::Average(AverageArgs { country }))
                },
                4 => match cli::prompt_country() {
                    Ok(country) => Some(Commands::Measurements(MeasurementsArgs { country })),
                    Err(e) => {
                        println!("{} {}", "Failed to get country:".red(), e);
                        None
                    },
                },
                5 => None, // Exit
                _ => unreachable!(),
            },
        };

        // Execute the command if one was determined
        if let Some(command) = command_to_run {
            let command_result = app.run_command(command).await;
            if let Err(e) = command_result {
                error!("Command execution failed: {:?}", e); // Goes to file only (unless RUST_LOG overrides)
                println!(
                    "{} {}",
                    "Error executing command:".red(), // User-facing error
                    e.to_string().red()
                );
            }
        } else if selection == options.len() - 1 {
            println!("{}", "Exiting application. Goodbye!".green());
            break;
        }

        println!("\n---\n");
    }

    Ok(())
}
