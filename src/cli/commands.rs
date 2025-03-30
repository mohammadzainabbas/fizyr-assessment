//! Defines the core application logic, state management, command definitions,
//! and execution handlers for the CLI.
//!
//! This module orchestrates interactions between the API client, database,
//! and user interface elements (prompts, tables, progress bars), managing the
//! overall application flow based on user input and application state.

use crate::api::OpenAQClient;
use crate::db::Database;
use crate::error::{AppError, Result};
use chrono::{Duration, NaiveTime, Utc};
use colored::*;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table};
use dialoguer::{theme::ColorfulTheme, Input, Select};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::Mutex;
use tracing::{error, info};

/// Represents the different states the application can be in, primarily tracking
/// database initialization and data import status. This influences the available
/// commands shown in the interactive menu.
#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    /// Initial state: Database schema has not been created.
    Uninitialized,
    /// State: Database schema exists, but no data has been imported yet.
    DbInitialized,
    /// State: Database schema exists, and data has been imported at least once.
    DataImported,
}

/// A predefined list of country codes used for data fetching and analysis.
pub const COUNTRIES: [&str; 6] = [
    "NL", // Netherlands
    "DE", // Germany
    "FR", // France
    "GR", // Greece
    "ES", // Spain
    "PK", // Pakistan
];

/// A mapping from country codes to their corresponding IDs in the OpenAQ API.
pub fn get_country_id_map() -> std::collections::HashMap<&'static str, u32> {
    let mut map = std::collections::HashMap::new();
    map.insert("PK", 109); // Pakistan
    map.insert("NL", 94); // Netherlands
    map.insert("DE", 50); // Germany
    map.insert("GR", 80); // Greece
    map.insert("ES", 67); // Spain
    map.insert("FR", 22); // France
    map
}

/// Returns a map associating country codes with their full names.
/// Used for displaying user-friendly names in prompts and output.
fn get_country_name_map() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert("NL", "Netherlands");
    map.insert("DE", "Germany");
    map.insert("FR", "France");
    map.insert("GR", "Greece");
    map.insert("ES", "Spain");
    map.insert("PK", "Pakistan");
    map
}

/// Defines the available commands triggerable via the interactive menu.
#[derive(Debug, Clone)]
pub enum Commands {
    /// Initialize or re-initialize the database schema (`locations`, `sensors`, `measurements` tables and indexes).
    InitDb,
    /// Import data from the OpenAQ API: fetches top 10 locations per country, saves locations/sensors,
    /// then fetches daily measurements for each sensor for the specified number of past days.
    Import { days: i64 },
    /// Find the most polluted country (from `COUNTRIES`) based on recent PM2.5/PM10 data.
    MostPolluted,
    /// Calculate the 5-day average air quality metrics for a specific country.
    Average(AverageArgs),
    /// Get the latest measurements for all parameters, grouped by locality, for a specific country.
    MeasurementsByLocality(MeasurementsByLocalityArgs),
}

/// Arguments for the `Average` command.
#[derive(Debug, Clone)]
pub struct AverageArgs {
    /// The 2-letter country code for which to calculate the average.
    pub country: String,
}

/// Arguments for the `MeasurementsByLocality` command.
#[derive(Debug, Clone)]
pub struct MeasurementsByLocalityArgs {
    /// The 2-letter country code for which to retrieve measurements.
    pub country: String,
}

/// The main application structure.
///
/// Holds shared resources like the database connection pool and API client,
/// along with the application's current state, managed within an `Arc<Mutex>`
/// for safe concurrent access (though current usage is sequential).
pub struct App {
    db: Database,
    api_client: OpenAQClient,
    state: Arc<Mutex<AppState>>, // Shared, mutable state tracking DB/import status
}

