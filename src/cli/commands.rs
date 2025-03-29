use crate::api::{MockDataProvider, OpenAQClient};
use crate::db::Database;
use crate::error::{AppError, Result};
use chrono::{Duration, Utc};
// Removed clap imports
use colored::*;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Table}; // Added Attribute, Color
use dialoguer::{theme::ColorfulTheme, Input, Select};
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::sync::Arc; // For Arc<Mutex<AppState>>
use std::time::Duration as StdDuration;
use tokio::sync::Mutex; // For App state
use tracing::{error, info};

/// Represents the current state of the application initialization.
#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    Uninitialized,
    DbInitialized,
    DataImported,
}

/// List of countries to consider for pollution analysis
pub const COUNTRIES: [&str; 6] = [
    "NL", // Netherlands
    "DE", // Germany
    "FR", // France
    "GR", // Greece
    "ES", // Spain
    "PK", // Pakistan
];

// Commands enum (no clap attributes)
#[derive(Debug, Clone)]
pub enum Commands {
    InitDb,
    Import { days: i64 },
    MostPolluted,
    Average(AverageArgs),
    Measurements(MeasurementsArgs),
}

// Args structs (no clap attributes)
#[derive(Debug, Clone)]
pub struct AverageArgs {
    pub country: String,
    pub days: i64,
}

#[derive(Debug, Clone)]
pub struct MeasurementsArgs {
    pub country: String,
}

/// CLI application
pub struct App {
    db: Database,
    api_client: OpenAQClient,
    mock_provider: MockDataProvider,
    state: Arc<Mutex<AppState>>, // Add state field
}

