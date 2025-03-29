//! Defines the core application logic, state management, command definitions,
//! and execution handlers for the CLI.
//!
//! This module orchestrates the interaction between the API client, database,
//! and user interface elements (prompts, tables, progress bars).

use crate::api::{MockDataProvider, OpenAQClient};
use crate::db::Database;
use crate::error::{AppError, Result};
use chrono::{Duration, Utc};
use colored::*;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table};
use dialoguer::{theme::ColorfulTheme, Input, Select};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap; // Add this import
use std::env;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::Mutex;
use tracing::{error, info};

/// Represents the different states the application can be in, primarily tracking
/// database initialization and data import status. This influences the available
/// commands in the interactive menu.
#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    /// Initial state before the database schema is created.
    Uninitialized,
    /// State after the database schema has been initialized but no data imported yet.
    DbInitialized,
    /// State after data has been successfully imported at least once.
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

// Add a function to get the country name map
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

/// Defines the available commands that the user can trigger through the interactive menu.
#[derive(Debug, Clone)]
pub enum Commands {
    /// Initialize or re-initialize the database schema.
    InitDb,
    /// Import data from the OpenAQ API for a specified number of past days.
    Import { days: i64 },
    /// Find the most polluted country based on recent PM2.5 and PM10 data (last 7 days).
    MostPolluted,
    /// Calculate the 5-day average air quality metrics for a specific country.
    Average(AverageArgs),
    /// Get the latest measurements for all parameters, grouped by city, for a specific country.
    Measurements(MeasurementsArgs),
}

/// Arguments for the `Average` command.
#[derive(Debug, Clone)]
pub struct AverageArgs {
    /// The 2-letter country code.
    pub country: String,
    // Removed days field
}

/// Arguments for the `Measurements` command.
#[derive(Debug, Clone)]
pub struct MeasurementsArgs {
    /// The 2-letter country code.
    pub country: String,
}

/// The main application structure, holding database connections, API clients,
/// and the current application state.
pub struct App {
    db: Database,
    api_client: OpenAQClient,
    mock_provider: MockDataProvider, // Used as fallback if API fails
    state: Arc<Mutex<AppState>>,     // Shared, mutable state
}