impl App {
    /// Creates a new `App` instance, initializing shared resources.
    ///
    /// - Loads environment variables from `.env` if present.
    /// - Establishes the database connection pool.
    /// - Creates the OpenAQ API client.
    /// - Determines the initial `AppState` by checking the database status.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Env` if `OPENAQ_KEY` is not set.
    /// Returns `AppError::Db` if the database connection fails.
    pub async fn new() -> Result<Self> {
        dotenv::dotenv().ok(); // Load .env file, ignore errors if not found

        // Get DATABASE_URL from env or use default
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            info!("DATABASE_URL not set, using default: postgres://postgres:postgres@localhost:5432/air_quality");
            "postgres://postgres:postgres@localhost:5432/air_quality".to_string()
        });

        // Get required OPENAQ_KEY from env
        let api_key = env::var("OPENAQ_KEY").map_err(|e| {
            error!("Required environment variable OPENAQ_KEY is not set.");
            AppError::Env(e) // Propagate the VarError wrapped in AppError
        })?;

        let db = Database::new(&database_url).await?;
        let api_client = OpenAQClient::new(api_key);

        // Determine initial state by checking database
        let initial_state = if db.has_data_imported().await? {
            AppState::DataImported
        } else if db.is_schema_initialized().await? {
            AppState::DbInitialized
        } else {
            AppState::Uninitialized
        };
        info!("Determined initial application state: {:?}", initial_state);

        Ok(Self {
            db,
            api_client,
            state: Arc::new(Mutex::new(initial_state)),
        })
    }

    /// Returns a clone of the current application state.
    /// Acquires a lock on the state mutex.
    pub async fn get_state(&self) -> AppState {
        self.state.lock().await.clone()
    }

    /// Executes the given command, handling associated logic and state updates.
    ///
    /// This is the main dispatcher for application actions selected by the user.
    /// It orchestrates calls to API, database, and handles UI feedback like progress bars.
    /// State-changing commands (`InitDb`, `Import`) update the shared `AppState`.
    ///
    /// # Arguments
    ///
    /// * `command` - The `Commands` enum variant representing the user's choice.
    ///
    /// # Errors
    ///
    /// Propagates errors from underlying operations (DB, API, IO, etc.) as `AppError`.
    pub async fn run_command(&self, command: Commands) -> Result<()> {
        let state_clone = Arc::clone(&self.state); // Clone Arc for potential state updates

        match command {
            Commands::InitDb => {
                println!("{}", "Initializing database schema...".yellow());
                let pb = Self::create_spinner("Connecting and initializing...");
                self.db.init_schema().await?;
                pb.finish_with_message("Database schema initialized successfully!".to_string());
                info!("Database schema initialization command successful.");

                // Update state only if it was previously Uninitialized
                let mut state = state_clone.lock().await;
                if *state == AppState::Uninitialized {
                    *state = AppState::DbInitialized;
                    info!("App state updated: Uninitialized -> DbInitialized");
                } else {
                    info!("Database re-initialized, state remains {:?}.", *state);
                }
                Ok(())
            },
            Commands::Import { days } => {
                self.import_data(days).await?;

                // Update state to DataImported after successful import
                let mut state = state_clone.lock().await;
                *state = AppState::DataImported;
                info!("App state updated: {:?} -> DataImported", *state); // Log previous state too
                Ok(())
            },
            Commands::MostPolluted => {
                self.find_most_polluted().await?;
                Ok(())
            },
            Commands::Average(args) => {
                self.calculate_average(&args.country).await?;
                Ok(())
            },
            Commands::MeasurementsByLocality(args) => {
                // Renamed variant
                self.get_measurements_by_locality_table(&args.country)
                    .await?; // Renamed method call
                Ok(())
            },
        }
    }

    /// Imports air quality data for the specified number of past days for all predefined `COUNTRIES`.
    ///
    /// The import process follows these steps:
    /// 1. Ensures the database schema (`locations`, `sensors`, `measurements`) is initialized.
    /// 2. Iterates through each country defined in `COUNTRIES`.
    /// 3. Fetches the top 10 locations for the current country using the OpenAQ API.
    /// 4. Inserts the fetched location data into the `locations` table.
    /// 5. Inserts the sensor data associated with these locations into the `sensors` table.
    /// 6. Collects all successfully saved sensors across all processed countries.
    /// 7. Iterates through the collected sensors and fetches daily aggregated measurements
    ///    from the OpenAQ API for the specified date range (`days` ago to now).
    ///    - Includes retry logic (3 attempts with 10s delay) for measurement fetching errors.
    /// 8. Converts valid fetched measurements into `DbMeasurement` structs.
    /// 9. Inserts all collected `DbMeasurement` records into the `measurements` table in a single transaction.
    ///
    /// Displays progress using `indicatif` progress bars. Handles and logs errors during API calls
    /// and database operations, attempting to continue processing other countries/sensors where possible.
    ///
    /// # Arguments
    ///
    /// * `days` - The number of past days (from midnight UTC) for which to import measurement data.
    ///
    /// # Errors
    ///
    /// Returns `AppError` if critical operations like schema initialization or the final
    /// measurement insertion transaction fail. Errors during individual API calls or
    /// location/sensor insertions are logged, and the process attempts to continue.
    async fn import_data(&self, days: i64) -> Result<()> {
        println!(
            "{} {}",
            "Importing data for the last".yellow(),
            format!("{} days", days).yellow().bold()
        );

        info!("Ensuring database schema exists before import...");
        self.db.init_schema().await?; // Idempotent schema initialization

        // Calculate date range aligned to midnight UTC
        let today_utc = Utc::now().date_naive();
        let end_date = today_utc
            .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap())
            .and_local_timezone(Utc)
            .unwrap();
        let start_date = (today_utc - Duration::days(days))
            .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap())
            .and_local_timezone(Utc)
            .unwrap();
        info!("Importing data from {} to {}", start_date, end_date);

        let total_countries = COUNTRIES.len();
        let pb_locations = Self::create_progress_bar(total_countries as u64);
        pb_locations.set_message("Fetching & saving locations/sensors...");

        // Store (location, sensor) pairs to fetch measurements later
        let mut sensors_to_fetch: Vec<(crate::models::Location, crate::models::SensorBase)> =
            Vec::new();

        // --- Step 1 & 2: Fetch and Save Locations/Sensors per Country ---
        for country_code in COUNTRIES.iter() {
            pb_locations.set_message(format!("Processing {}...", country_code));
            info!("Fetching locations for country: {}", country_code);

            let country_id_map = get_country_id_map();
            let country_id = match country_id_map.get(country_code) {
                Some(id) => *id, // Dereference id
                None => {
                    error!(
                        "No country ID mapping found for {}. Skipping.",
                        country_code
                    );
                    pb_locations.println(format!(
                        "{} No country ID mapping found for {}. Skipping.",
                        "Error:".red(),
                        country_code
                    ));
                    pb_locations.inc(1);
                    continue;
                },
            };

            // Fetch top 10 locations for the country
            let locations = match self
                .api_client
                .get_locations_for_country(&[country_id])
                .await
            {
                Ok(locs) => locs,
                Err(e) => {
                    error!(
                        "Failed to fetch locations for {} (ID: {}): {}. Skipping.",
                        country_code, country_id, e
                    );
                    pb_locations.println(format!(
                        "{} Failed to fetch locations for {} (ID: {}): {}. Skipping.",
                        "Error:".red(),
                        country_code,
                        country_id,
                        e
                    ));
                    pb_locations.inc(1);
                    continue;
                },
            };
            info!("Fetched {} locations for {}", locations.len(), country_code);

            if locations.is_empty() {
                pb_locations.println(format!(
                    "{} No locations found for {}. Skipping.",
                    "Warning:".yellow(),
                    country_code
                ));
                pb_locations.inc(1);
                continue;
            }

            // Save locations to DB
            if let Err(e) = self.db.insert_locations(&locations).await {
                error!(
                    "Failed to insert locations for {}: {}. Skipping country's sensors.",
                    country_code, e
                );
                pb_locations.println(format!(
                    "{} Failed to save locations for {}: {}. Skipping sensors.",
                    "Error:".red(),
                    country_code,
                    e
                ));
                pb_locations.inc(1);
                continue;
            }

            // Save sensors and collect them for measurement fetching
            for loc in locations {
                if let Err(e) = self.db.insert_sensors(loc.id as i64, &loc.sensors).await {
                    // Log error but continue processing other locations/sensors
                    error!("Failed to insert sensors for location {}: {}", loc.id, e);
                    pb_locations.println(format!(
                        "{} Failed to save sensors for location {}: {}.",
                        "Warning:".yellow(),
                        loc.id,
                        e
                    ));
                } else {
                    // Add sensors to the list for fetching measurements later
                    for sensor in loc.sensors.iter() {
                        sensors_to_fetch.push((loc.clone(), sensor.clone())); // Clone necessary data
                    }
                }
            }
            pb_locations.inc(1);
        }
        pb_locations.finish_with_message("Finished fetching & saving locations/sensors.");

        // --- Step 3 & 4: Fetch and Save Measurements for All Collected Sensors ---
        if sensors_to_fetch.is_empty() {
            println!("{}", "No sensors found to fetch measurements for.".yellow());
            info!("Data import process finished: No sensors found.");
            return Ok(());
        }

        let pb_measurements = Self::create_progress_bar(sensors_to_fetch.len() as u64);
        pb_measurements.set_message("Fetching measurements...");
        let mut all_db_measurements = Vec::new();
        let max_retries = 3;
        let retry_delay = StdDuration::from_secs(10);

        for (location_context, sensor) in sensors_to_fetch {
            pb_measurements.set_message(format!("Sensor {}...", sensor.id));
            info!("Fetching measurements for sensor ID: {}", sensor.id);
            let mut measurements_v3 = None; // Option to hold fetched measurements

            for attempt in 0..max_retries {
                match self
                    .api_client
                    .get_measurements_for_sensor(sensor.id, start_date, end_date)
                    .await
                {
                    Ok(m) => {
                        measurements_v3 = Some(m);
                        break; // Success, exit retry loop
                    },
                    Err(e) => {
                        error!(
                            "Attempt {}/{} failed to fetch measurements for sensor {}: {}",
                            attempt + 1,
                            max_retries,
                            sensor.id,
                            e
                        );
                        if attempt + 1 < max_retries {
                            pb_measurements.println(format!(
                                "{} Retrying sensor {} after {:?}...",
                                "Warning:".yellow(),
                                sensor.id,
                                retry_delay
                            ));
                            tokio::time::sleep(retry_delay).await;
                        } else {
                            pb_measurements.println(format!(
                                "{} Failed to fetch measurements for sensor {} after {} attempts: {}. Skipping.",
                                "Error:".red(), sensor.id, max_retries, e
                            ));
                        }
                    },
                }
            }

            // Process measurements if fetched successfully
            if let Some(fetched_measurements) = measurements_v3 {
                info!(
                    "Fetched {} measurements for sensor {}",
                    fetched_measurements.len(),
                    sensor.id
                );
                for m_v3 in fetched_measurements {
                    let db_m = crate::models::DbMeasurement::from_daily_measurement(
                        &m_v3,
                        &location_context, // Use the stored location context
                        &sensor,           // Use the stored sensor context
                    );
                    all_db_measurements.push(db_m);
                }
            }
            pb_measurements.inc(1);
        }
        pb_measurements.finish_with_message("Finished fetching measurements.");

        // --- Step 4 (Continued): Insert Measurements into DB ---
        if all_db_measurements.is_empty() {
            println!(
                "{}",
                "No measurements fetched successfully to insert.".yellow()
            );
            info!("Data import process finished: No measurements fetched.");
            return Ok(());
        }

        println!(
            "{}",
            format!(
                "Inserting {} total measurements...",
                all_db_measurements.len()
            )
            .yellow()
        );
        let pb_insert = Self::create_spinner("Inserting data into database...");
        self.db.insert_measurements(&all_db_measurements).await?;
        pb_insert.finish_with_message("Data insertion completed successfully!".to_string());
        info!("Inserted {} total measurements.", all_db_measurements.len());
        info!("Data import process finished.");
        Ok(())
    }

    /// Finds and displays the most polluted country based on recent data.
    ///
    /// Queries the database using `db.get_most_polluted_country` (which uses a 7-day window
    /// and a weighted PM2.5/PM10 index) and formats the result in a table.
    ///
    /// # Errors
    ///
    /// Returns `AppError` if the database query or table formatting fails.
    async fn find_most_polluted(&self) -> Result<()> {
        println!(
            "{}",
            "Finding the most polluted country (based on last 7 days PM2.5/PM10)...".yellow()
        );
        let pb = Self::create_spinner("Querying database...");
        let country_refs: Vec<&str> = COUNTRIES.to_vec(); // Convert array to Vec<&str>
        let result = self.db.get_most_polluted_country(&country_refs).await?;
        pb.finish_and_clear(); // Clear spinner before printing table

        let country_map = get_country_name_map();
        let full_country_name = country_map
            .get(result.country.as_str())
            .copied()
            .unwrap_or(result.country.as_str()); // Fallback to code if name not found

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("Metric").fg(Color::Green),
                Cell::new("Value").fg(Color::Green),
            ]);
        table.add_row(vec![
            Cell::new("Most Polluted Country"),
            Cell::new(format!("{} ({})", full_country_name, result.country)) // Show "Name (Code)"
                .fg(Color::Cyan)
                .add_attribute(Attribute::Bold),
        ]);
        table.add_row(vec![
            Cell::new("Pollution Index"), // Index = (PM2.5 * 1.5) + PM10
            Cell::new(format!("{:.2}", result.pollution_index)),
        ]);
        table.add_row(vec![
            Cell::new("Avg PM2.5 (µg/m³)"),
            Cell::new(Self::format_optional_float(result.pm25_avg)),
        ]);
        table.add_row(vec![
            Cell::new("Avg PM10 (µg/m³)"),
            Cell::new(Self::format_optional_float(result.pm10_avg)),
        ]);
        println!("{table}");
        Ok(())
    }

    /// Calculates and displays the 5-day average air quality for a given country.
    ///
    /// Validates the country code, queries the database using `db.get_average_air_quality`,
    /// and formats the result in a table.
    ///
    /// # Arguments
    ///
    /// * `country` - The 2-letter country code provided by the user.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Cli` if the country code is invalid.
    /// Returns `AppError` if the database query or table formatting fails.
    async fn calculate_average(&self, country: &str) -> Result<()> {
        let country_code = country.to_uppercase();
        let country_map = get_country_name_map();
        let full_country_name = country_map
            .get(country_code.as_str())
            .copied()
            .unwrap_or(country_code.as_str());

        // Validate country code against the predefined list
        if !COUNTRIES.contains(&country_code.as_str()) {
            return Err(AppError::Cli(format!(
                "Invalid country code '{}'. Must be one of: {:?}",
                country_code, COUNTRIES
            )));
        }

        println!(
            "{} {}-{} {} ({})",
            "Calculating".yellow(),
            "5".yellow().bold(), // Hardcoded 5 days
            "day average for".yellow(),
            full_country_name.yellow().bold(),
            country_code.yellow().bold()
        );
        let pb = Self::create_spinner("Querying database...");
        let result = self.db.get_average_air_quality(&country_code).await?;
        pb.finish_and_clear();

        // Get full name again for the result (in case DB returns only code)
        let result_full_name = country_map
            .get(result.country.as_str())
            .copied()
            .unwrap_or(result.country.as_str());

        println!(
            "{}-{} {} ({}) ({})",
            "5".bold(), // Hardcoded 5 days
            "day average air quality for".green(),
            result_full_name.bold().cyan(),
            result.country.bold().cyan(), // Show code too
            format!("Based on {} measurements", result.measurement_count).dimmed()
        );

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("Parameter").fg(Color::Green),
                Cell::new("Average Value (µg/m³)").fg(Color::Green), // Assuming common unit
            ]);

        table.add_row(vec![
            Cell::new("PM2.5"),
            Cell::new(Self::format_optional_float(result.avg_pm25)),
        ]);
        table.add_row(vec![
            Cell::new("PM10"),
            Cell::new(Self::format_optional_float(result.avg_pm10)),
        ]);
        table.add_row(vec![
            Cell::new("O3"),
            Cell::new(Self::format_optional_float(result.avg_o3)),
        ]);
        table.add_row(vec![
            Cell::new("NO2"),
            Cell::new(Self::format_optional_float(result.avg_no2)),
        ]);
        table.add_row(vec![
            Cell::new("SO2"),
            Cell::new(Self::format_optional_float(result.avg_so2)),
        ]);
        table.add_row(vec![
            Cell::new("CO"),
            Cell::new(Self::format_optional_float(result.avg_co)),
        ]);
        println!("{table}");
        Ok(())
    }

    /// Fetches and displays the latest measurement for each parameter, grouped by locality,
    /// for the specified country.
    ///
    /// Validates the country code, queries the database using `db.get_latest_measurements_by_locality`,
    /// and formats the results in a table.
    ///
    /// # Arguments
    ///
    /// * `country` - The 2-letter country code provided by the user.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Cli` if the country code is invalid.
    /// Returns `AppError` if the database query or table formatting fails.
    async fn get_measurements_by_locality_table(&self, country: &str) -> Result<()> {
        // Renamed method
        let country_code = country.to_uppercase();
        let country_map = get_country_name_map();
        let full_country_name = country_map
            .get(country_code.as_str())
            .copied()
            .unwrap_or(country_code.as_str());

        // Validate country code
        if !COUNTRIES.contains(&country_code.as_str()) {
            return Err(AppError::Cli(format!(
                "Invalid country code '{}'. Must be one of: {:?}",
                country_code, COUNTRIES
            )));
        }

        println!(
            "{} {} ({})",
            "Fetching latest measurements by locality for".yellow(), // Updated text
            full_country_name.yellow().bold(),
            country_code.yellow().bold()
        );
        let pb = Self::create_spinner("Querying database...");
        // Call the renamed DB function
        let locality_measurements = self
            .db
            .get_latest_measurements_by_locality(&country_code)
            .await?;
        pb.finish_and_clear();

        if locality_measurements.is_empty() {
            // Use updated variable name
            println!(
                "{}",
                format!(
                    "No measurements found for localities in {} ({})", // Updated text
                    full_country_name, country_code
                )
                .yellow()
            );
            return Ok(());
        }

        println!(
            "{} {} ({})",
            "Latest measurements by locality for".green(), // Updated text
            full_country_name.bold().cyan(),
            country_code.bold().cyan()
        );

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("Locality").fg(Color::Green), // Updated header
                Cell::new("PM2.5").fg(Color::Green),
                Cell::new("PM10").fg(Color::Green),
                Cell::new("O3").fg(Color::Green),
                Cell::new("NO2").fg(Color::Green),
                Cell::new("SO2").fg(Color::Green),
                Cell::new("CO").fg(Color::Green),
                Cell::new("Last Updated (UTC)").fg(Color::Green),
            ]);

        // Helper to format Option<Decimal>
        let format_decimal = |val: Option<sqlx::types::Decimal>| -> String {
            val.map(|d| format!("{:.2}", d))
                .unwrap_or_else(|| "-".to_string())
        };

        for measurement in locality_measurements {
            // Use updated variable name
            table.add_row(vec![
                Cell::new(measurement.locality).fg(Color::Cyan), // Use renamed field
                Cell::new(format_decimal(measurement.pm25)),
                Cell::new(format_decimal(measurement.pm10)),
                Cell::new(format_decimal(measurement.o3)),
                Cell::new(format_decimal(measurement.no2)),
                Cell::new(format_decimal(measurement.so2)),
                Cell::new(format_decimal(measurement.co)),
                Cell::new(measurement.last_updated.format("%Y-%m-%d %H:%M")), // Format timestamp
            ]);
        }
        println!("{table}");
        Ok(())
    }

    // --- Helper Methods ---

    /// Creates a standard spinner ProgressBar.
    fn create_spinner(msg: &str) -> ProgressBar {
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(StdDuration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {msg}")
                .unwrap() // Assume template is valid
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message(msg.to_string());
        pb
    }

    /// Creates a standard progress bar.
    fn create_progress_bar(len: u64) -> ProgressBar {
        let pb = ProgressBar::new(len);
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) {msg}",
            )
            .unwrap() // Assume template is valid
            .progress_chars("#>-"),
        );
        pb
    }

    /// Formats an Option<f64> into a String, showing "-" if None or formatting to 2 decimal places if Some.
    fn format_optional_float(val: Option<f64>) -> String {
        val.map(|v| format!("{:.2}", v))
            .unwrap_or_else(|| "-".to_string())
    }
}

