mod api;
mod cli;
mod db;
mod error;
mod models;

use cli::{App, AverageArgs, Commands, MeasurementsArgs}; // Import necessary items
use colored::*;
use dialoguer::{theme::ColorfulTheme, Select};
use error::Result;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    info!("Initializing air quality analysis app...");

    // Initialize the application state (DB connection, API client)
    let app = match App::new().await {
        Ok(app) => {
            info!("Application initialized successfully.");
            app
        },
        Err(e) => {
            error!("Failed to initialize application: {:?}", e);
            println!(
                "{}",
                "Error: Failed to initialize application. Check logs.".red()
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
        let options = &[
            "Initialize Database Schema",
            "Import Recent Air Quality Data",
            "Find Most Polluted Country",
            "Calculate Average Air Quality for a Country",
            "Get All Measurements for a Country",
            "Exit",
        ];

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What would you like to do?")
            .items(options)
            .default(0)
            .interact_opt()? // Use interact_opt to handle potential cancellation (e.g., Ctrl+C)
            .unwrap_or(options.len() - 1); // Default to Exit if cancelled

        // Clear the screen or add spacing for better readability (optional)
        // print!("\x1B[2J\x1B[1;1H"); // Clears screen - might be too aggressive
        println!("\n---\n"); // Add spacing

        // Handle the user's choice
        let command_result = match selection {
            0 => app.run_command(Commands::InitDb).await,
            1 => {
                // Prompt for days
                match cli::prompt_days() {
                    Ok(days) => app.run_command(Commands::Import { days }).await,
                    Err(e) => {
                        println!("{} {}", "Failed to get input:".red(), e);
                        continue; // Skip command execution, go to next loop iteration
                    },
                }
            },
            2 => app.run_command(Commands::MostPolluted).await,
            3 => {
                // Prompt for country and days
                let country = match cli::prompt_country() {
                    Ok(c) => c,
                    Err(e) => {
                        println!("{} {}", "Failed to get country:".red(), e);
                        continue;
                    },
                };
                let days = match cli::prompt_days() {
                    Ok(d) => d,
                    Err(e) => {
                        println!("{} {}", "Failed to get days:".red(), e);
                        continue;
                    },
                };
                app.run_command(Commands::Average(AverageArgs { country, days }))
                    .await
            },
            4 => {
                // Prompt for country
                match cli::prompt_country() {
                    Ok(country) => {
                        app.run_command(Commands::Measurements(MeasurementsArgs { country }))
                            .await
                    },
                    Err(e) => {
                        println!("{} {}", "Failed to get country:".red(), e);
                        continue;
                    },
                }
            },
            5 => {
                println!("{}", "Exiting application. Goodbye!".green());
                break; // Exit the loop
            },
            _ => unreachable!(), // Should not happen with the current setup
        };

        // Handle potential errors from command execution
        if let Err(e) = command_result {
            error!("Command execution failed: {:?}", e);
            println!(
                "{} {}",
                "Error executing command:".red(),
                e.to_string().red()
            );
        }

        println!("\n---\n"); // Add spacing before next prompt
    }

    Ok(())
}
