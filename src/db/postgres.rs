//! Provides PostgreSQL database interaction functionalities using `sqlx`.
//!
//! Includes capabilities for establishing connection pools, initializing the database schema,
//! inserting air quality measurements, and executing various analytical queries.
//! Also contains integration tests for database operations (requires the `integration-tests` feature).

use crate::error::{AppError, Result};
use crate::models::{
    CityLatestMeasurements, CountryAirQuality, DbMeasurement, Measurement, PollutionRanking,
};
use rayon::prelude::*; // Used for parallel data transformation
use sqlx::{postgres::PgPoolOptions, Pool, Postgres, Row};
use tracing::{debug, error, info};

/// Represents the database connection pool and provides methods for database operations.
///
/// Holds a `sqlx::Pool` for efficient connection management.
pub struct Database {
    pool: Pool<Postgres>,
}

impl Database {
    /// Creates a new `Database` instance by establishing a connection pool.
    ///
    /// # Arguments
    ///
    /// * `database_url` - The connection string for the PostgreSQL database.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Db` if the connection pool cannot be established.
    pub async fn new(database_url: &str) -> Result<Self> {
        info!("Connecting to database..."); // Simplified log message

        let pool = PgPoolOptions::new()
            .max_connections(10) // Configure maximum number of connections in the pool
            .connect(database_url)
            .await
            .map_err(|e| {
                error!("Failed to connect to database: {}", e);
                AppError::Db(e.into()) // Wrap sqlx::Error into AppError::Db
            })?;

        info!("Connected to database successfully");
        Ok(Self { pool })
    }

    /// Initializes the database schema by creating the `measurements` table and necessary indexes.
    ///
    /// Uses `CREATE TABLE IF NOT EXISTS` and `CREATE INDEX IF NOT EXISTS` to be idempotent,
    /// meaning it can be safely run multiple times without causing errors if the objects already exist.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Db` if any SQL query fails during schema creation.
    pub async fn init_schema(&self) -> Result<()> {
        info!("Initializing database schema (if necessary)...");

        // Create the main table for storing air quality measurements.
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS measurements (
                id SERIAL PRIMARY KEY,
                location_id BIGINT NOT NULL,
                location TEXT NOT NULL,
                parameter TEXT NOT NULL,
                value NUMERIC NOT NULL, -- Using NUMERIC for precise storage
                unit TEXT NOT NULL,
                date_utc TIMESTAMPTZ NOT NULL,
                date_local TEXT NOT NULL, -- Storing local time as text as provided by API
                country TEXT NOT NULL,
                city TEXT,
                latitude DOUBLE PRECISION,
                longitude DOUBLE PRECISION,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW() -- Timestamp of insertion
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("Failed to create measurements table: {}", e);
            AppError::Db(e.into())
        })?;