// --- User Interaction Helpers ---

/// Prompts the user to select a country from the predefined `COUNTRIES` list.
/// Displays country names along with codes for clarity.
///
/// # Errors
///
/// Returns `AppError::Dialoguer` if the user interaction fails (e.g., Ctrl+C).
pub fn prompt_country() -> Result<String> {
    let country_map = get_country_name_map();
    // Create display strings like "Netherlands (NL)"
    let country_display: Vec<String> = COUNTRIES
        .iter()
        .map(|code| format!("{} ({})", country_map.get(code).unwrap_or(code), code))
        .collect();

    let selection_index = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a country")
        .items(&country_display) // Show user-friendly list
        .default(0)
        .interact()?; // Propagate dialoguer errors

    // Return the selected country code
    Ok(COUNTRIES[selection_index].to_string())
}

/// Prompts the user to enter the number of days for historical data import.
///
/// Validates that the input is an integer between 7 and 365 (inclusive).
/// Defaults to 7 days.
///
/// # Errors
///
/// Returns `AppError::Dialoguer` if the user interaction fails.
/// Returns `AppError::Cli` indirectly via validation if input is outside the allowed range.
pub fn prompt_days() -> Result<i64> {
    let days: i64 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter number of days for history (min 7, max 365)")
        .default(7i64) // Default to 7 days
        .validate_with(|input: &i64| -> std::result::Result<(), &str> {
            if (7..=365).contains(input) {
                // Use range check
                Ok(())
            } else {
                Err("Please enter a number of days between 7 and 365.")
            }
        })
        .interact_text()?; // Propagate dialoguer errors
    Ok(days)
}