impl App {
    /// Creates a new `App` instance.
    ///
    /// Initializes database connection, API client, loads environment variables,
    /// and determines the initial `AppState` by checking the database.
    pub async fn new() -> Result<Self> {
        dotenv::dotenv().ok(); // Load .env file if present
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@localhost:5432/air_quality".to_string()
        });
        let api_key = env::var("OPENAQ_KEY").map_err(|e| {
            error!("OPENAQ_KEY environment variable not set: {}", e);
            AppError::Env(e)
        })?;

        let db = Database::new(&database_url).await?;
        let api_client = OpenAQClient::new(api_key);
        let mock_provider = MockDataProvider::new();

        // Determine initial state by querying the database
        let initial_state = if db.has_data_imported().await? {
            AppState::DataImported
        } else if db.is_schema_initialized().await? {
            AppState::DbInitialized
        } else {
            AppState::Uninitialized
        };
        info!("Initial application state: {:?}", initial_state);

        Ok(Self {
            db,
            api_client,
            mock_provider,
            state: Arc::new(Mutex::new(initial_state)),
        })
    }

    /// Returns a clone of the current application state.
    pub async fn get_state(&self) -> AppState {
        self.state.lock().await.clone()
    }

    /// Executes the given command.
    ///
    /// Handles the core logic for each command, including user feedback (progress bars, tables),
    /// interaction with the database and API client, and updating the application state
    /// upon successful completion of state-changing commands (`InitDb`, `Import`).
    pub async fn run_command(&self, command: Commands) -> Result<()> {
        let state_clone = Arc::clone(&self.state); // Clone Arc for state updates

        match command {
            Commands::InitDb => {
                println!("{}", "Initializing database schema...".yellow());
                let pb = ProgressBar::new_spinner();
                pb.enable_steady_tick(StdDuration::from_millis(120));
                pb.set_style(
                    ProgressStyle::with_template("{spinner:.blue} {msg}")?
                        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
                );
                pb.set_message("Connecting and initializing...");
                self.db.init_schema().await?;
                pb.finish_with_message("Database schema initialized successfully!".to_string());
                info!("Database schema initialized successfully");

                // Update state only if it was previously Uninitialized
                let mut state = state_clone.lock().await;
                if *state == AppState::Uninitialized {
                    *state = AppState::DbInitialized;
                    info!("App state updated to: {:?}", *state);
                } else {
                    // If already DbInitialized or DataImported, state remains the same after re-init
                    info!("Database re-initialized (state remains {:?}).", *state);
                }
                Ok(())
            },
            Commands::Import { days } => {
                self.import_data(days).await?;

                // Update state to DataImported after successful import
                let mut state = state_clone.lock().await;
                *state = AppState::DataImported;
                info!("App state updated to: {:?}", *state);
                Ok(())
            },
            Commands::MostPolluted => {
                self.find_most_polluted().await?;
                Ok(())
            },
            Commands::Average(args) => {
                // Call calculate_average without the days argument
                self.calculate_average(&args.country).await?;
                Ok(())
            },
            Commands::Measurements(args) => {
                self.get_measurements_table(&args.country).await?;
                Ok(())
            },
        }
    }

    /// Fetches data from the OpenAQ API (or mock provider on failure) for the specified
    /// number of days for all `COUNTRIES` and inserts it into the database.
    async fn import_data(&self, days: i64) -> Result<()> {
        println!(
            "{} {}",
            "Importing data for the last".yellow(),
            format!("{} days", days).yellow().bold()
        );
        let pb = ProgressBar::new((COUNTRIES.len() * 2) as u64);
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) {msg}",
            )?
            .progress_chars("#>-"),
        );
        pb.enable_steady_tick(StdDuration::from_millis(100));

        info!("Ensuring database schema exists...");
        self.db.init_schema().await?;

        let end_date = Utc::now();
        let start_date = end_date - Duration::days(days);

        for country in COUNTRIES.iter() {
            let country_str = country.to_string();
            pb.set_message(format!("Fetching data for {}...", country_str));

            let measurements = match self
                .api_client
                .get_measurements_for_country_in_date_range(country, start_date, end_date)
                .await
            {
                Ok(m) => {
                    info!("Fetched {} measurements for {} from API", m.len(), country);
                    m
                },
                Err(e) => {
                    info!("API request failed: {}. Using mock data instead.", e);
                    let mock_m = self
                        .mock_provider
                        .get_measurements_for_country_in_date_range(
                            &country_str,
                            start_date,
                            end_date,
                        )?;
                    info!(
                        "Generated {} mock measurements for {}",
                        mock_m.len(),
                        country_str
                    );
                    mock_m
                },
            };
            pb.inc(1);

            pb.set_message(format!(
                "Inserting {} measurements for {}...",
                measurements.len(),
                country_str
            ));
            info!(
                "Processed {} measurements for {}",
                measurements.len(),
                country_str
            );
            self.db.insert_measurements(&measurements).await?;
            pb.inc(1);
        }
        pb.finish_with_message("Data import completed successfully!".to_string());
        info!("Data import completed successfully");
        Ok(())
    }

    /// Queries the database for the most polluted country based on a weighted index
    /// of recent PM2.5 and PM10 values (last 7 days) and displays the result in a table.
    async fn find_most_polluted(&self) -> Result<()> {
        println!(
            "{}",
            "Finding the most polluted country (last 7 days)...".yellow()
        ); // Updated description
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(StdDuration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {msg}")?
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message("Querying database...");
        let country_refs: Vec<&str> = COUNTRIES.to_vec();
        // NOTE: The actual time interval logic is in db.get_most_polluted_country
        let result = self.db.get_most_polluted_country(&country_refs).await?;
        pb.finish_and_clear();

        let country_map = get_country_name_map();
        // Fix E0716: Ensure the fallback reference lives long enough
        let full_country_name = country_map
            .get(result.country.as_str())
            .copied() // Convert Option<&&'static str> to Option<&'static str>
            .unwrap_or(&result.country); // Fallback to &String (coerces to &str with appropriate lifetime)

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
            Cell::new(format!("{} ({})", full_country_name, result.country)) // Show full name + code
                .fg(Color::Cyan)
                .add_attribute(Attribute::Bold),
        ]);
        table.add_row(vec![
            Cell::new("Pollution Index"),
            Cell::new(format!("{:.2}", result.pollution_index)),
        ]);
        table.add_row(vec![
            Cell::new("Avg PM2.5 (µg/m³)"),
            Cell::new(
                result
                    .pm25_avg
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ]);
        table.add_row(vec![
            Cell::new("Avg PM10 (µg/m³)"),
            Cell::new(
                result
                    .pm10_avg
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ]);
        println!("{table}");
        Ok(())
    }

    /// Calculates the 5-day average value for each pollutant
    /// for the given country and displays the results in a table.
    // Remove the 'days' parameter from the function signature
    async fn calculate_average(&self, country: &str) -> Result<()> {
        // Removed unused 'days' variable
        let country_code = country.to_uppercase();
        let country_map = get_country_name_map();
        // Fix E0716: Ensure the fallback reference lives long enough
        let full_country_name = country_map
            .get(country_code.as_str())
            .copied()
            .unwrap_or(&country_code);

        // Validate country code against the predefined list
        if !COUNTRIES.contains(&country_code.as_str()) {
            return Err(AppError::Cli(format!(
                "Invalid country code. Must be one of: {:?}",
                COUNTRIES
            )));
        }
        println!(
            "{} {}-{} {} ({})", // Updated format string for fixed 5 days
            "Calculating".yellow(),
            "5-day average for".yellow(),      // Hardcode 5 days
            full_country_name.yellow().bold(), // Use full name
            "country code:".yellow(),
            country_code.yellow().bold()
        );
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(StdDuration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {msg}")?
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message("Querying database...");
        // Call DB function without days argument
        let result = self.db.get_average_air_quality(&country_code).await?;
        pb.finish_and_clear();

        // Fix E0716: Ensure the fallback reference lives long enough
        let result_full_name = country_map
            .get(result.country.as_str())
            .copied()
            .unwrap_or(&result.country); // Get full name for result

        println!(
            "{}-{} {} {} ({}) ({})", // Corrected format string (6 placeholders)
            "5".bold(),              // Hardcode 5 days
            "day average air quality for".green(),
            result_full_name.bold().cyan(), // Use full name
            result.country.bold().cyan(),   // Show code too
            "Based on".dimmed(),
            format!("{} measurements", result.measurement_count).dimmed()
        );
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("Parameter").fg(Color::Green),
                Cell::new("Average Value (µg/m³)").fg(Color::Green),
            ]);
        let format_avg = |val: Option<f64>| -> String {
            val.map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "-".to_string())
        };
        table.add_row(vec![
            Cell::new("PM2.5"),
            Cell::new(format_avg(result.avg_pm25)),
        ]);
        table.add_row(vec![
            Cell::new("PM10"),
            Cell::new(format_avg(result.avg_pm10)),
        ]);
        table.add_row(vec![Cell::new("O3"), Cell::new(format_avg(result.avg_o3))]);
        table.add_row(vec![
            Cell::new("NO2"),
            Cell::new(format_avg(result.avg_no2)),
        ]);
        table.add_row(vec![
            Cell::new("SO2"),
            Cell::new(format_avg(result.avg_so2)),
        ]);
        table.add_row(vec![Cell::new("CO"), Cell::new(format_avg(result.avg_co))]);
        println!("{table}");
        Ok(())
    }

    /// Fetches the latest measurement for each parameter, grouped by city, for the specified
    /// country and displays the results in a table.
    async fn get_measurements_table(&self, country: &str) -> Result<()> {
        let country_code = country.to_uppercase();
        let country_map = get_country_name_map();
        // Fix E0716: Ensure the fallback reference lives long enough
        let full_country_name = country_map
            .get(country_code.as_str())
            .copied()
            .unwrap_or(&country_code);

        // Validate country code
        if !COUNTRIES.contains(&country_code.as_str()) {
            return Err(AppError::Cli(format!(
                "Invalid country code. Must be one of: {:?}",
                COUNTRIES
            )));
        }
        println!(
            "{} {} ({})", // Updated format string
            "Fetching latest measurements by city for".yellow(),
            full_country_name.yellow().bold(), // Use full name
            country_code.yellow().bold()
        );
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(StdDuration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {msg}")?
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message("Querying database...");
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
                .yellow() // Use full name
            );
            return Ok(());
        }
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![
                Cell::new("City").fg(Color::Green),
                Cell::new("PM2.5 (µg/m³)").fg(Color::Green),
                Cell::new("PM10 (µg/m³)").fg(Color::Green),
                Cell::new("O3 (µg/m³)").fg(Color::Green),
                Cell::new("NO2 (µg/m³)").fg(Color::Green),
                Cell::new("SO2 (µg/m³)").fg(Color::Green),
                Cell::new("CO (µg/m³)").fg(Color::Green),
                Cell::new("Last Updated (UTC)").fg(Color::Green),
            ]);
        let format_value = |val: Option<sqlx::types::Decimal>| -> String {
            val.map(|d| format!("{:.2}", d))
                .unwrap_or_else(|| "-".to_string())
        };
        for measurement in city_measurements {
            table.add_row(vec![
                Cell::new(measurement.city).fg(Color::Cyan),
                Cell::new(format_value(measurement.pm25)),
                Cell::new(format_value(measurement.pm10)),
                Cell::new(format_value(measurement.o3)),
                Cell::new(format_value(measurement.no2)),
                Cell::new(format_value(measurement.so2)),
                Cell::new(format_value(measurement.co)),
                Cell::new(measurement.last_updated.format("%Y-%m-%d %H:%M")),
            ]);
        }
        println!("{table}");
        Ok(())
    }
}

