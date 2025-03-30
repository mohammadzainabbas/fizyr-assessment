//! Main entry point and interactive loop for the Air Quality Analysis CLI.
//!
//! Initializes logging, application state (including API client and DB connection),
//! and runs the main menu loop, dispatching user-selected commands.

mod api;
mod cli;
mod db;
mod error;
mod models;

use cli::{App, AppState, AverageArgs, Commands, MeasurementsByLocalityArgs}; // Renamed Args struct
use colored::*;
use dialoguer::{theme::ColorfulTheme, Select};
use error::Result;
use tracing::{error, info, Level};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// Main asynchronous function to run the CLI application.
///
/// Sets up logging, initializes the application, and enters the main interactive loop
/// to process user commands until exit.
#[tokio::main]
async fn main() -> Result<()> {
    // Configure rolling file logging to `logs/app.log`
    let file_appender = tracing_appender::rolling::daily("logs", "app.log");
    let (non_blocking_appender, _guard) = tracing_appender::non_blocking(file_appender); // _guard ensures logs are flushed on exit

    // Determine log level from RUST_LOG env var, defaulting to INFO
    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let env_filter =
        EnvFilter::try_new(&log_level).unwrap_or_else(|_| EnvFilter::new(Level::INFO.to_string()));

    // Layer for writing logs to the rolling file (without ANSI colors)
    let file_layer = fmt::layer()
        .with_writer(non_blocking_appender)
        .with_ansi(false);

    // Layer for console output, filtered to OFF by default to avoid cluttering the UI.
    // Panics and explicit `eprintln!` will still appear on stderr.
    // Can be overridden by RUST_LOG (e.g., `RUST_LOG=warn cargo run`).
    let console_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_filter(EnvFilter::new("off"));

    // Initialize the tracing subscriber with the configured layers and filter
    tracing_subscriber::registry()
        .with(env_filter) // Apply the global RUST_LOG filter first
        .with(file_layer)
        .with(console_layer)
        .init();

    info!("Initializing air quality analysis app...");

    // Initialize the main application struct, handling potential errors
    let app = match App::new().await {
        Ok(app) => {
            info!("Application initialized successfully.");
            app
        },
        Err(e) => {
            // Log detailed error to file (and console if RUST_LOG allows)
            error!("Failed to initialize application: {:?}", e);
            // Print user-friendly error message to console
            println!(
                "{}",
                "Error: Failed to initialize application. Check logs/app.log for details.".red()
            );
            return Err(e); // Exit the application
        },
    };

    // Display welcome message
    println!(
        "{}",
        "Welcome to the Air Quality Analysis CLI!".cyan().bold()
    );

    // --- Main Interactive Application Loop ---
    loop {
        // Determine the current state to show appropriate menu options
        let current_state = app.get_state().await;
        info!("Current state for menu: {:?}", current_state);

        // Build menu options dynamically based on the application state
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
                options.push("Get Measurements by Locality"); // Updated menu text
            },
        }
        options.push("Exit"); // Always add Exit option

        // Prompt user with the interactive menu
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What would you like to do?")
            .items(&options)
            .default(0)
            .interact_opt()? // Allow optional interaction (e.g., Ctrl+C)
            .unwrap_or(options.len() - 1); // Default to Exit if interaction is cancelled

        println!("\n---\n"); // Separator for clarity

        // Map the user's menu selection to a specific command based on the current state
        let command_to_run = match current_state {
            AppState::Uninitialized => match selection {
                0 => Some(Commands::InitDb),
                1 => None, // Exit
                _ => unreachable!(),
            },
            AppState::DbInitialized => match selection {
                0 => Some(Commands::InitDb),
                1 => match cli::prompt_days() {
                    Ok(days) => Some(Commands::Import { days }),
                    Err(e) => {
                        println!("{} {}", "Failed to get input:".red(), e);
                        None // Don't run a command if input fails
                    },
                },
                2 => None, // Exit
                _ => unreachable!(),
            },
            AppState::DataImported => match selection {
                0 => Some(Commands::InitDb),
                1 => match cli::prompt_days() {
                    Ok(days) => Some(Commands::Import { days }),
                    Err(e) => {
                        println!("{} {}", "Failed to get input:".red(), e);
                        None
                    },
                },
                2 => Some(Commands::MostPolluted),
                3 => {
                    // Prompt for country needed for Average command
                    match cli::prompt_country() {
                        Ok(country) => Some(Commands::Average(AverageArgs { country })),
                        Err(e) => {
                            println!("{} {}", "Failed to get country:".red(), e);
                            continue; // Re-prompt if country selection fails
                        },
                    }
                },
                4 => {
                    // Prompt for country needed for Measurements command
                    match cli::prompt_country() {
                        Ok(country) => Some(Commands::MeasurementsByLocality(
                            MeasurementsByLocalityArgs { country },
                        )), // Renamed variant and args struct
                        Err(e) => {
                            println!("{} {}", "Failed to get country:".red(), e);
                            None
                        },
                    }
                },
                5 => None, // Exit
                _ => unreachable!(),
            },
        };

        // Execute the selected command, if any
        if let Some(command) = command_to_run {
            let command_result = app.run_command(command).await;
            // Handle potential errors during command execution
            if let Err(e) = command_result {
                error!("Command execution failed: {:?}", e); // Log detailed error
                println!(
                    "{} {}",
                    "Error executing command:".red(), // Show user-friendly error
                    e.to_string().red()
                );
            }
        } else if selection == options.len() - 1 {
            // If no command was run and the selection was the last item (Exit)
            println!("{}", "Exiting application. Goodbye!".green());
            break; // Exit the main loop
        }

        println!("\n---\n"); // Separator before next loop iteration
    }

    Ok(()) // Indicate successful application termination
}