impl App {
    /// Create a new CLI application and determine initial state
    pub async fn new() -> Result<Self> {
        dotenv::dotenv().ok();
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@localhost:5432/air_quality".to_string()
        });
        let api_key = env::var("OPENAQ_KEY").map_err(|e| {
            error!("OPENAQ_KEY environment variable not set: {}", e);
            AppError::Env(e) // Use renamed variant Env
        })?;

        let db = Database::new(&database_url).await?;
        let api_client = OpenAQClient::new(api_key);
        let mock_provider = MockDataProvider::new();

        // Determine initial state
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
            state: Arc::new(Mutex::new(initial_state)), // Initialize state
        })
    }

    /// Get the current application state
    pub async fn get_state(&self) -> AppState {
        self.state.lock().await.clone()
    }

    /// Run a specific command chosen by the user, updating state on success
    pub async fn run_command(&self, command: Commands) -> Result<()> {
        // Clone state Arc for potential update later
        let state_clone = Arc::clone(&self.state);

        let result = match command {
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

                // Update state if successful
                let mut state = state_clone.lock().await;
                if *state == AppState::Uninitialized {
                    *state = AppState::DbInitialized;
                    info!("App state updated to: {:?}", *state);
                } else {
                    // If already initialized or data imported, just confirm re-initialization
                    info!("Database re-initialized.");
                }
                Ok(()) // Return Ok explicitly inside the match arm
            },
            Commands::Import { days } => {
                // Prompt for days is now in main.rs
                self.import_data(days).await?;

                // Update state if successful
                let mut state = state_clone.lock().await;
                *state = AppState::DataImported; // Always update to DataImported after import
                info!("App state updated to: {:?}", *state);
                Ok(())
            },
            Commands::MostPolluted => {
                self.find_most_polluted().await?;
                Ok(())
            },
            Commands::Average(args) => {
                self.calculate_average(&args.country, args.days).await?;
                Ok(())
            },
            Commands::Measurements(args) => {
                // Prompt for country is now in main.rs
                self.get_measurements_table(&args.country).await?; // Renamed internal call
                Ok(())
            },
        }; // End of match

        result // Return the result from the match
    }

    /// Import air quality data into the database
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

    /// Find the most polluted country and display as table
    async fn find_most_polluted(&self) -> Result<()> {
        println!("{}", "Finding the most polluted country...".yellow());
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(StdDuration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {msg}")?
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message("Querying database...");
        let country_refs: Vec<&str> = COUNTRIES.to_vec();
        let result = self.db.get_most_polluted_country(&country_refs).await?;
        pb.finish_and_clear();

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
            Cell::new(&result.country)
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

    /// Calculate average air quality for a country and display as table
    async fn calculate_average(&self, country: &str, days: i64) -> Result<()> {
        let country_code = country.to_uppercase();
        if !COUNTRIES.contains(&country_code.as_str()) {
            return Err(AppError::Cli(format!(
                "Invalid country code. Must be one of: {:?}",
                COUNTRIES
            )));
        }
        println!(
            "{} {} {}-{}",
            "Calculating".yellow(),
            format!("{}", days).yellow().bold(),
            "day average for".yellow(),
            country_code.yellow().bold()
        );
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(StdDuration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {msg}")?
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message("Querying database...");
        let result = self.db.get_average_air_quality(&country_code, days).await?;
        pb.finish_and_clear();

        println!(
            "{}-{} {} ({} {})",
            format!("{}", days).bold(),
            "day average air quality for".green(),
            result.country.bold().cyan(),
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

    /// Get latest measurements by city for a country and display as a table
    async fn get_measurements_table(&self, country: &str) -> Result<()> {
        // Renamed function
        let country_code = country.to_uppercase();
        if !COUNTRIES.contains(&country_code.as_str()) {
            return Err(AppError::Cli(format!(
                "Invalid country code. Must be one of: {:?}",
                COUNTRIES
            )));
        }
        println!(
            "{} {}",
            "Fetching latest measurements by city for".yellow(),
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
                format!("No measurements found for cities in {}", country_code).yellow()
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
} // End of impl App

// --- Helper function to prompt for country ---
pub fn prompt_country() -> Result<String> {
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a country")
        .items(&COUNTRIES)
        .default(0)
        .interact()?;
    Ok(COUNTRIES[selection].to_string())
}

// --- Helper function to prompt for days ---
pub fn prompt_days() -> Result<i64> {
    let days: i64 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter number of days for history (e.g., 5)")
        .default(5)
        .validate_with(|input: &i64| -> std::result::Result<(), &str> {
            if *input > 0 && *input <= 365 {
                Ok(())
            } else {
                Err("Please enter a positive number of days (up to 365).")
            }
        })
        .interact_text()?;
    Ok(days)
}

// --- Tests ---
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::models::{
        CityLatestMeasurements, CountryAirQuality, DbMeasurement, Measurement, PollutionRanking,
    };
    use chrono::{Duration, Utc};
    use num_traits::FromPrimitive;
    use sqlx::types::Decimal;
    use std::sync::{Arc, Mutex};

    // --- Mock Database State ---
    #[derive(Clone, Default)]
    struct MockDbState {
        init_schema_called: bool,
        insert_measurements_called: bool,
        get_most_polluted_called: bool,
        get_average_called: bool,
        get_latest_by_city_called: bool,
        most_polluted_result: Option<Result<PollutionRanking>>,
        average_result: Option<Result<CountryAirQuality>>,
        latest_by_city_result: Option<Result<Vec<CityLatestMeasurements>>>,
    }

    // --- Mock Database ---
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
        fn expect_get_most_polluted(&self, result: Result<PollutionRanking>) {
            self.state.lock().unwrap().most_polluted_result = Some(result);
        }
        fn expect_get_average(&self, result: Result<CountryAirQuality>) {
            self.state.lock().unwrap().average_result = Some(result);
        }
        fn expect_get_latest_by_city(&self, result: Result<Vec<CityLatestMeasurements>>) {
            self.state.lock().unwrap().latest_by_city_result = Some(result);
        }
        async fn init_schema(&self) -> Result<()> {
            self.state.lock().unwrap().init_schema_called = true;
            Ok(())
        }
        async fn insert_measurements(&self, _measurements: &[Measurement]) -> Result<()> {
            self.state.lock().unwrap().insert_measurements_called = true;
            Ok(())
        }
        async fn get_most_polluted_country(&self, _countries: &[&str]) -> Result<PollutionRanking> {
            let mut state = self.state.lock().unwrap();
            state.get_most_polluted_called = true;
            state.most_polluted_result.take().unwrap_or_else(|| {
                panic!("MockDatabase::get_most_polluted_country called without expectation")
            })
        }
        async fn get_average_air_quality(
            &self,
            _country: &str,
            _days: i64,
        ) -> Result<CountryAirQuality> {
            let mut state = self.state.lock().unwrap();
            state.get_average_called = true;
            state.average_result.take().unwrap_or_else(|| {
                panic!("MockDatabase::get_average_air_quality called without expectation")
            })
        }
        async fn get_latest_measurements_by_city(
            &self,
            _country: &str,
        ) -> Result<Vec<CityLatestMeasurements>> {
            let mut state = self.state.lock().unwrap();
            state.get_latest_by_city_called = true;
            state.latest_by_city_result.take().unwrap_or_else(|| {
                panic!("MockDatabase::get_latest_measurements_by_city called without expectation")
            })
        }
        // Added mock checks for state detection
        async fn is_schema_initialized(&self) -> Result<bool> {
            Ok(self.state.lock().unwrap().init_schema_called)
        }
        async fn has_data_imported(&self) -> Result<bool> {
            Ok(self.state.lock().unwrap().insert_measurements_called)
        }
    }

    // --- Test Application ---
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
        async fn run_command(&self, command: Commands) -> Result<()> {
            match command {
                Commands::InitDb => self.run_init_db().await,
                Commands::Import { days } => self.run_import(days).await,
                Commands::MostPolluted => self.run_most_polluted().await,
                Commands::Average(args) => self.run_average(&args.country, args.days).await,
                Commands::Measurements(args) => self.run_measurements_table(&args.country).await, // Renamed call
            }
        }
        async fn run_init_db(&self) -> Result<()> {
            self.db.init_schema().await?;
            Ok(())
        }
        async fn run_import(&self, days: i64) -> Result<()> {
            self.db.init_schema().await?;
            let end_date = Utc::now();
            let start_date = end_date - Duration::days(days);
            for country in COUNTRIES.iter() {
                let mock_measurements = self
                    .mock_provider
                    .get_measurements_for_country_in_date_range(country, start_date, end_date)?;
                self.db.insert_measurements(&mock_measurements).await?;
            }
            Ok(())
        }
        async fn run_most_polluted(&self) -> Result<()> {
            let country_refs: Vec<&str> = COUNTRIES.iter().copied().collect();
            let _result = self.db.get_most_polluted_country(&country_refs).await?;
            Ok(())
        }
        async fn run_average(&self, country: &str, days: i64) -> Result<()> {
            let country_code = country.to_uppercase();
            if !COUNTRIES.contains(&country_code.as_str()) {
                return Err(AppError::Cli(format!("Invalid country code: {}", country)));
            }
            let _result = self.db.get_average_air_quality(&country_code, days).await?;
            Ok(())
        }
        // Renamed test function
        async fn run_measurements_table(&self, country: &str) -> Result<()> {
            let country_code = country.to_uppercase();
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

    // --- Tests ---
    #[tokio::test]
    async fn test_cmd_init_db() {
        let app = TestApp::new();
        let command = Commands::InitDb;
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().init_schema_called);
    }

    #[tokio::test]
    async fn test_cmd_import() {
        let app = TestApp::new();
        let command = Commands::Import { days: 3 };
        app.db
            .expect_get_most_polluted(Ok(PollutionRanking::new("PK"))); // Need expectation for init check
        app.db.expect_get_average(Ok(CountryAirQuality {
            country: "NL".to_string(),
            avg_pm25: None,
            avg_pm10: None,
            avg_o3: None,
            avg_no2: None,
            avg_so2: None,
            avg_co: None,
            measurement_count: 0,
        })); // Need expectation for init check
        app.db.expect_get_latest_by_city(Ok(vec![])); // Need expectation for init check

        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().init_schema_called);
        assert!(app.db.state.lock().unwrap().insert_measurements_called);
    }

    #[tokio::test]
    async fn test_cmd_most_polluted() {
        let app = TestApp::new();
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
        assert!(app.db.state.lock().unwrap().get_most_polluted_called);
    }

    #[tokio::test]
    async fn test_cmd_average_valid_country() {
        let app = TestApp::new();
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
        app.db.expect_get_average(Ok(expected_average));
        let command = Commands::Average(AverageArgs {
            country: "NL".to_string(),
            days: 5,
        });
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().get_average_called);
    }

    #[tokio::test]
    async fn test_cmd_average_invalid_country() {
        let app = TestApp::new();
        let command = Commands::Average(AverageArgs {
            country: "XX".to_string(),
            days: 5,
        });
        let result = app.run_command(command).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            AppError::Cli(msg) => assert!(msg.contains("Invalid country code")),
            _ => panic!("Expected CliError"),
        }
        assert!(!app.db.state.lock().unwrap().get_average_called);
    }

    #[tokio::test]
    async fn test_cmd_measurements_valid_country() {
        let app = TestApp::new();
        let expected_city_measurements = vec![];
        app.db
            .expect_get_latest_by_city(Ok(expected_city_measurements));
        let command = Commands::Measurements(MeasurementsArgs {
            country: "DE".to_string(),
        });
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().get_latest_by_city_called);
    }

    #[tokio::test]
    async fn test_cmd_measurements_invalid_country() {
        let app = TestApp::new();
        let command = Commands::Measurements(MeasurementsArgs {
            country: "YY".to_string(),
        });
        let result = app.run_command(command).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            AppError::Cli(msg) => assert!(msg.contains("Invalid country code")),
            _ => panic!("Expected CliError"),
        }
        assert!(!app.db.state.lock().unwrap().get_latest_by_city_called);
    }
}