// --- User Interaction Helpers ---

/// Prompts the user to select a country from the predefined `COUNTRIES` list.
pub fn prompt_country() -> Result<String> {
    let country_map = get_country_name_map();
    let country_display: Vec<String> = COUNTRIES
        .iter()
        .map(|code| format!("{} ({})", country_map.get(code).unwrap_or(code), code)) // Show "Name (Code)"
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a country")
        .items(&country_display) // Use display names
        .default(0)
        .interact()?;
    Ok(COUNTRIES[selection].to_string()) // Return the code
}

/// Prompts the user to enter the number of days for historical data retrieval.
///
/// Validates that the input is an integer between 7 and 365 (inclusive).
pub fn prompt_days() -> Result<i64> {
    let days: i64 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter number of days for history (min 7, max 365)") // Updated prompt
        .default(7) // Default to 7 days
        .validate_with(|input: &i64| -> std::result::Result<(), &str> {
            if *input >= 7 && *input <= 365 {
                // Updated validation: >= 7
                Ok(())
            } else {
                Err("Please enter a number of days between 7 and 365.") // Updated error message
            }
        })
        .interact_text()?;
    Ok(days)
}

// --- Tests ---
#[cfg(test)]
mod tests {
    use super::*;
    // Note: No need to import Result from crate::error as it's shadowed by std::result::Result here.
    // Use crate::error::Result explicitly if needed.
    use crate::models::{CityLatestMeasurements, CountryAirQuality, Measurement, PollutionRanking};
    use chrono::{Duration, Utc};
    // Removed unused Decimal, DbMeasurement, FromPrimitive imports from test scope
    use std::sync::{Arc, Mutex};

