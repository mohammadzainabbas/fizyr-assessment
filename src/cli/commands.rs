use crate::api::{MockDataProvider, OpenAQClient};
use crate::db::Database;
use crate::error::{AppError, Result};
use chrono::{Duration, Utc};
// Removed clap imports
use colored::*;
use dialoguer::{theme::ColorfulTheme, Input, Select}; // Added Input, Select, ColorfulTheme
use indicatif::{ProgressBar, ProgressStyle}; // Added indicatif
use std::env;
use std::time::Duration as StdDuration; // Added StdDuration for progress bar
use tracing::{error, info};

/// List of countries to consider for pollution analysis
pub const COUNTRIES: [&str; 6] = [
    "NL", // Netherlands
    "DE", // Germany
    "FR", // France
    "GR", // Greece
    "ES", // Spain
    "PK", // Pakistan
];

// Removed Cli struct definition

// Keep Commands enum, but remove clap attributes
#[derive(Debug, Clone)] // Added Clone
pub enum Commands {
    /// Initialize the database schema
    InitDb,

    /// Import recent air quality data into the database
    Import { days: i64 },

    /// Find the most polluted country among Netherlands, Germany, France, Greece, Spain, and Pakistan
    MostPolluted,

    /// Calculate average air quality for a specific country
    Average(AverageArgs), // Keep using the struct for organization

    /// Get all measurements for a specific country
    Measurements(MeasurementsArgs), // Keep using the struct for organization
}

// Keep Args structs, but remove clap attributes
#[derive(Debug, Clone)] // Added Clone
pub struct AverageArgs {
    pub country: String,
    pub days: i64,
}

#[derive(Debug, Clone)] // Added Clone
pub struct MeasurementsArgs {
    pub country: String,
}

/// CLI application
pub struct App {
    db: Database,
    api_client: OpenAQClient,
    mock_provider: MockDataProvider,
}