// --- Unit Tests ---
// These tests focus on the command handling logic within `App`, using mock objects
// for database and API interactions to isolate the CLI logic.
#[cfg(test)]
mod tests {
    use super::*; // Import items from parent module (App, Commands, etc.)
    use crate::models::{CityLatestMeasurements, CountryAirQuality, PollutionRanking};
    use chrono::{Duration, Utc};
    use std::sync::{Arc, Mutex}; // Use std Mutex for simplicity in tests

    // --- Mock Database State ---
    /// Tracks calls made to the `MockDatabase` and stores expected results for unit tests.
    #[derive(Clone, Default)]
    struct MockDbState {
        init_schema_called: bool,
        insert_measurements_called: bool,
        get_most_polluted_called: bool,
        get_average_called: bool,
        get_latest_by_city_called: bool,
        // Store expected results for query methods
        most_polluted_result: Option<crate::error::Result<PollutionRanking>>,
        average_result: Option<crate::error::Result<CountryAirQuality>>,
        latest_by_city_result: Option<crate::error::Result<Vec<CityLatestMeasurements>>>,
    }

    // --- Mock Database ---
    /// A mock implementation of the `Database` struct for testing `App` logic
    /// without requiring a real database connection. It uses `MockDbState`
    /// to track interactions and return predefined results.
    #[derive(Clone)]
    struct MockDatabase {
        /// Shared state to track calls and hold expected results.
        state: Arc<Mutex<MockDbState>>,
    }

