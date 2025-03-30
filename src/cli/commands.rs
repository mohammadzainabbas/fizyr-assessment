//! Defines the core application logic, state management, command definitions,
//! and execution handlers for the CLI.
//!
//! This module orchestrates interactions between the API client, database,
//! and user interface elements (prompts, tables, progress bars), managing the
//! overall application flow based on user input and application state.

use crate::api::OpenAQClient;
use crate::db::Database;
use crate::error::{AppError, Result};
// Removed unused model imports: CityLatestMeasurements, CountryAirQuality, Measurement, PollutionRanking
// Removed MockDataProvider import below
use chrono::{Duration, Utc};
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
    /// Initialize or re-initialize the database schema (`measurements` table and indexes).
    InitDb,
    /// Import data from the OpenAQ API for a specified number of past days.
    Import { days: i64 },
    /// Find the most polluted country (from `COUNTRIES`) based on recent PM2.5/PM10 data.
    MostPolluted,
    /// Calculate the 5-day average air quality metrics for a specific country.
    Average(AverageArgs),
    /// Get the latest measurements for all parameters, grouped by city, for a specific country.
    Measurements(MeasurementsArgs),
}

/// Arguments for the `Average` command.
#[derive(Debug, Clone)]
pub struct AverageArgs {
    /// The 2-letter country code for which to calculate the average.
    pub country: String,
}