        // Create indexes to speed up common query patterns.
        // Index on country for filtering by country.
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_measurements_country ON measurements(country)"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("Failed to create country index: {}", e);
            AppError::Db(e.into())
        })?;

        // Index on parameter for filtering by pollutant type.
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_measurements_parameter ON measurements(parameter)"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("Failed to create parameter index: {}", e);
            AppError::Db(e.into())
        })?;

        // Index on date_utc for time-based filtering and ordering.
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_measurements_date_utc ON measurements(date_utc)"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("Failed to create date index: {}", e);
            AppError::Db(e.into())
        })?;

        info!("Database schema initialized successfully");
        Ok(())
    }

    /// Inserts a batch of `Measurement` records into the database.
    ///
    /// Converts API `Measurement` structs to `DbMeasurement` in parallel using Rayon.
    /// Executes insertions within a single database transaction for atomicity.
    /// Uses `ON CONFLICT DO NOTHING` to silently ignore potential duplicate entries
    /// (based on unique constraints, though none are explicitly defined here besides PRIMARY KEY).
    ///
    /// # Arguments
    ///
    /// * `measurements` - A slice of `Measurement` structs fetched from the API.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Db` if the transaction fails to begin, commit, or if any
    /// individual insertion query fails.
    pub async fn insert_measurements(&self, measurements: &[Measurement]) -> Result<()> {
        if measurements.is_empty() {
            debug!("No measurements provided for insertion.");
            return Ok(());
        }

        info!(
            "Preparing to insert {} measurements into database...",
            measurements.len()
        );

        // Convert API measurements to DB format in parallel for potential performance gain.
        let db_measurements: Vec<DbMeasurement> = measurements
            .par_iter() // Use Rayon for parallel iteration
            .map(|m| DbMeasurement::from(m.clone()))
            .collect();

        // Use a transaction to ensure all measurements are inserted or none are.
        let mut tx = self.pool.begin().await.map_err(|e| {
            error!("Failed to begin database transaction: {}", e);
            AppError::Db(e.into())
        })?;

        // Iterate and execute INSERT query for each measurement.
        for m in &db_measurements {
            // Note: Consider using `sqlx::query!` macro for compile-time checks if not dynamic.
            // Using `ON CONFLICT DO NOTHING` assumes duplicates are okay to ignore.
            // If specific conflict handling (e.g., update) is needed, adjust the query.
            sqlx::query(
                r#"
                INSERT INTO measurements
                (location_id, location, parameter, value, unit, date_utc, date_local, country, city, latitude, longitude)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(m.location_id)
            .bind(&m.location)
            .bind(&m.parameter)
            .bind(m.value) // Binds the Decimal type
            .bind(&m.unit)
            .bind(m.date_utc)
            .bind(&m.date_local)
            .bind(&m.country)
            .bind(&m.city)
            .bind(m.latitude)
            .bind(m.longitude)
            .execute(&mut *tx) // Execute within the transaction
            .await
            .map_err(|e| {
                // Log specific insertion error, but transaction will likely be rolled back.
                error!("Failed to insert measurement record: {}", e);
                AppError::Db(e.into())
            })?;
        }

        // Commit the transaction if all insertions were successful.
        tx.commit().await.map_err(|e| {
            error!("Failed to commit database transaction: {}", e);
            AppError::Db(e.into())
        })?;

        info!("Successfully inserted {} measurements", measurements.len());
        Ok(())
    }

    /// Finds the most polluted country among a given list based on recent PM2.5 and PM10 data.
    ///
    /// Calculates a pollution index: `(avg_pm25 * 1.5) + avg_pm10` using data from the last 7 days.
    /// Returns the country with the highest index.
    ///
    /// # Arguments
    ///
    /// * `countries` - A slice of country codes (e.g., "NL", "DE") to consider.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Db` if the query fails. Returns a default `PollutionRanking` with index 0
    /// if no relevant data is found for any of the specified countries in the last 7 days.
    pub async fn get_most_polluted_country(&self, countries: &[&str]) -> Result<PollutionRanking> {
        if countries.is_empty() {
            // Handle case where no countries are provided, perhaps return an error or default.
            // For now, returning a default for "Unknown". Consider a specific error.
            error!("No countries provided to find the most polluted.");
            return Ok(PollutionRanking::new("Unknown"));
        }
        info!("Finding the most polluted country among: {:?}", countries);

        // Join country codes into a comma-separated string suitable for SQL IN clause.
        // Note: This approach is generally safe for known country codes but be wary of SQL injection
        // if `countries` could come from untrusted input without sanitization. Binding is safer.
        let countries_list = countries.join("','");

        // SQL Query Explanation:
        // 1. CTE `latest_data`: Calculates the average value for PM2.5 and PM10 for each country
        //    within the last 7 days.
        // 2. Main Query: Groups by country, calculates the weighted pollution index,
        //    extracts the specific PM2.5 and PM10 averages using MAX(CASE...), orders by the index descending,
        //    and takes the top result.
        let query = format!(
            r#"
            WITH latest_data AS (
                SELECT
                    country,
                    parameter,
                    AVG(value::DOUBLE PRECISION) as avg_value -- Cast NUMERIC to float for calculation
                FROM measurements
                WHERE
                    country IN ('{}') -- Injecting the list here
                    AND parameter IN ('pm25', 'pm10')
                    AND date_utc > NOW() - INTERVAL '7 days'
                GROUP BY country, parameter
            )
            SELECT
                country,
                -- Calculate weighted pollution index (PM2.5 weighted higher)
                SUM(CASE WHEN parameter = 'pm25' THEN avg_value * 1.5 ELSE 0 END)::DOUBLE PRECISION +
                SUM(CASE WHEN parameter = 'pm10' THEN avg_value ELSE 0 END)::DOUBLE PRECISION as pollution_index,
                -- Extract average PM2.5 and PM10 values for the result
                MAX(CASE WHEN parameter = 'pm25' THEN avg_value ELSE NULL END)::DOUBLE PRECISION as pm25_avg,
                MAX(CASE WHEN parameter = 'pm10' THEN avg_value ELSE NULL END)::DOUBLE PRECISION as pm10_avg
            FROM latest_data
            GROUP BY country
            ORDER BY pollution_index DESC
            LIMIT 1
            "#,
            countries_list // Use the joined list
        );

        // Execute the query, mapping the result to a tuple.
        let result = sqlx::query_as::<_, (String, f64, Option<f64>, Option<f64>)>(&query)
            .fetch_optional(&self.pool) // Use fetch_optional as there might be no data
            .await
            .map_err(|e| {
                error!("Failed to query most polluted country: {}", e);
                AppError::Db(e.into())
            })?;

        match result {
            Some((country, pollution_index, pm25_avg, pm10_avg)) => {
                info!(
                    "Most polluted country determined: {} with index: {}",
                    country, pollution_index
                );
                Ok(PollutionRanking {
                    country,
                    pollution_index,
                    pm25_avg,
                    pm10_avg,
                })
            },
            None => {
                // If no data found for any country in the list within the time frame.
                let default_country = countries.first().map_or("Unknown", |c| *c);
                error!(
                    "No recent pollution data (PM2.5/PM10) found for the specified countries: {:?}",
                    countries
                );
                // Return a default ranking for the first country in the list (or "Unknown").
                Ok(PollutionRanking::new(default_country))
            },
        }
    }

    /// Calculates the 5-day average air quality for a specific country.
    ///
    /// Averages values for PM2.5, PM10, O3, NO2, SO2, and CO from the last 5 days.
    ///
    /// # Arguments
    ///
    /// * `country` - The 2-letter country code.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Db` if the query fails. Returns default `CountryAirQuality`
    /// with zero counts and None averages if no data is found for the country in the last 5 days.
    pub async fn get_average_air_quality(&self, country: &str) -> Result<CountryAirQuality> {
        info!("Calculating 5-day average air quality for {}", country);

        // SQL Query Explanation:
        // Uses conditional aggregation (AVG(CASE...)) to calculate the average for each
        // parameter separately within a single query, filtered by country and the last 5 days.
        // COUNT(*) gets the total number of measurements included in the averages.
        let query = r#"
        SELECT
            country,
            AVG(CASE WHEN parameter = 'pm25' THEN value::DOUBLE PRECISION ELSE NULL END) as avg_pm25,
            AVG(CASE WHEN parameter = 'pm10' THEN value::DOUBLE PRECISION ELSE NULL END) as avg_pm10,
            AVG(CASE WHEN parameter = 'o3' THEN value::DOUBLE PRECISION ELSE NULL END) as avg_o3,
            AVG(CASE WHEN parameter = 'no2' THEN value::DOUBLE PRECISION ELSE NULL END) as avg_no2,
            AVG(CASE WHEN parameter = 'so2' THEN value::DOUBLE PRECISION ELSE NULL END) as avg_so2,
            AVG(CASE WHEN parameter = 'co' THEN value::DOUBLE PRECISION ELSE NULL END) as avg_co,
            COUNT(*) as measurement_count
        FROM measurements
        WHERE
            country = $1 -- Use binding for country parameter
            AND date_utc > NOW() - INTERVAL '5 days' -- Hardcoded 5-day interval
        GROUP BY country
        "#;

        // Execute the query, binding the country parameter.
        let result = sqlx::query_as::<
            _,
            (
                String,      // country
                Option<f64>, // avg_pm25
                Option<f64>, // avg_pm10
                Option<f64>, // avg_o3
                Option<f64>, // avg_no2
                Option<f64>, // avg_so2
                Option<f64>, // avg_co
                i64,         // measurement_count
            ),
        >(query)
        .bind(country)
        .fetch_optional(&self.pool) // Use fetch_optional as there might be no data
        .await
        .map_err(|e| {
            error!("Failed to query average air quality for {}: {}", country, e);
            AppError::Db(e.into())
        })?;

        match result {
            Some((
                country_name, // Renamed to avoid conflict with input `country`
                avg_pm25,
                avg_pm10,
                avg_o3,
                avg_no2,
                avg_so2,
                avg_co,
                measurement_count,
            )) => {
                info!(
                    "Found 5-day average air quality data for {} ({} measurements)",
                    country_name, measurement_count
                );
                Ok(CountryAirQuality {
                    country: country_name,
                    avg_pm25,
                    avg_pm10,
                    avg_o3,
                    avg_no2,
                    avg_so2,
                    avg_co,
                    measurement_count,
                })
            },
            None => {
                // If no measurements found for the country in the last 5 days.
                info!("No recent air quality data found for {}", country);
                Ok(CountryAirQuality {
                    country: country.to_string(),
                    avg_pm25: None,
                    avg_pm10: None,
                    avg_o3: None,
                    avg_no2: None,
                    avg_so2: None,
                    avg_co: None,
                    measurement_count: 0,
                })
            },
        }
    }

    /// Gets the latest measurement for each parameter, grouped by city, for a specific country.
    ///
    /// Uses `DISTINCT ON` to efficiently find the latest record per city/parameter combination,
    /// then pivots the data using conditional aggregation (`MAX(CASE...)`) to structure the result.
    ///
    /// # Arguments
    ///
    /// * `country` - The 2-letter country code.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Db` if the query fails. Returns an empty Vec if no data is found.
    pub async fn get_latest_measurements_by_city(
        &self,
        country: &str,
    ) -> Result<Vec<CityLatestMeasurements>> {
        info!("Fetching latest measurements by city for {}", country);

        // SQL Query Explanation:
        // 1. CTE `latest_city_param`: Uses `DISTINCT ON (city, parameter)` ordered by `date_utc DESC`
        //    to select only the single latest row for each unique combination of city and parameter
        //    within the specified country.
        // 2. Main Query: Groups the results from the CTE by city. Uses `MAX(CASE...)` to pivot
        //    the parameter values into separate columns (pm25, pm10, etc.). `MAX(date_utc)` finds the
        //    most recent update timestamp among all parameters for that city.
        let query = r#"
        WITH latest_city_param AS (
            SELECT DISTINCT ON (city, parameter)
                city,
                parameter,
                value, -- The value from the latest record
                date_utc -- The timestamp from the latest record
            FROM measurements
            WHERE country = $1 AND city IS NOT NULL -- Filter by country, ignore null cities
            ORDER BY city, parameter, date_utc DESC -- Crucial for DISTINCT ON
        )
        SELECT
            city,
            -- Pivot parameter values into columns
            MAX(CASE WHEN parameter = 'pm25' THEN value ELSE NULL END) as pm25,
            MAX(CASE WHEN parameter = 'pm10' THEN value ELSE NULL END) as pm10,
            MAX(CASE WHEN parameter = 'o3' THEN value ELSE NULL END) as o3,
            MAX(CASE WHEN parameter = 'no2' THEN value ELSE NULL END) as no2,
            MAX(CASE WHEN parameter = 'so2' THEN value ELSE NULL END) as so2,
            MAX(CASE WHEN parameter = 'co' THEN value ELSE NULL END) as co,
            -- Find the overall latest update time for the city across all parameters
            MAX(date_utc) as last_updated
        FROM latest_city_param
        GROUP BY city
        ORDER BY city -- Order results alphabetically by city name
        "#;

        let results = sqlx::query_as::<_, CityLatestMeasurements>(query)
            .bind(country)
            .fetch_all(&self.pool) // Fetch all resulting rows
            .await
            .map_err(|e| {
                error!(
                    "Failed to fetch latest measurements by city for {}: {}",
                    country, e
                );
                AppError::Db(e.into())
            })?;

        info!(
            "Retrieved latest measurements for {} cities in {}",
            results.len(),
            country
        );
        Ok(results)
    }

    /// Checks if the `measurements` table exists in the database schema.
    ///
    /// Useful for determining application state (e.g., before allowing data import).
    ///
    /// # Errors
    ///
    /// Returns `AppError::Db` if the query to `information_schema.tables` fails.
    pub async fn is_schema_initialized(&self) -> Result<bool> {
        debug!("Checking if database schema is initialized...");
        let query = "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'measurements')";
        let result = sqlx::query(query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to check schema existence: {}", e);
                AppError::Db(e.into())
            })?;
        // Try to get the boolean result, default to false if extraction fails (shouldn't happen with EXISTS)
        let initialized = result.try_get::<bool, _>(0).unwrap_or(false);
        debug!("Schema initialized status: {}", initialized);
        Ok(initialized)
    }

    /// Checks if any data has been imported into the `measurements` table.
    ///
    /// First checks if the schema is initialized. If not, returns `Ok(false)`.
    /// Otherwise, checks if at least one row exists in the `measurements` table.
    /// Useful for determining application state.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Db` if any underlying database query fails.
    pub async fn has_data_imported(&self) -> Result<bool> {
        debug!("Checking if data has been imported...");
        // Ensure schema exists before checking for data.
        if !self.is_schema_initialized().await? {
            debug!("Schema not initialized, therefore no data imported.");
            return Ok(false);
        }
        // Check if at least one row exists in the table.
        let query = "SELECT EXISTS (SELECT 1 FROM measurements LIMIT 1)";
        let result = sqlx::query(query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to check for imported data: {}", e);
                AppError::Db(e.into())
            })?;
        let has_data = result.try_get::<bool, _>(0).unwrap_or(false);
        debug!("Data imported status: {}", has_data);
        Ok(has_data)
    }
}