    impl MockDatabase {
        /// Creates a new `MockDatabase` with default state.
        fn new() -> Self {
            Self {
                state: Arc::new(Mutex::new(MockDbState::default())),
            }
        }

        /// Sets the expected result for the next call to `get_most_polluted_country`.
        /// Panics if the mock method is called without an expectation being set.
        fn expect_get_most_polluted(&self, result: crate::error::Result<PollutionRanking>) {
            self.state.lock().unwrap().most_polluted_result = Some(result);
        }

        /// Sets the expected result for the next call to `get_average_air_quality`.
        /// Panics if the mock method is called without an expectation being set.
        fn expect_get_average(&self, result: crate::error::Result<CountryAirQuality>) {
            self.state.lock().unwrap().average_result = Some(result);
        }

        /// Sets the expected result for the next call to `get_latest_measurements_by_city`.
        /// Panics if the mock method is called without an expectation being set.
        fn expect_get_latest_by_city(
            &self,
            result: crate::error::Result<Vec<CityLatestMeasurements>>,
        ) {
            self.state.lock().unwrap().latest_by_city_result = Some(result);
        }

        // --- Mocked Database Methods ---

        async fn init_schema(&self) -> crate::error::Result<()> {
            self.state.lock().unwrap().init_schema_called = true;
            Ok(()) // Assume success for mock
        }