impl App {
    /// Create a new CLI application
    pub async fn new() -> Result<Self> {
        // Load environment variables
        dotenv::dotenv().ok();

        // Check for required environment variables
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@localhost:5432/air_quality".to_string()
        });

        let api_key = env::var("OPENAQ_KEY").map_err(|e| {
            error!("OPENAQ_KEY environment variable not set: {}", e);
            AppError::Env(e) // Use renamed variant Env
        })?;

        // Connect to the database
        let db = Database::new(&database_url).await?;

        // Create API client
        let api_client = OpenAQClient::new(api_key);

        // Create mock data provider
        let mock_provider = MockDataProvider::new();

        Ok(Self {
            db,
            api_client,
            mock_provider,
        })
    }

    /// Run a specific command chosen by the user
    pub async fn run_command(&self, command: Commands) -> Result<()> {
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
            },
            Commands::Import { days } => {
                // Prompt for days if needed (or handle default) - This logic is now in main.rs
                // For now, assume 'days' is passed correctly
                self.import_data(days).await?;
            },
            Commands::MostPolluted => {
                self.find_most_polluted().await?;
            },
            Commands::Average(args) => {
                // Prompt for country/days is now in main.rs
                self.calculate_average(&args.country, args.days).await?;
            },
            Commands::Measurements(args) => {
                // Prompt for country is now in main.rs
                self.get_measurements(&args.country).await?;
            },
        }
        Ok(())
    }

    /// Import air quality data into the database
    async fn import_data(&self, days: i64) -> Result<()> {
        println!(
            "{} {}",
            "Importing data for the last".yellow(),
            format!("{} days", days).yellow().bold()
        );

        // --- Progress Bar Setup ---
        let pb = ProgressBar::new((COUNTRIES.len() * 2) as u64); // 1 step for fetch, 1 for insert per country
        pb.set_style(ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) {msg}",
        )?
        .progress_chars("#>-"));
        pb.enable_steady_tick(StdDuration::from_millis(100));
        // --------------------------

        // First ensure the database schema exists (maybe skip progress bar here or make it quick)
        info!("Ensuring database schema exists...");
        self.db.init_schema().await?; // Consider if this needs its own progress indication

        // Calculate the date range
        let end_date = Utc::now();
        let start_date = end_date - Duration::days(days);

        // For each country, fetch and import data
        for country in COUNTRIES.iter() {
            let country_str = country.to_string(); // Clone for async block
            pb.set_message(format!("Fetching data for {}...", country_str));

            // Try to fetch data from the API, if it fails, use the mock provider
            let measurements = match self
                .api_client
                .get_measurements_for_country_in_date_range(country, start_date, end_date)
                .await
            {
                Ok(measurements) => {
                    info!(
                        "Fetched {} measurements for {} from API",
                        measurements.len(),
                        country
                    );
                    measurements
                },
                Err(e) => {
                    // If API fails, use mock data
                    info!("API request failed: {}. Using mock data instead.", e);
                    let mock_measurements = self
                        .mock_provider
                        .get_measurements_for_country_in_date_range(
                            &country_str,
                            start_date,
                            end_date,
                        )?;
                    info!(
                        "Generated {} mock measurements for {}",
                        mock_measurements.len(),
                        country_str
                    );
                    mock_measurements
                },
            };
            pb.inc(1); // Increment progress after fetch/mock

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
            pb.inc(1); // Increment progress after insert
        }

        pb.finish_with_message("Data import completed successfully!".to_string());
        info!("Data import completed successfully");

        Ok(())
    }

    /// Find the most polluted country
    async fn find_most_polluted(&self) -> Result<()> {
        println!("{}", "Finding the most polluted country...".yellow());
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(StdDuration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {msg}")?
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message("Querying database...");

        let country_refs: Vec<&str> = COUNTRIES.to_vec(); // Use .to_vec() instead
        let result = self.db.get_most_polluted_country(&country_refs).await?;

        pb.finish_and_clear(); // Clear spinner before printing results

        println!(
            "{} {}",
            "Most polluted country:".green(),
            result.country.bold()
        );
        println!(
            "{} {:.2}",
            "Pollution index:".green(),
            result.pollution_index
        );

        if let Some(pm25) = result.pm25_avg {
            println!("  PM2.5 average: {:.2} µg/m³", pm25);
        }

        if let Some(pm10) = result.pm10_avg {
            println!("  PM10 average: {:.2} µg/m³", pm10);
        }

        Ok(())
    }

    /// Calculate average air quality for a country
    async fn calculate_average(&self, country: &str, days: i64) -> Result<()> {
        let country_code = country.to_uppercase();

        // Validation might be better handled in main.rs before calling this
        if !COUNTRIES.contains(&country_code.as_str()) {
            return Err(AppError::Cli(format!(
                // Use renamed variant Cli
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
            "{}-{} {}",
            format!("{}", days).bold(),
            "day average air quality for".green(),
            result.country.bold()
        );
        println!("Based on {} measurements", result.measurement_count);
        println!("------------------------------------------");

        if let Some(pm25) = result.avg_pm25 {
            println!("  PM2.5: {:.2} µg/m³", pm25);
        }

        if let Some(pm10) = result.avg_pm10 {
            println!("  PM10: {:.2} µg/m³", pm10);
        }

        if let Some(o3) = result.avg_o3 {
            println!("  O3: {:.2} µg/m³", o3);
        }

        if let Some(no2) = result.avg_no2 {
            println!("  NO2: {:.2} µg/m³", no2);
        }

        if let Some(so2) = result.avg_so2 {
            println!("  SO2: {:.2} µg/m³", so2);
        }

        if let Some(co) = result.avg_co {
            println!("  CO: {:.2} µg/m³", co);
        }

        Ok(())
    }

    /// Get all measurements for a country
    async fn get_measurements(&self, country: &str) -> Result<()> {
        let country_code = country.to_uppercase();

        // Validation might be better handled in main.rs
        if !COUNTRIES.contains(&country_code.as_str()) {
            return Err(AppError::Cli(format!(
                // Use renamed variant Cli
                "Invalid country code. Must be one of: {:?}",
                COUNTRIES
            )));
        }

        println!(
            "{} {}",
            "Fetching measurements for".yellow(),
            country_code.yellow().bold()
        );
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(StdDuration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {msg}")?
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message("Querying database...");

        let measurements = self.db.get_measurements_for_country(&country_code).await?;

        pb.finish_and_clear();

        println!("Measurements for {}", country_code.bold());
        println!("Total measurements: {}", measurements.len());
        println!("------------------------------------------");

        for (i, m) in measurements.iter().enumerate() {
            if i >= 20 {
                // Show first 20
                println!(
                    "... and {} more (showing first 20 only)",
                    measurements.len() - 20
                );
                break;
            }

            println!(
                "[{}] {} - {}: {} {} {}", // Removed extra {} between parameter and colon
                m.date_utc.format("%Y-%m-%d %H:%M").to_string().dimmed(), // Format date
                m.location.cyan(),
                m.parameter.blue(),
                m.value,
                m.unit.dimmed(),
                m.city.as_deref().unwrap_or("").italic() // Italicize city
            );
        }

        Ok(())
    }
} // End of impl App

// --- Helper function to prompt for country ---
// Moved outside impl App to be a free function in the module
pub fn prompt_country() -> Result<String> {
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a country")
        .items(&COUNTRIES)
        .default(0)
        .interact()?;
    Ok(COUNTRIES[selection].to_string())
}

// --- Helper function to prompt for days ---
// Moved outside impl App
pub fn prompt_days() -> Result<i64> {
    let days: i64 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter number of days for history (e.g., 5)")
        .default(5)
        .validate_with(|input: &i64| -> std::result::Result<(), &str> {
            if *input > 0 && *input <= 365 {
                // Example validation
                Ok(())
            } else {
                Err("Please enter a positive number of days (up to 365).")
            }
        })
        .interact_text()?;
    Ok(days)
}

// --- Tests ---
// Need to adapt tests to call `run_command` with enum variants
// instead of parsing CLI args. Mocking dialoguer/indicatif is complex,
// so focus on testing the core logic triggered by commands.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::models::{CountryAirQuality, DbMeasurement, Measurement, PollutionRanking};
    use chrono::{Duration, Utc};
    use num_traits::FromPrimitive; // Correct import path for FromPrimitive
    use sqlx::types::Decimal; // Correct import path for Decimal
    use std::sync::{Arc, Mutex};

    // --- Mock Database State --- (Keep as is)
    // Stores expected results and tracks calls for the mock database
    #[derive(Clone, Default)]
    struct MockDbState {
        init_schema_called: bool,
        insert_measurements_called: bool,
        get_most_polluted_called: bool,
        get_average_called: bool,
        get_measurements_called: bool,
        // Store expected return values for query methods
        most_polluted_result: Option<Result<PollutionRanking>>,
        average_result: Option<Result<CountryAirQuality>>,
        measurements_result: Option<Result<Vec<DbMeasurement>>>,
    }

    // --- Mock Database ---
    // A simple mock database that uses the state above
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
        fn expect_get_most_polluted(&self, result: Result<PollutionRanking>) {
            self.state.lock().unwrap().most_polluted_result = Some(result);
        }
        fn expect_get_average(&self, result: Result<CountryAirQuality>) {
            self.state.lock().unwrap().average_result = Some(result);
        }
        fn expect_get_measurements(&self, result: Result<Vec<DbMeasurement>>) {
            self.state.lock().unwrap().measurements_result = Some(result);
        }

        // Mocked database operations used by TestApp (Keep as is)
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
            state
                .most_polluted_result
                .take() // Consume the expected result
                .unwrap_or_else(|| {
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

        async fn get_measurements_for_country(&self, _country: &str) -> Result<Vec<DbMeasurement>> {
            let mut state = self.state.lock().unwrap();
            state.get_measurements_called = true;
            state.measurements_result.take().unwrap_or_else(|| {
                panic!("MockDatabase::get_measurements_for_country called without expectation")
            })
        }
    }

    // --- Test Application ---
    // Uses the MockDatabase and MockDataProvider
    struct TestApp {
        db: MockDatabase,
        mock_provider: MockDataProvider, // Use the existing mock provider
    }

    impl TestApp {
        fn new() -> Self {
            Self {
                db: MockDatabase::new(),
                mock_provider: MockDataProvider::new(),
            }
        }

        // --- Re-implemented command handlers using mocks ---
        // These mirror the logic of the real App methods but use the mock DB
        // We pass the command enum directly now.

        async fn run_command(&self, command: Commands) -> Result<()> {
            match command {
                Commands::InitDb => self.run_init_db().await,
                Commands::Import { days } => self.run_import(days).await,
                Commands::MostPolluted => self.run_most_polluted().await,
                Commands::Average(args) => self.run_average(&args.country, args.days).await,
                Commands::Measurements(args) => self.run_measurements(&args.country).await,
            }
        }

        // Individual command handlers for testing
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
            // Assertions on _result could be added if needed
            Ok(())
        }

        async fn run_average(&self, country: &str, days: i64) -> Result<()> {
            let country_code = country.to_uppercase();
            if !COUNTRIES.contains(&country_code.as_str()) {
                return Err(AppError::CliError(format!(
                    "Invalid country code: {}",
                    country
                )));
            }
            let _result = self.db.get_average_air_quality(&country_code, days).await?;
            Ok(())
        }

        async fn run_measurements(&self, country: &str) -> Result<()> {
            let country_code = country.to_uppercase();
            if !COUNTRIES.contains(&country_code.as_str()) {
                return Err(AppError::CliError(format!(
                    "Invalid country code: {}",
                    country
                )));
            }
            let _measurements = self.db.get_measurements_for_country(&country_code).await?;
            Ok(())
        }
    }

    // Helper to create a DbMeasurement for tests
    // Imports moved to the top of the module

    fn create_db_measurement(
        country: &str,
        parameter: &str,
        value: f64, // Keep input as f64 for convenience
        days_ago: i64,
    ) -> DbMeasurement {
        DbMeasurement {
            id: Some(rand::random::<i32>().abs()), // Random ID for test
            location_id: 12345,
            location: format!("Test DB Loc {}", country),
            parameter: parameter.to_string(),
            value: Decimal::from_f64(value).unwrap_or_default(), // Convert to Decimal
            unit: "µg/m³".to_string(),
            date_utc: Utc::now() - Duration::days(days_ago),
            date_local: format!(
                "{}",
                (Utc::now() - Duration::days(days_ago)).format("%Y-%m-%dT%H:%M:%S%z")
            ),
            country: country.to_string(),
            city: Some(format!("Test DB City {}", country)),
            latitude: Some(52.0),
            longitude: Some(5.0),
        }
    }

    // --- Tests ---
    // --- Updated Tests ---
    #[tokio::test]
    async fn test_cmd_init_db() {
        // Renamed test
        let app = TestApp::new();
        let command = Commands::InitDb;
        let result = app.run_command(command).await; // Use run_command
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().init_schema_called);
    }

    #[tokio::test]
    async fn test_cmd_import() {
        // Renamed test
        let app = TestApp::new();
        let command = Commands::Import { days: 3 }; // Create enum variant
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().init_schema_called);
        assert!(app.db.state.lock().unwrap().insert_measurements_called);
    }

    #[tokio::test]
    async fn test_cmd_most_polluted() {
        // Renamed test
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
        // Renamed test
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
            // Create enum variant
            country: "NL".to_string(),
            days: 5,
        });
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().get_average_called);
    }

    #[tokio::test]
    async fn test_cmd_average_invalid_country() {
        // Renamed test
        let app = TestApp::new();
        // No DB expectation needed

        let command = Commands::Average(AverageArgs {
            country: "XX".to_string(), // Invalid country
            days: 5,
        });
        let result = app.run_command(command).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            AppError::CliError(msg) => assert!(msg.contains("Invalid country code")),
            _ => panic!("Expected CliError"),
        }
        assert!(!app.db.state.lock().unwrap().get_average_called);
    }

    #[tokio::test]
    async fn test_cmd_measurements_valid_country() {
        // Renamed test
        let app = TestApp::new();
        let expected_measurements = vec![
            create_db_measurement("DE", "pm25", 18.0, 1),
            create_db_measurement("DE", "pm10", 28.0, 1),
        ];
        app.db.expect_get_measurements(Ok(expected_measurements));

        let command = Commands::Measurements(MeasurementsArgs {
            // Create enum variant
            country: "DE".to_string(),
        });
        let result = app.run_command(command).await;
        assert!(result.is_ok());
        assert!(app.db.state.lock().unwrap().get_measurements_called);
    }

    #[tokio::test]
    async fn test_cmd_measurements_invalid_country() {
        // Renamed test
        let app = TestApp::new();
        // No DB expectation needed

        let command = Commands::Measurements(MeasurementsArgs {
            country: "YY".to_string(), // Invalid country
        });
        let result = app.run_command(command).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            AppError::CliError(msg) => assert!(msg.contains("Invalid country code")),
            _ => panic!("Expected CliError"),
        }
        assert!(!app.db.state.lock().unwrap().get_measurements_called);
    }
}