    // --- Mock Database State (for tracking calls) ---
    #[derive(Clone, Default)]
    struct MockDbState {
        init_schema_called: bool,
        insert_measurements_called: bool,
        get_most_polluted_called: bool,
        get_average_called: bool,
        get_latest_by_city_called: bool,
        most_polluted_result: Option<Result<PollutionRanking>>,
        average_result: Option<Result<CountryAirQuality>>,
        latest_by_city_result: Option<crate::error::Result<Vec<CityLatestMeasurements>>>,
    }

    // --- Mock Database (implements necessary methods for testing App logic) ---
    #[derive(Clone)]
    struct MockDatabase {
        state: Arc<Mutex<MockDbState>>,
    }

    impl MockDatabase {
        fn new() -> Self {
            Self {
                state: Arc::new(Mutex::new(MockDbState::default())),
            }
        }
        // Methods to set expectations for query results
        fn expect_get_most_polluted(&self, result: crate::error::Result<PollutionRanking>) {
            self.state.lock().unwrap().most_polluted_result = Some(result);
        }
        fn expect_get_average(&self, result: crate::error::Result<CountryAirQuality>) {
            self.state.lock().unwrap().average_result = Some(result);
        }
        fn expect_get_latest_by_city(
            &self,
            result: crate::error::Result<Vec<CityLatestMeasurements>>,
        ) {
            self.state.lock().unwrap().latest_by_city_result = Some(result);
        }