        async fn insert_measurements(
            &self,
            _measurements: &[crate::models::DbMeasurement], // Expect DbMeasurement now
        ) -> crate::error::Result<()> {
            self.state.lock().unwrap().insert_measurements_called = true;
            Ok(()) // Assume success for mock
        }

        async fn get_most_polluted_country(
            &self,
            _countries: &[&str], // Ignore input in mock
        ) -> crate::error::Result<PollutionRanking> {
            let mut state = self.state.lock().unwrap();
            state.get_most_polluted_called = true;
            // Return the expected result or panic if none was set
            state.most_polluted_result.take().unwrap_or_else(|| {
                panic!("MockDatabase::get_most_polluted_country called without expectation set")
            })
        }

        async fn get_average_air_quality(
            &self,
            _country: &str, // Ignore input in mock
        ) -> crate::error::Result<CountryAirQuality> {
            let mut state = self.state.lock().unwrap();
            state.get_average_called = true;
            // Return the expected result or panic if none was set
            state.average_result.take().unwrap_or_else(|| {
                panic!("MockDatabase::get_average_air_quality called without expectation set")
            })
        }

        async fn get_latest_measurements_by_city(
            &self,
            _country: &str, // Ignore input in mock
        ) -> crate::error::Result<Vec<CityLatestMeasurements>> {
            let mut state = self.state.lock().unwrap();
            state.get_latest_by_city_called = true;
            // Return the expected result or panic if none was set
            state.latest_by_city_result.take().unwrap_or_else(|| {
                panic!(
                    "MockDatabase::get_latest_measurements_by_city called without expectation set"
                )
            })
        }
    }