// --- Integration Tests ---
// These tests interact with a real PostgreSQL database.
// They are gated by the `integration-tests` feature flag.
// Run using: `cargo test --features integration-tests`
// Requires a running PostgreSQL instance configured via DATABASE_URL env var.
#[cfg(test)]
#[cfg(feature = "integration-tests")] // Apply feature gate to the whole module
mod tests {
    use super::*; // Import items from parent module (Database, etc.)
    use crate::models::{Dates, Measurement};
    use chrono::{Duration, Utc};
    use num_traits::FromPrimitive; // Required for Decimal::from_f64
    use sqlx::types::Decimal;
    use sqlx::{PgPool, Row}; // PgPool is injected by #[sqlx::test]

    /// Helper function to create a `Measurement` instance for testing purposes.
    fn create_test_measurement(
        country: &str,
        parameter: &str,
        value: f64,
        days_ago: i64,
    ) -> Measurement {
        let timestamp = Utc::now() - Duration::days(days_ago);
        Measurement {
            location_id: rand::random(), // Use random ID for variety
            location: format!("Test Location {}", country),
            parameter: parameter.to_string(),
            value,
            unit: "µg/m³".to_string(),
            date: Dates {
                utc: timestamp,
                local: timestamp.to_rfc3339(),
            },
            country: country.to_string(),
            city: Some(format!("Test City {}", country)),
            coordinates: Some(Coordinates {
                // Add some coordinates
                latitude: Some(52.0),
                longitude: Some(5.0),
            }),
        }
    }