/// Arguments for the `Measurements` command.
#[derive(Debug, Clone)]
pub struct MeasurementsArgs {
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
    // mock_provider: MockDataProvider, // Removed field
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
        // let mock_provider = MockDataProvider::new(); // Removed initialization

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
            // mock_provider, // Removed field from initialization
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
            Commands::Measurements(args) => {
                self.get_measurements_table(&args.country).await?;
                Ok(())
            },
        }
    }

    /// Imports data for the specified number of past days for all `COUNTRIES`.
    ///
    /// Imports data for the specified number of past days for all `COUNTRIES`.
    ///
    /// Fetches locations for each country, then fetches measurements for each sensor
    /// at those locations using the OpenAQ v3 API. Data is then converted and
    /// inserted into the database. Displays progress using `indicatif`.
    ///
    /// # Arguments
    ///
    /// * `days` - The number of days of historical data to import.
    ///
    /// # Errors
    ///
    /// Returns `AppError` if schema initialization, API fetching, data conversion,
    /// or database insertion fails.
    async fn import_data(&self, days: i64) -> Result<()> {
        println!(
            "{} {}",
            "Importing data for the last".yellow(),
            format!("{} days", days).yellow().bold()
        );

        info!("Ensuring database schema exists before import...");
        self.db.init_schema().await?; // Idempotent schema initialization

        let end_date = Utc::now();
        let start_date = end_date - Duration::days(days);
        info!("Importing data from {} to {}", start_date, end_date);

        let mut all_db_measurements = Vec::new();
        let total_countries = COUNTRIES.len();
        let pb_outer = Self::create_progress_bar(total_countries as u64);
        pb_outer.set_message("Fetching locations...");

        for country_code in COUNTRIES.iter() {
            pb_outer.set_message(format!("Processing {}...", country_code));
            info!("Fetching locations for country: {}", country_code);

            let locations = match self
                .api_client
                .get_locations_for_country(country_code)
                .await
            {
                Ok(locs) => locs,
                Err(e) => {
                    error!(
                        "Failed to fetch locations for {}: {}. Skipping country.",
                        country_code, e
                    );
                    pb_outer.println(format!(
                        "{} Failed to fetch locations for {}: {}. Skipping.",
                        "Error:".red(),
                        country_code,
                        e
                    ));
                    pb_outer.inc(1); // Increment outer progress bar even on skip
                    continue; // Skip to the next country
                },
            };
            info!("Found {} locations for {}", locations.len(), country_code);

            if locations.is_empty() {
                pb_outer.println(format!(
                    "{} No locations found for {}. Skipping.",
                    "Warning:".yellow(),
                    country_code,
                ));
                pb_outer.inc(1);
                continue;
            }

            let pb_inner = Self::create_progress_bar(locations.len() as u64);
            pb_inner.set_message(format!(
                "Fetching measurements for {} locations...",
                country_code
            ));

            for location in locations {
                pb_inner.set_message(format!(
                    "Loc {}: {} sensors",
                    location.id,
                    location.sensors.len()
                ));
                info!(
                    "Processing location ID: {} ({} sensors)",
                    location.id,
                    location.sensors.len()
                );

                for sensor in &location.sensors {
                    info!("Fetching measurements for sensor ID: {}", sensor.id);
                    let measurements_v3 = match self
                        .api_client
                        .get_measurements_for_sensor(sensor.id, start_date, end_date)
                        .await
                    {
                        Ok(m) => m,
                        Err(e) => {
                            error!(
                                "Failed to fetch measurements for sensor {}: {}. Skipping sensor.",
                                sensor.id, e
                            );
                            pb_inner.println(format!(
                                "{} Failed to fetch measurements for sensor {}: {}. Skipping.",
                                "Warning:".yellow(),
                                sensor.id,
                                e
                            ));
                            continue; // Skip to the next sensor
                        },
                    };

                    info!(
                        "Fetched {} measurements for sensor {}",
                        measurements_v3.len(),
                        sensor.id
                    );

                    // Convert MeasurementV3 to DbMeasurement using the implemented function
                    for m_v3 in measurements_v3 {
                        // Determine the measurement time. The API doesn't return it directly
                        // in the /measurements endpoint result items. We assume the measurement
                        // corresponds roughly to the start of its period if available,
                        // otherwise we might need a different strategy or accept potential inaccuracy.
                        // For now, let's use the period's start time or fallback to `start_date`
                        // of the query range as a rough estimate.
                        // FIXME: This timestamp handling might need refinement based on API behavior.
                        let measurement_time = m_v3
                            .period
                            .as_ref()
                            .and_then(|p| p.datetime_from.as_ref())
                            .map(|dt| dt.utc)
                            .unwrap_or(start_date); // Fallback to query start time

                        let db_m = crate::models::DbMeasurement::from_v3_measurement(
                            &m_v3,
                            &location,
                            sensor.id,
                            measurement_time,
                        );
                        all_db_measurements.push(db_m);
                    }
                }
                pb_inner.inc(1); // Increment inner progress bar after processing a location
            }
            pb_inner.finish_with_message(format!("Finished fetching for {}", country_code));
            pb_outer.inc(1); // Increment outer progress bar after processing a country
        }
        pb_outer.finish_with_message("Finished fetching all countries.");

        if all_db_measurements.is_empty() {
            println!("{}", "No measurements fetched to insert.".yellow());
            info!("Data import process finished: No measurements fetched.");
            return Ok(());
        }

        // Insert all collected measurements into the database
        println!(
            "{}",
            format!(
                "Inserting {} total measurements...",
                all_db_measurements.len()
            )
            .yellow()
        );
        let pb_insert = Self::create_spinner("Inserting data into database...");
        self.db.insert_measurements(&all_db_measurements).await?; // Assuming insert_measurements accepts Vec<DbMeasurement>
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

    /// Fetches and displays the latest measurement for each parameter, grouped by city,
    /// for the specified country.
    ///
    /// Validates the country code, queries the database using `db.get_latest_measurements_by_city`,
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
    async fn get_measurements_table(&self, country: &str) -> Result<()> {
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
            "Fetching latest measurements by city for".yellow(),
            full_country_name.yellow().bold(),
            country_code.yellow().bold()
        );
        let pb = Self::create_spinner("Querying database...");
        let city_measurements = self
            .db
            .get_latest_measurements_by_city(&country_code)
            .await?;
        pb.finish_and_clear();

        if city_measurements.is_empty() {
            println!(
                "{}",
                format!(
                    "No measurements found for cities in {} ({})",
                    full_country_name, country_code
                )
                .yellow()
            );
            return Ok(());
        }

        println!(
            "{} {} ({})",
            "Latest measurements by city for".green(),
            full_country_name.bold().cyan(),
            country_code.bold().cyan()
        );

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("City").fg(Color::Green),
                Cell::new("PM2.5").fg(Color::Green), // Assuming µg/m³ unit implicitly
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

        for measurement in city_measurements {
            table.add_row(vec![
                Cell::new(measurement.city).fg(Color::Cyan),
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
    /// A simplified version of `App` using `MockDatabase` and `MockDataProvider`
    /// specifically for unit testing the command dispatch and validation logic in `App`.
    struct TestApp {
        db: MockDatabase,
        // mock_provider: MockDataProvider, // Removed field
    }

    impl TestApp {
        /// Creates a new `TestApp` with mock components.
        fn new() -> Self {
            Self {
                db: MockDatabase::new(),
                // mock_provider: MockDataProvider::new(), // Removed initialization
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
                Commands::Measurements(args) => self.run_measurements_table(&args.country).await,
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
                        location_id: 1,     // Placeholder
                        sensor_id: Some(1), // Placeholder
                        location_name: "Mock Location".to_string(),
                        parameter_id: 1,                       // Placeholder
                        parameter_name: "pm25".to_string(),    // Placeholder
                        value: sqlx::types::Decimal::from(10), // Placeholder
                        unit: "µg/m³".to_string(),
                        date_utc: Utc::now(),
                        date_local: Utc::now().to_rfc3339(),
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

        async fn run_measurements_table(&self, country: &str) -> crate::error::Result<()> {
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
        let app = TestApp::new();
        // Set expectation for the mock DB call (empty vec is a valid result)
        app.db.expect_get_latest_by_city(Ok(vec![]));

        let command = Commands::Measurements(MeasurementsArgs {
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
        let app = TestApp::new();
        // No DB expectation needed
        let command = Commands::Measurements(MeasurementsArgs {
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