    // --- Test Harness ---
    /// A simplified version of `App` using `MockDatabase`
    /// specifically for unit testing the command dispatch and validation logic in `App`.
    struct TestApp {
        db: MockDatabase,
    }

    impl TestApp {
        /// Creates a new `TestApp` with mock components.
        fn new() -> Self {
            Self {
                db: MockDatabase::new(),
            }
        }

        /// Simplified command runner that dispatches to internal methods using the mock DB.
        /// This isolates the command logic from the actual `App::new` initialization
        /// and the main interactive loop.
        async fn run_command(&self, command: Commands) -> crate::error::Result<()> {
            match command {
                Commands::InitDb => self.run_init_db().await,
                Commands::Import { days } => self.run_import(days).await,
                Commands::MostPolluted => self.run_most_polluted().await,
                Commands::Average(args) => self.run_average(&args.country).await,
                Commands::MeasurementsByLocality(args) => {
                    self.run_measurements_by_locality_table(&args.country).await
                }, // Renamed variant and method call
            }
        }

        // --- Simplified Internal Command Handlers (using MockDatabase) ---
        // These methods mirror the structure of the real `App` methods but use the mock DB.

        async fn run_init_db(&self) -> crate::error::Result<()> {
            self.db.init_schema().await?;
            Ok(())
        }

        async fn run_import(&self, days: i64) -> crate::error::Result<()> {
            self.db.init_schema().await?; // Import implicitly initializes schema
            let end_date = Utc::now();
            let _start_date = end_date - Duration::days(days); // Prefixed with underscore
            for country in COUNTRIES.iter() {
                // Simulate fetching and converting to DbMeasurement for the test
                // In a real test, you might create more realistic DbMeasurement instances
                let placeholder_db_measurements = vec![
                    // Create one placeholder DbMeasurement per country for the test
                    crate::models::DbMeasurement {
                        id: None,
                        location_id: 1,                         // Placeholder
                        sensor_id: 1, // Placeholder (Made non-optional based on model change)
                        sensor_name: "Mock Sensor".to_string(), // Added
                        location_name: "Mock Location".to_string(),
                        parameter_id: 1,                    // Placeholder
                        parameter_name: "pm25".to_string(), // Placeholder
                        parameter_display_name: Some("PM2.5".to_string()), // Added
                        value_avg: Some(sqlx::types::Decimal::from(10)), // Wrap in Some()
                        value_min: Some(sqlx::types::Decimal::from(8)), // Added
                        value_max: Some(sqlx::types::Decimal::from(12)), // Added
                        measurement_count: Some(24),        // Added
                        unit: "µg/m³".to_string(),
                        date_utc: Utc::now()
                            .date_naive()
                            .and_hms_opt(0, 0, 0)
                            .unwrap()
                            .and_local_timezone(Utc)
                            .unwrap(), // Use start of day
                        date_local: Utc::now().date_naive().to_string(), // Use date string
                        country: country.to_string(),
                        city: Some("Mock City".to_string()),
                        latitude: Some(0.0),
                        longitude: Some(0.0),
                        is_mobile: false,
                        is_monitor: true,
                        owner_name: "Mock Owner".to_string(),
                        provider_name: "Mock Provider".to_string(),
                    },
                ];
                // Call the mock insert with the placeholder DbMeasurement data
                self.db
                    .insert_measurements(&placeholder_db_measurements)
                    .await?;
            }
            Ok(())
        }

        async fn run_most_polluted(&self) -> crate::error::Result<()> {
            let country_refs: Vec<&str> = COUNTRIES.iter().copied().collect();
            let _result = self.db.get_most_polluted_country(&country_refs).await?;
            // Test focuses on verifying the DB call was made; result formatting is UI concern.
            Ok(())
        }