    /// Helper function to set up the database schema and insert standard test data.
    /// Ensures the schema exists before inserting data.
    async fn insert_test_data(pool: &PgPool) -> Result<()> {
        let db = Database { pool: pool.clone() };
        db.init_schema().await?; // Ensure schema exists

        let measurements = vec![
            // Netherlands data (recent)
            create_test_measurement("NL", "pm25", 15.0, 1),
            create_test_measurement("NL", "pm10", 25.0, 1),
            create_test_measurement("NL", "no2", 30.0, 1),
            // Germany data (recent)
            create_test_measurement("DE", "pm25", 18.0, 1),
            create_test_measurement("DE", "pm10", 28.0, 1),
            // Pakistan data (recent, higher pollution)
            create_test_measurement("PK", "pm25", 50.0, 1),
            create_test_measurement("PK", "pm10", 80.0, 1),
            // France data (older, outside 5-day window for avg test)
            create_test_measurement("FR", "pm25", 10.0, 6),
            // Greece data (recent)
            create_test_measurement("GR", "pm10", 22.0, 1),
            // Spain data (recent)
            create_test_measurement("ES", "pm25", 12.0, 1),
        ];
        db.insert_measurements(&measurements).await?;
        Ok(())
    }

    /// Tests the `init_schema` function to ensure the table and indexes are created correctly.
    #[sqlx::test] // Macro handles setting up transaction/pool for the test
    async fn test_init_schema(pool: PgPool) -> Result<()> {
        let db = Database { pool };
        info!("Running integration test: test_init_schema");
        let result = db.init_schema().await;
        assert!(result.is_ok(), "init_schema should succeed");

        // Verify table exists using information_schema
        let table_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'measurements')",
        )
        .fetch_one(&db.pool)
        .await?;
        assert!(
            table_exists,
            "measurements table should exist after init_schema"
        );

        // Verify indexes exist using pg_indexes
        let indexes = [
            "idx_measurements_country",
            "idx_measurements_parameter",
            "idx_measurements_date_utc",
        ];
        for index_name in indexes {
            let index_exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT FROM pg_indexes WHERE schemaname = 'public' AND indexname = $1)"
            )
            .bind(index_name)
            .fetch_one(&db.pool)
            .await?;
            assert!(
                index_exists,
                "Index {} should exist after init_schema",
                index_name
            );
        }

        Ok(())
    }

    /// Tests the `insert_measurements` function correctly inserts data.
    #[sqlx::test]
    async fn test_insert_measurements(pool: PgPool) -> Result<()> {
        info!("Running integration test: test_insert_measurements");
        let db = Database { pool };
        db.init_schema().await?; // Prerequisite: schema must exist

        let m1 = create_test_measurement("NL", "pm25", 10.5, 1);
        let m2 = create_test_measurement("DE", "pm10", 20.2, 1);
        let measurements = vec![m1.clone(), m2.clone()];

        let result = db.insert_measurements(&measurements).await;
        assert!(result.is_ok(), "insert_measurements should succeed");

        // Verify data count
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM measurements")
            .fetch_one(&db.pool)
            .await?;
        assert_eq!(count, 2, "Should be 2 measurements inserted");

        // Verify specific inserted data
        let row = sqlx::query_as::<_, DbMeasurement>(
            "SELECT * FROM measurements WHERE country = 'NL' AND parameter = 'pm25'",
        )
        .fetch_one(&db.pool)
        .await?;
        assert_eq!(row.country, "NL");
        assert_eq!(row.parameter, "pm25");
        // Compare Decimal values carefully
        assert_eq!(
            row.value,
            Decimal::from_f64(10.5).unwrap(),
            "Inserted value mismatch for NL pm25"
        );
        assert_eq!(row.location_id, m1.location_id); // Check other fields if necessary

        Ok(())
    }

    /// Tests the `get_most_polluted_country` function logic.
    #[sqlx::test]
    async fn test_get_most_polluted_country(pool: PgPool) -> Result<()> {
        info!("Running integration test: test_get_most_polluted_country");
        insert_test_data(&pool).await?; // Insert standard test data
        let db = Database { pool };

        let countries = ["NL", "DE", "FR", "GR", "ES", "PK"];
        let result = db.get_most_polluted_country(&countries).await?;

        // Expected calculation based on test data (pm25*1.5 + pm10):
        // PK: (50 * 1.5) + 80 = 75 + 80 = 155
        // DE: (18 * 1.5) + 28 = 27 + 28 = 55
        // NL: (15 * 1.5) + 25 = 22.5 + 25 = 47.5
        // ES: (12 * 1.5) + 0 = 18
        // GR: 0 + 22 = 22
        // FR: No recent data included in calculation
        assert_eq!(result.country, "PK", "Pakistan should be the most polluted");
        // Use tolerance for floating-point comparisons
        assert!(
            (result.pollution_index - 155.0).abs() < 1e-6,
            "Pollution index mismatch for PK"
        );
        assert!(
            result.pm25_avg.is_some(),
            "PM2.5 average should exist for PK"
        );
        assert!(
            (result.pm25_avg.unwrap() - 50.0).abs() < 1e-6,
            "PM2.5 average mismatch for PK"
        );
        assert!(
            result.pm10_avg.is_some(),
            "PM10 average should exist for PK"
        );
        assert!(
            (result.pm10_avg.unwrap() - 80.0).abs() < 1e-6,
            "PM10 average mismatch for PK"
        );

        // Test case with no recent data (only FR has old data)
        let result_fr = db.get_most_polluted_country(&["FR"]).await?;
        assert_eq!(
            result_fr.country, "FR",
            "Country should default to FR when no data"
        );
        assert!(
            (result_fr.pollution_index - 0.0).abs() < 1e-6,
            "Pollution index should be 0 for FR"
        );
        assert!(
            result_fr.pm25_avg.is_none(),
            "PM2.5 avg should be None for FR"
        );
        assert!(
            result_fr.pm10_avg.is_none(),
            "PM10 avg should be None for FR"
        );

        Ok(())
    }

    /// Tests the `get_average_air_quality` function logic over a 5-day period.
    #[sqlx::test]
    async fn test_get_average_air_quality(pool: PgPool) -> Result<()> {
        info!("Running integration test: test_get_average_air_quality");
        insert_test_data(&pool).await?;
        let db = Database { pool };

        // Test for NL (should have 3 recent measurements: pm25, pm10, no2)
        let result_nl = db.get_average_air_quality("NL").await?;
        assert_eq!(result_nl.country, "NL");
        assert_eq!(
            result_nl.measurement_count, 3,
            "NL should have 3 measurements in last 5 days"
        );
        assert!(result_nl.avg_pm25.is_some());
        assert!((result_nl.avg_pm25.unwrap() - 15.0).abs() < 1e-6);
        assert!(result_nl.avg_pm10.is_some());
        assert!((result_nl.avg_pm10.unwrap() - 25.0).abs() < 1e-6);
        assert!(result_nl.avg_no2.is_some());
        assert!((result_nl.avg_no2.unwrap() - 30.0).abs() < 1e-6);
        assert!(result_nl.avg_o3.is_none(), "NL should have no O3 data"); // No O3 data inserted

        // Test for FR (only old data exists, > 5 days ago)
        let result_fr = db.get_average_air_quality("FR").await?;
        assert_eq!(result_fr.country, "FR");
        assert_eq!(
            result_fr.measurement_count, 0,
            "FR should have 0 measurements in last 5 days"
        );
        assert!(result_fr.avg_pm25.is_none());

        // Test for a country with no data at all
        let result_xx = db.get_average_air_quality("XX").await?; // Assuming XX has no data
        assert_eq!(result_xx.country, "XX");
        assert_eq!(
            result_xx.measurement_count, 0,
            "XX should have 0 measurements"
        );

        Ok(())
    }

    /// Tests the `get_latest_measurements_by_city` function logic.
    #[sqlx::test]
    async fn test_get_latest_measurements_by_city(pool: PgPool) -> Result<()> {
        info!("Running integration test: test_get_latest_measurements_by_city");
        insert_test_data(&pool).await?; // Insert standard test data

        // Add slightly older data for NL to test DISTINCT ON logic
        let db = Database { pool };
        let older_nl_pm25 = create_test_measurement("NL", "pm25", 5.0, 2); // Older PM2.5 value
        let older_nl_o3 = create_test_measurement("NL", "o3", 40.0, 1); // O3 data (recent)
        db.insert_measurements(&[older_nl_pm25, older_nl_o3])
            .await?;

        let results_nl = db.get_latest_measurements_by_city("NL").await?;

        assert_eq!(results_nl.len(), 1, "Should only be one city entry for NL");
        let nl_city_data = &results_nl[0];
        assert_eq!(nl_city_data.city, "Test City NL");

        // Check latest values (should pick the most recent ones from insert_test_data or the added O3)
        assert!(nl_city_data.pm25.is_some());
        assert_eq!(
            nl_city_data.pm25.unwrap(),
            Decimal::from_f64(15.0).unwrap(),
            "Latest NL PM2.5 mismatch (should be 15.0, not 5.0)"
        );
        assert!(nl_city_data.pm10.is_some());
        assert_eq!(
            nl_city_data.pm10.unwrap(),
            Decimal::from_f64(25.0).unwrap(),
            "Latest NL PM10 mismatch"
        );
        assert!(nl_city_data.no2.is_some());
        assert_eq!(
            nl_city_data.no2.unwrap(),
            Decimal::from_f64(30.0).unwrap(),
            "Latest NL NO2 mismatch"
        );
        assert!(nl_city_data.o3.is_some());
        assert_eq!(
            nl_city_data.o3.unwrap(),
            Decimal::from_f64(40.0).unwrap(),
            "Latest NL O3 mismatch"
        ); // Check the added O3
        assert!(nl_city_data.so2.is_none(), "NL SO2 should be None");
        assert!(nl_city_data.co.is_none(), "NL CO should be None");

        // Check last_updated timestamp (should be the timestamp of the most recent measurement overall for the city)
        let one_day_ago = Utc::now() - Duration::days(1);
        // Allow some tolerance for timestamp comparison due to test execution time variance
        assert!(
            (nl_city_data.last_updated - one_day_ago)
                .num_seconds()
                .abs()
                < 15,
            "Last updated timestamp mismatch"
        );

        // Test for a country with no city data (e.g., if test data only had country-level info)
        // let results_no_city = db.get_latest_measurements_by_city("COUNTRY_WITHOUT_CITY").await?;
        // assert!(results_no_city.is_empty());

        Ok(())
    }

    /// Tests the `is_schema_initialized` helper function state changes.
    #[sqlx::test]
    async fn test_is_schema_initialized(pool: PgPool) -> Result<()> {
        let db = Database { pool };
        // Before init
        assert!(
            !db.is_schema_initialized().await?,
            "Schema should not be initialized initially"
        );
        // After init
        db.init_schema().await?;
        assert!(
            db.is_schema_initialized().await?,
            "Schema should be initialized after calling init_schema"
        );
        Ok(())
    }

    /// Tests the `has_data_imported` helper function state changes.
    #[sqlx::test]
    async fn test_has_data_imported(pool: PgPool) -> Result<()> {
        let db = Database { pool };
        // Before init/insert
        assert!(
            !db.has_data_imported().await?,
            "Should have no data before init"
        );
        db.init_schema().await?;
        assert!(
            !db.has_data_imported().await?,
            "Should have no data after init but before insert"
        );
        // After insert
        insert_test_data(&db.pool).await?;
        assert!(
            db.has_data_imported().await?,
            "Should have data after insert"
        );
        Ok(())
    }
}
