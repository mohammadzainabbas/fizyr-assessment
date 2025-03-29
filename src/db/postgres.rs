//! Provides database interaction functionalities using `sqlx` with a PostgreSQL backend.
//! Includes schema initialization, data insertion, and various querying capabilities
//! for air quality measurements.

use crate::error::{AppError, Result};
use crate::models::{
    CityLatestMeasurements, CountryAirQuality, DbMeasurement, Measurement, PollutionRanking,
};
use rayon::prelude::*;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres, Row};
use tracing::{debug, error, info};

/// Represents the database connection pool and provides methods for database operations.
pub struct Database {
    pool: Pool<Postgres>,
}

impl Database {
    /// Create a new database connection pool
    pub async fn new(database_url: &str) -> Result<Self> {
        info!("Connecting to database: {}", database_url);

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .map_err(|e| {
                error!("Failed to connect to database: {}", e);
                AppError::Db(e.into())
            })?;

        info!("Connected to database successfully");

        Ok(Self { pool })
    }

    /// Initialize the database schema
    pub async fn init_schema(&self) -> Result<()> {
        info!("Initializing database schema");

        // Create table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS measurements (
                id SERIAL PRIMARY KEY,
                location_id BIGINT NOT NULL,
                location TEXT NOT NULL,
                parameter TEXT NOT NULL,
                value NUMERIC NOT NULL,
                unit TEXT NOT NULL,
                date_utc TIMESTAMPTZ NOT NULL,
                date_local TEXT NOT NULL,
                country TEXT NOT NULL,
                city TEXT,
                latitude DOUBLE PRECISION,
                longitude DOUBLE PRECISION,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("Failed to create measurements table: {}", e);
            AppError::Db(e.into())
        })?;

        // Index for efficient country-based filtering
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_measurements_country ON measurements(country)"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("Failed to create country index: {}", e);
            AppError::Db(e.into())
        })?;

        // Index for efficient parameter-based filtering
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_measurements_parameter ON measurements(parameter)"#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("Failed to create parameter index: {}", e);
            AppError::Db(e.into())
        })?;

        // Index for efficient time-based filtering
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

    /// Insert a batch of measurements into the database
    pub async fn insert_measurements(&self, measurements: &[Measurement]) -> Result<()> {
        if measurements.is_empty() {
            debug!("No measurements to insert");
            return Ok(());
        }

        info!(
            "Inserting {} measurements into database",
            measurements.len()
        );

        // Convert measurements to database format in parallel
        let db_measurements: Vec<DbMeasurement> = measurements
            .par_iter()
            .map(|m| DbMeasurement::from(m.clone()))
            .collect();

        // Use a transaction for atomic and potentially faster batch inserts
        let mut tx = self.pool.begin().await.map_err(|e| {
            error!("Failed to begin transaction: {}", e);
            AppError::Db(e.into())
        })?;

        for m in &db_measurements {
            sqlx::query(
                r#"
                INSERT INTO measurements 
                (location_id, location, parameter, value, unit, date_utc, date_local, country, city, latitude, longitude)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                ON CONFLICT DO NOTHING -- Avoid errors if the exact measurement already exists (though unlikely with timestamps)
                "#,
            )
            .bind(m.location_id)
            .bind(&m.location)
            .bind(&m.parameter)
            .bind(m.value)
            .bind(&m.unit)
            .bind(m.date_utc)
            .bind(&m.date_local)
            .bind(&m.country)
            .bind(&m.city)
            .bind(m.latitude)
            .bind(m.longitude)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                error!("Failed to insert measurement: {}", e);
                AppError::Db(e.into())
            })?;
        }

        tx.commit().await.map_err(|e| {
            error!("Failed to commit transaction: {}", e);
            AppError::Db(e.into())
        })?;

        info!("Successfully inserted {} measurements", measurements.len());

        Ok(())
    }

    /// Get the most polluted country based on the latest data
    pub async fn get_most_polluted_country(&self, countries: &[&str]) -> Result<PollutionRanking> {
        info!("Finding the most polluted country among: {:?}", countries);

        let countries_list = countries.join("','");

        let query = format!(
            r#"
            WITH latest_data AS (
                SELECT 
                    country,
                    parameter,
                    AVG(value::DOUBLE PRECISION) as avg_value
                FROM measurements
                WHERE
                    country IN ('{}')
                    AND parameter IN ('pm25', 'pm10')
                    AND date_utc > NOW() - INTERVAL '7 days'
                GROUP BY country, parameter
            )
            SELECT
                country,
                SUM(CASE WHEN parameter = 'pm25' THEN avg_value * 1.5 ELSE 0 END)::DOUBLE PRECISION +
                SUM(CASE WHEN parameter = 'pm10' THEN avg_value ELSE 0 END)::DOUBLE PRECISION as pollution_index,
                MAX(CASE WHEN parameter = 'pm25' THEN avg_value ELSE NULL END)::DOUBLE PRECISION as pm25_avg,
                MAX(CASE WHEN parameter = 'pm10' THEN avg_value ELSE NULL END)::DOUBLE PRECISION as pm10_avg
            FROM latest_data
            GROUP BY country
            ORDER BY pollution_index DESC
            LIMIT 1
            "#,
            countries_list
        );

        let result = sqlx::query_as::<_, (String, f64, Option<f64>, Option<f64>)>(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to query most polluted country: {}", e);
                AppError::Db(e.into())
            })?;

        match result {
            Some((country, pollution_index, pm25_avg, pm10_avg)) => {
                info!(
                    "Most polluted country: {} with index: {}",
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
                // If no data found, return default with the first country
                let default_country = countries.first().unwrap_or(&"Unknown");

                error!("No pollution data found for the specified countries");

                Ok(PollutionRanking::new(default_country))
            },
        }
    }

    /// Calculate the 5-day average air quality for a specific country
    pub async fn get_average_air_quality(&self, country: &str) -> Result<CountryAirQuality> {
        // Removed days parameter
        info!("Calculating 5-day average air quality for {}", country); // Updated log message

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
            country = $1
            AND date_utc > NOW() - INTERVAL '5 days' -- Hardcoded 5 days
        GROUP BY country
        "#;

        // Removed interval variable

        let result = sqlx::query_as::<
            _,
            (
                String,
                Option<f64>,
                Option<f64>,
                Option<f64>,
                Option<f64>,
                Option<f64>,
                Option<f64>,
                i64,
            ),
        >(query)
        .bind(country)
        // Removed bind(interval)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!("Failed to query average air quality: {}", e);
            AppError::Db(e.into())
        })?;

        match result {
            Some((
                country,
                avg_pm25,
                avg_pm10,
                avg_o3,
                avg_no2,
                avg_so2,
                avg_co,
                measurement_count,
            )) => {
                info!(
                    "Found air quality data for {} with {} measurements",
                    country, measurement_count
                );

                Ok(CountryAirQuality {
                    country,
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
                info!("No air quality data found for {}", country);

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

    /// Get the latest measurement for each parameter, grouped by city, for a specific country.
    ///
    /// This function retrieves the most recent value recorded for each pollutant (pm25, pm10, etc.)
    /// within each city of the specified country. It also includes the timestamp of the
    /// latest update among any of these parameters for that city.
    pub async fn get_latest_measurements_by_city(
        &self,
        country: &str,
    ) -> Result<Vec<CityLatestMeasurements>> {
        info!("Fetching latest measurements by city for {}", country);

        // Use DISTINCT ON to get the latest record for each city/parameter combination
        // Then pivot the data using conditional aggregation
        let query = r#"
        WITH latest_city_param AS (
            SELECT DISTINCT ON (city, parameter)
                city,
                parameter,
                value,
                date_utc
            FROM measurements
            WHERE country = $1 AND city IS NOT NULL
            ORDER BY city, parameter, date_utc DESC
        )
        SELECT
            city,
            MAX(CASE WHEN parameter = 'pm25' THEN value ELSE NULL END) as pm25,
            MAX(CASE WHEN parameter = 'pm10' THEN value ELSE NULL END) as pm10,
            MAX(CASE WHEN parameter = 'o3' THEN value ELSE NULL END) as o3,
            MAX(CASE WHEN parameter = 'no2' THEN value ELSE NULL END) as no2,
            MAX(CASE WHEN parameter = 'so2' THEN value ELSE NULL END) as so2,
            MAX(CASE WHEN parameter = 'co' THEN value ELSE NULL END) as co,
            MAX(date_utc) as last_updated -- Get the latest update time for any parameter in the city
        FROM latest_city_param
        GROUP BY city
        ORDER BY city
        "#;

        let results = sqlx::query_as::<_, CityLatestMeasurements>(query)
            .bind(country)
            .fetch_all(&self.pool)
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

    /// Check if the database schema (measurements table) exists.
    pub async fn is_schema_initialized(&self) -> Result<bool> {
        let query = "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'measurements')";
        let result = sqlx::query(query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Db(e.into()))?;
        Ok(result.try_get::<bool, _>(0).unwrap_or(false))
    }

    /// Check if any data has been imported into the measurements table.
    pub async fn has_data_imported(&self) -> Result<bool> {
        // First check if schema exists, if not, no data is imported
        if !self.is_schema_initialized().await? {
            return Ok(false);
        }
        let query = "SELECT EXISTS (SELECT 1 FROM measurements LIMIT 1)";
        let result = sqlx::query(query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Db(e.into()))?;
        Ok(result.try_get::<bool, _>(0).unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Dates, Measurement};
    use chrono::{Duration, Utc};
    use num_traits::FromPrimitive; // Required for Decimal::from_f64
    use sqlx::{types::Decimal, PgPool, Row};

    /// Helper function to create a Measurement instance for testing.
    fn create_test_measurement(
        country: &str,
        parameter: &str,
        value: f64,
        days_ago: i64,
    ) -> Measurement {
        Measurement {
            location_id: 123, // Use a fixed ID for simplicity
            location: format!("Test Location {}", country),
            parameter: parameter.to_string(),
            value,
            unit: "µg/m³".to_string(),
            date: Dates {
                utc: Utc::now() - Duration::days(days_ago),
                local: format!(
                    "{}", // Format as ISO 8601 string
                    (Utc::now() - Duration::days(days_ago)).format("%Y-%m-%dT%H:%M:%S%z") // Example: 2023-10-27T10:00:00+00:00
                ),
            },
            country: country.to_string(),
            city: Some(format!("Test City {}", country)),
            coordinates: None, // Coordinates not essential for these tests
        }
    }

    /// Helper function to set up the database schema and insert standard test data.
    async fn insert_test_data(pool: &PgPool) -> Result<()> {
        let db = Database { pool: pool.clone() };
        db.init_schema().await?; // Ensure schema exists before inserting

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
            // France data (older)
            create_test_measurement("FR", "pm25", 10.0, 6),
            // Greece data (recent)
            create_test_measurement("GR", "pm10", 22.0, 1),
            // Spain data (recent)
            create_test_measurement("ES", "pm25", 12.0, 1),
        ];
        db.insert_measurements(&measurements).await?;
        Ok(())
    }

    #[sqlx::test]
    #[ignore] // Ignore this test by default, requires a running DB
    async fn test_init_schema(pool: PgPool) -> Result<()> {
        let db = Database { pool };
        let result = db.init_schema().await;
        assert!(result.is_ok());

        // Check if table exists
        let row = sqlx::query(
            "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'measurements')",
        )
        .fetch_one(&db.pool)
        .await?;
        assert!(row.get::<bool, _>(0));

        // Check if indexes were created
        let indexes = [
            "idx_measurements_country",
            "idx_measurements_parameter",
            "idx_measurements_date_utc",
        ];
        for index_name in indexes {
            let row = sqlx::query("SELECT EXISTS (SELECT FROM pg_indexes WHERE indexname = $1)")
                .bind(index_name)
                .fetch_one(&db.pool)
                .await?;
            assert!(row.get::<bool, _>(0), "Index {} should exist", index_name);
        }

        Ok(())
    }

    #[sqlx::test]
    #[ignore] // Ignore this test by default, requires a running DB
    async fn test_insert_measurements(pool: PgPool) -> Result<()> {
        let db = Database { pool };
        db.init_schema().await?;

        let m1 = create_test_measurement("NL", "pm25", 10.0, 1);
        let m2 = create_test_measurement("DE", "pm10", 20.0, 1);
        let measurements = vec![m1, m2];

        let result = db.insert_measurements(&measurements).await;
        assert!(result.is_ok());

        // Verify data was inserted
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM measurements")
            .fetch_one(&db.pool)
            .await?;
        assert_eq!(count, 2);

        let row =
            sqlx::query_as::<_, DbMeasurement>("SELECT * FROM measurements WHERE country = 'NL'")
                .fetch_one(&db.pool)
                .await?;
        assert_eq!(row.country, "NL");
        assert_eq!(row.parameter, "pm25");
        assert_eq!(row.value, Decimal::from_f64(10.0).unwrap());

        Ok(())
    }

    #[sqlx::test]
    #[ignore] // Ignore this test by default, requires a running DB
    async fn test_get_most_polluted_country(pool: PgPool) -> Result<()> {
        insert_test_data(&pool).await?;
        let db = Database { pool };

        let countries = ["NL", "DE", "FR", "GR", "ES", "PK"];
        let result = db.get_most_polluted_country(&countries).await?;

        // Pakistan should be most polluted based on test data (pm25*1.5 + pm10)
        // Expected calculation based on test data (pm25*1.5 + pm10):
        // PK: (50 * 1.5) + 80 = 75 + 80 = 155
        // DE: (18 * 1.5) + 28 = 27 + 28 = 55
        // NL: (15 * 1.5) + 25 = 22.5 + 25 = 47.5
        // ES: (12 * 1.5) + 0 = 18
        // GR: 0 + 22 = 22
        // FR: No recent data included in calculation
        assert_eq!(result.country, "PK");
        assert!((result.pollution_index - 155.0).abs() < 1e-6); // Use tolerance for float comparison
        assert!(result.pm25_avg.is_some());
        assert!((result.pm25_avg.unwrap() - 50.0).abs() < 1e-6);
        assert!(result.pm10_avg.is_some());
        assert!((result.pm10_avg.unwrap() - 80.0).abs() < 1e-6);

        Ok(())
    }

    #[sqlx::test]
    #[ignore] // Ignore this test by default, requires a running DB
    async fn test_get_average_air_quality(pool: PgPool) -> Result<()> {
        insert_test_data(&pool).await?;
        let db = Database { pool };

        // Test for NL (3 measurements within last 5 days)
        // Call with only country code now
        let result_nl = db.get_average_air_quality("NL").await?;
        assert_eq!(result_nl.country, "NL");
        assert_eq!(result_nl.measurement_count, 3);
        assert!(result_nl.avg_pm25.is_some());
        assert!((result_nl.avg_pm25.unwrap() - 15.0).abs() < 1e-6);
        assert!(result_nl.avg_pm10.is_some());
        assert!((result_nl.avg_pm10.unwrap() - 25.0).abs() < 1e-6);
        assert!(result_nl.avg_no2.is_some());
        assert!((result_nl.avg_no2.unwrap() - 30.0).abs() < 1e-6);
        assert!(result_nl.avg_o3.is_none()); // No O3 data was inserted for NL

        // Test for FR (only old data exists, should return 0 measurements within the 5-day window)
        // Call with only country code now
        let result_fr = db.get_average_air_quality("FR").await?;
        assert_eq!(result_fr.country, "FR");
        assert_eq!(result_fr.measurement_count, 0); // No recent measurements
        assert!(result_fr.avg_pm25.is_none());

        // Test for a country with no data at all
        // Call with only country code now
        let result_xx = db.get_average_air_quality("XX").await?;
        assert_eq!(result_xx.country, "XX");
        assert_eq!(result_xx.measurement_count, 0);

        Ok(())
    }

    // TODO: Add test for get_latest_measurements_by_city
}