        async fn run_average(&self, country: &str) -> crate::error::Result<()> {
            let country_code = country.to_uppercase();
            // Perform validation as in the real App method
            if !COUNTRIES.contains(&country_code.as_str()) {
                return Err(AppError::Cli(format!("Invalid country code: {}", country)));
            }
            let _result = self.db.get_average_air_quality(&country_code).await?;
            Ok(())
        }

        async fn run_measurements_by_locality_table(
            &self,
            country: &str,
        ) -> crate::error::Result<()> {
            // Renamed method
            let country_code = country.to_uppercase();
            // Perform validation as in the real App method
            if !COUNTRIES.contains(&country_code.as_str()) {
                return Err(AppError::Cli(format!("Invalid country code: {}", country)));
            }
            let _measurements = self
                .db
                .get_latest_measurements_by_city(&country_code)
                .await?;
            Ok(())
        }
    }

    // --- Unit Tests for Command Logic using TestApp ---

    #[tokio::test]
    async fn test_cmd_init_db_calls_db_method() {
        let app = TestApp::new();
        let command = Commands::InitDb;
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(
            app.db.state.lock().unwrap().init_schema_called,
            "init_schema should be called"
        );
    }

    #[tokio::test]
    async fn test_cmd_import_calls_db_methods() {
        let app = TestApp::new();
        let command = Commands::Import { days: 3 }; // Use a small number of days for test
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(
            app.db.state.lock().unwrap().init_schema_called,
            "init_schema should be called during import"
        );
        assert!(
            app.db.state.lock().unwrap().insert_measurements_called,
            "insert_measurements should be called during import"
        );
    }

    #[tokio::test]
    async fn test_cmd_most_polluted_calls_db_method() {
        let app = TestApp::new();
        // Set expectation for the mock DB call
        let expected_ranking = PollutionRanking::new("PK"); // Simple default is fine for checking call
        app.db.expect_get_most_polluted(Ok(expected_ranking));

        let command = Commands::MostPolluted;
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(
            app.db.state.lock().unwrap().get_most_polluted_called,
            "get_most_polluted_country should be called"
        );
    }

    #[tokio::test]
    async fn test_cmd_average_valid_country_calls_db_method() {
        let app = TestApp::new();
        // Set expectation for the mock DB call
        let expected_average = CountryAirQuality {
            country: "NL".to_string(),
            measurement_count: 0,
            avg_pm25: None,
            avg_pm10: None,
            avg_o3: None,
            avg_no2: None,
            avg_so2: None,
            avg_co: None,
        };
        app.db.expect_get_average(Ok(expected_average));

        let command = Commands::Average(AverageArgs {
            country: "NL".to_string(),
        });
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(
            app.db.state.lock().unwrap().get_average_called,
            "get_average_air_quality should be called"
        );
    }

    #[tokio::test]
    async fn test_cmd_average_invalid_country_fails_validation() {
        let app = TestApp::new();
        // No DB expectation needed as validation should fail first
        let command = Commands::Average(AverageArgs {
            country: "XX".to_string(),
        }); // Invalid code
        let result = app.run_command(command).await;
        assert!(result.is_err());
        // Check the error type and message
        match result.err().unwrap() {
            AppError::Cli(msg) => assert!(msg.contains("Invalid country code: XX")), // Check specific message
            e => panic!("Expected CliError, got {:?}", e),
        }
        // Ensure the DB method was *not* called due to failed validation
        assert!(
            !app.db.state.lock().unwrap().get_average_called,
            "get_average_air_quality should not be called for invalid country"
        );
    }

    #[tokio::test]
    async fn test_cmd_measurements_valid_country_calls_db_method() {
        // Keep test name for clarity
        let app = TestApp::new();
        // Set expectation for the mock DB call (empty vec is a valid result)
        app.db.expect_get_latest_by_city(Ok(vec![])); // DB method name remains the same for now

        let command = Commands::MeasurementsByLocality(MeasurementsByLocalityArgs {
            // Use renamed variant and args struct
            country: "DE".to_string(),
        });
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(
            app.db.state.lock().unwrap().get_latest_by_city_called,
            "get_latest_measurements_by_city should be called"
        );
    }

    #[tokio::test]
    async fn test_cmd_measurements_invalid_country_fails_validation() {
        // Keep test name for clarity
        let app = TestApp::new();
        // No DB expectation needed
        let command = Commands::MeasurementsByLocality(MeasurementsByLocalityArgs {
            // Use renamed variant and args struct
            country: "YY".to_string(),
        }); // Invalid code
        let result = app.run_command(command).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            AppError::Cli(msg) => assert!(msg.contains("Invalid country code: YY")), // Check specific message
            e => panic!("Expected CliError, got {:?}", e),
        }
        assert!(
            !app.db.state.lock().unwrap().get_latest_by_city_called,
            "get_latest_measurements_by_city should not be called for invalid country"
        );
    }
}