        // Mock implementations of Database methods used by App
        async fn init_schema(&self) -> crate::error::Result<()> {
            self.state.lock().unwrap().init_schema_called = true;
            Ok(())
        }
        async fn insert_measurements(
            &self,
            _measurements: &[Measurement],
        ) -> crate::error::Result<()> {
            self.state.lock().unwrap().insert_measurements_called = true;
            Ok(())
        }
        async fn get_most_polluted_country(
            &self,
            _countries: &[&str],
        ) -> crate::error::Result<PollutionRanking> {
            let mut state = self.state.lock().unwrap();
            state.get_most_polluted_called = true;
            state.most_polluted_result.take().unwrap_or_else(|| {
                panic!("MockDatabase::get_most_polluted_country called without expectation")
            })
        }
        // Update mock signature to remove _days
        // Update mock signature to remove _days
        async fn get_average_air_quality(
            &self,
            _country: &str,
            // _days: i64, // Removed
        ) -> crate::error::Result<CountryAirQuality> {
            let mut state = self.state.lock().unwrap();
            state.get_average_called = true;
            state.average_result.take().unwrap_or_else(|| {
                panic!("MockDatabase::get_average_air_quality called without expectation")
            })
        }
        async fn get_latest_measurements_by_city(
            &self,
            _country: &str,
        ) -> crate::error::Result<Vec<CityLatestMeasurements>> {
            let mut state = self.state.lock().unwrap();
            state.get_latest_by_city_called = true;
            state.latest_by_city_result.take().unwrap_or_else(|| {
                panic!("MockDatabase::get_latest_measurements_by_city called without expectation")
            })
        }
        // Removed unused mock methods is_schema_initialized and has_data_imported
    }

    // --- Test Harness (simplified version of App for testing command logic) ---
    struct TestApp {
        db: MockDatabase,
        mock_provider: MockDataProvider,
    }

    impl TestApp {
        fn new() -> Self {
            Self {
                db: MockDatabase::new(),
                mock_provider: MockDataProvider::new(),
            }
        }
        // Simplified command runner that calls mock methods
        async fn run_command(&self, command: Commands) -> crate::error::Result<()> {
            match command {
                Commands::InitDb => self.run_init_db().await,
                Commands::Import { days } => self.run_import(days).await,
                Commands::MostPolluted => self.run_most_polluted().await,
                // Call run_average without days arg
                Commands::Average(args) => self.run_average(&args.country).await,
                Commands::Measurements(args) => self.run_measurements_table(&args.country).await,
            }
        }
        // Simplified internal methods calling the mock DB
        async fn run_init_db(&self) -> crate::error::Result<()> {
            self.db.init_schema().await?;
            Ok(())
        }
        async fn run_import(&self, days: i64) -> crate::error::Result<()> {
            self.db.init_schema().await?; // Import implicitly initializes schema if needed
            let end_date = Utc::now();
            let start_date = end_date - Duration::days(days);
            for country in COUNTRIES.iter() {
                // In tests, we assume mock provider succeeds
                let mock_measurements = self
                    .mock_provider
                    .get_measurements_for_country_in_date_range(country, start_date, end_date)?;
                self.db.insert_measurements(&mock_measurements).await?;
            }
            Ok(())
        }
        async fn run_most_polluted(&self) -> crate::error::Result<()> {
            let country_refs: Vec<&str> = COUNTRIES.iter().copied().collect();
            let _result = self.db.get_most_polluted_country(&country_refs).await?;
            // In real app, this result would be formatted and printed
            Ok(())
        }
        // Remove days parameter from test harness function
        async fn run_average(&self, country: &str) -> crate::error::Result<()> {
            let country_code = country.to_uppercase();
            if !COUNTRIES.contains(&country_code.as_str()) {
                return Err(AppError::Cli(format!("Invalid country code: {}", country)));
            }
            // Call DB function without days (already fixed, ensuring it stays)
            let _result = self.db.get_average_air_quality(&country_code).await?;
            // In real app, this result would be formatted and printed
            Ok(())
        }
        async fn run_measurements_table(&self, country: &str) -> crate::error::Result<()> {
            let country_code = country.to_uppercase();
            if !COUNTRIES.contains(&country_code.as_str()) {
                return Err(AppError::Cli(format!("Invalid country code: {}", country)));
            }
            let _measurements = self
                .db
                .get_latest_measurements_by_city(&country_code)
                .await?;
            // In real app, this result would be formatted and printed
            Ok(())
        }
    }

    // --- Unit Tests for Command Logic ---
    #[tokio::test]
    async fn test_cmd_init_db() {
        let app = TestApp::new();
        let command = Commands::InitDb;
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().init_schema_called); // Verify mock method was called
    }

    #[tokio::test]
    async fn test_cmd_import() {
        let app = TestApp::new();
        let command = Commands::Import { days: 3 };
        // No specific expectations needed for DB queries during import test,
        // as we primarily check if insert_measurements was called.
        // The mock provider handles data generation.

        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().init_schema_called); // Import calls init_schema
        assert!(app.db.state.lock().unwrap().insert_measurements_called); // Verify mock method was called
    }

    #[tokio::test]
    async fn test_cmd_most_polluted() {
        let app = TestApp::new();
        // Set expectation for the mock DB call
        let expected_ranking = PollutionRanking {
            country: "PK".to_string(),
            pollution_index: 150.0,
            pm25_avg: Some(50.0),
            pm10_avg: Some(100.0),
        };
        app.db.expect_get_most_polluted(Ok(expected_ranking));

        let command = Commands::MostPolluted;
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().get_most_polluted_called); // Verify mock method was called
    }

    #[tokio::test]
    async fn test_cmd_average_valid_country() {
        let app = TestApp::new();
        // Set expectation for the mock DB call
        let expected_average = CountryAirQuality {
            country: "NL".to_string(),
            avg_pm25: Some(15.0),
            avg_pm10: Some(25.0),
            avg_o3: None,
            avg_no2: Some(30.0),
            avg_so2: None,
            avg_co: None,
            measurement_count: 10,
        };
        // Update expectation call to match the new signature (no days)
        app.db.expect_get_average(Ok(expected_average));

        // Create AverageArgs without the days field (already fixed, ensuring it stays)
        let command = Commands::Average(AverageArgs {
            country: "NL".to_string(),
        });
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().get_average_called); // Verify mock method was called
    }

    #[tokio::test]
    async fn test_cmd_average_invalid_country() {
        let app = TestApp::new();
        // No DB expectation needed as it should fail validation first
        // Create AverageArgs without the days field (already fixed, ensuring it stays)
        let command = Commands::Average(AverageArgs {
            country: "XX".to_string(), // Invalid country code
        });
        let result = app.run_command(command).await;
        assert!(result.is_err());
        // Check that the error is the expected CLI validation error
        match result.err().unwrap() {
            AppError::Cli(msg) => assert!(msg.contains("Invalid country code")),
            e => panic!("Expected CliError, got {:?}", e),
        }
        // Ensure the DB method was *not* called
        assert!(!app.db.state.lock().unwrap().get_average_called);
    }

    #[tokio::test]
    async fn test_cmd_measurements_valid_country() {
        let app = TestApp::new();
        // Set expectation for the mock DB call (empty vec is a valid result)
        let expected_city_measurements = vec![];
        app.db
            .expect_get_latest_by_city(Ok(expected_city_measurements));

        let command = Commands::Measurements(MeasurementsArgs {
            country: "DE".to_string(),
        });
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().get_latest_by_city_called); // Verify mock method was called
    }

    #[tokio::test]
    async fn test_cmd_measurements_invalid_country() {
        let app = TestApp::new();
        // No DB expectation needed
        let command = Commands::Measurements(MeasurementsArgs {
            country: "YY".to_string(), // Invalid country code
        });
        let result = app.run_command(command).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            AppError::Cli(msg) => assert!(msg.contains("Invalid country code")),
            e => panic!("Expected CliError, got {:?}", e),
        }
        assert!(!app.db.state.lock().unwrap().get_latest_by_city_called); // Ensure DB method was not called
    }
}
