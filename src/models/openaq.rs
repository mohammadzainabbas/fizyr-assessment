//! Defines data structures for the application.
//!
//! Includes structs for:
//! - Deserializing OpenAQ API v3 responses (Locations, Daily Measurements).
//! - Representing data stored in the database (`DbMeasurement`).
//! - Structuring results for CLI output (`CityLatestMeasurements`, `CountryAirQuality`, `PollutionRanking`).

use chrono::{DateTime, Utc};
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Decimal;
use tracing::warn;

// --- V3 API Response Structs ---

// Helper function to deserialize 'found' which can be u32 or ">10" string
fn deserialize_found<'de, D>(deserializer: D) -> std::result::Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let value: Option<Value> = Option::deserialize(deserializer)?;

    match value {
        Some(Value::Number(num)) => num
            .as_u64()
            .map(|v| v as u32)
            .ok_or_else(|| D::Error::custom("Invalid number for 'found' field"))
            .map(Some),
        Some(Value::String(s)) if s.starts_with('>') => Ok(None), // Treat ">10" etc. as unknown count (None)
        Some(other) => Err(D::Error::custom(format!(
            "Unexpected type for 'found' field: {:?}",
            other
        ))),
        None => Ok(None), // Handle null case
    }
}

/// Generic Metadata for V3 API responses.
#[allow(dead_code)] // Fields might not all be used currently
#[derive(Debug, Deserialize, Clone)]
pub struct MetaV3 {
    pub name: String,
    pub website: String,
    pub page: u32,
    pub limit: u32,
    #[serde(deserialize_with = "deserialize_found")]
    pub found: Option<u32>, // Now correctly deserialized as Option<u32> or None
}

/// Represents geographical coordinates (reusable).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Coordinates {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// Represents date and time with UTC and local variants (from V3 schema).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatetimeObject {
    pub utc: DateTime<Utc>,
    pub local: String, // Keep as string as timezone info might vary
}

/// Base representation of a parameter (from V3 schema).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")] // Match JSON field names
pub struct ParameterBase {
    pub id: i32,
    pub name: String,
    pub units: String,
    pub display_name: Option<String>,
}

/// Base representation of a country (from V3 schema).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountryBase {
    pub id: Option<i32>, // ID might not always be present depending on context
    pub code: String,
    pub name: String,
}

/// Base representation of a provider (from V3 schema).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderBase {
    pub id: i32,
    pub name: String,
}

/// Base representation of an owner entity (from V3 schema).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityBase {
    pub id: i32,
    pub name: String,
}

/// Base representation of an instrument (from V3 schema).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentBase {
    pub id: i32,
    pub name: String,
}

/// Base representation of a sensor (from V3 schema).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorBase {
    pub id: i32,
    pub name: String,
    pub parameter: ParameterBase,
}

/// Response structure for the `/v3/locations` endpoint.
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)] // Allow unused fields like 'meta'
pub struct LocationsResponse {
    pub meta: MetaV3,
    pub results: Vec<Location>,
}

/// Represents a single location from the `/v3/locations` endpoint.
#[allow(dead_code)] // Fields might not all be used currently
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub id: i32,
    pub name: Option<String>,
    pub locality: Option<String>, // Often the city name
    pub timezone: String,
    pub country: CountryBase,
    pub owner: EntityBase,
    pub provider: ProviderBase,
    pub is_mobile: bool,
    pub is_monitor: bool, // Often indicates reference grade
    pub instruments: Vec<InstrumentBase>,
    pub sensors: Vec<SensorBase>,
    pub coordinates: Coordinates,
    // pub licenses: Option<Vec<LocationLicense>>, // Simplified for now
    pub bounds: Vec<f64>,      // [min_lon, min_lat, max_lon, max_lat]
    pub distance: Option<f64>, // Included when searching by coordinates
    pub datetime_first: Option<DatetimeObject>,
    pub datetime_last: Option<DatetimeObject>,
}

// --- Daily Measurement Structs ---

/// Response structure for the `/v3/sensors/{id}/measurements/daily` endpoint.
#[derive(Debug, Deserialize, Clone)]
pub struct DailyMeasurementResponse {
    pub meta: MetaV3,
    pub results: Vec<DailyMeasurement>,
}

/// Represents a single daily aggregated measurement.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Allow unused fields like 'coordinates'
pub struct DailyMeasurement {
    pub value: f64, // This is the average value for the day
    // pub flag_info: FlagInfo, // Simplified for now
    pub parameter: ParameterBase,
    pub period: Period, // Contains the date range for the aggregation
    pub coordinates: Option<Coordinates>, // Coordinates might be null for aggregated data
    pub summary: Option<Summary>, // Contains min, max, avg, etc.
    pub coverage: Option<Coverage>, // Contains info about data completeness
}

/// Represents the time period for aggregated measurements.
#[allow(dead_code)] // Fields might not all be used currently
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Period {
    pub label: String,
    pub interval: String,              // e.g., "24:00:00" for daily
    pub datetime_from: DatetimeObject, // Start of the aggregation period
    pub datetime_to: DatetimeObject,   // End of the aggregation period
}

/// Represents summary statistics for an aggregated period.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Allow unused fields like 'q02', 'median', etc.
pub struct Summary {
    pub min: Option<f64>,
    pub q02: Option<f64>,
    pub q25: Option<f64>,
    pub median: Option<f64>,
    pub q75: Option<f64>,
    pub q98: Option<f64>,
    pub max: Option<f64>,
    pub avg: Option<f64>, // Should match the top-level 'value'
    pub sd: Option<f64>,  // Standard deviation
}

/// Represents data coverage information for an aggregated period.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Allow unused fields like 'expected_count', etc.
pub struct Coverage {
    pub expected_count: Option<i32>,
    pub expected_interval: Option<String>,
    pub observed_count: Option<i32>,
    pub observed_interval: Option<String>,
    pub percent_complete: Option<f64>,
    pub percent_coverage: Option<f64>,
    pub datetime_from: Option<DatetimeObject>, // Actual start of observed data
    pub datetime_to: Option<DatetimeObject>,   // Actual end of observed data
}

// --- Database and Output Structs ---

/// Represents a daily aggregated measurement structured for storage in the PostgreSQL database.
#[derive(Debug, Serialize, Clone, sqlx::FromRow)]
pub struct DbMeasurement {
    /// Primary key (auto-generated by the database).
    pub id: Option<i32>,
    pub location_id: i64,
    pub sensor_id: i64,      // Made non-optional assuming we always get it
    pub sensor_name: String, // Added
    pub location_name: String,
    pub parameter_id: i32,
    pub parameter_name: String,
    pub parameter_display_name: Option<String>, // Added
    /// Average value for the day (stored as Decimal, nullable).
    pub value_avg: Option<Decimal>,
    /// Minimum value for the day (stored as Decimal).
    pub value_min: Option<Decimal>,
    /// Maximum value for the day (stored as Decimal).
    pub value_max: Option<Decimal>,
    /// Number of measurements observed during the day.
    pub measurement_count: Option<i32>,
    pub unit: String,
    /// Start date/time (UTC) of the aggregation period (day).
    pub date_utc: DateTime<Utc>,
    /// Start date/time (local) of the aggregation period (day).
    pub date_local: String,
    pub country: String, // Country code
    pub city: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub is_mobile: bool,
    pub is_monitor: bool,
    pub owner_name: String,
    pub provider_name: String,
}

impl DbMeasurement {
    /// Creates a `DbMeasurement` from an API `DailyMeasurement` and its associated `Location` and `SensorBase` context.
    pub fn from_daily_measurement(
        m: &DailyMeasurement,
        location: &Location,
        sensor: &SensorBase,
    ) -> Self {
        // Use summary values if available, otherwise use the top-level average value
        let avg_val = m.summary.as_ref().and_then(|s| s.avg).unwrap_or(m.value);
        let min_val = m.summary.as_ref().and_then(|s| s.min);
        let max_val = m.summary.as_ref().and_then(|s| s.max);
        let measurement_count = m.coverage.as_ref().and_then(|c| c.observed_count);

        // Helper to convert Option<f64> to Option<Decimal>, filtering out negative values
        let to_decimal_opt = |val: Option<f64>| -> Option<Decimal> {
            val.filter(|&v| v >= 0.0) // Filter out negative values first
                .and_then(|v| {
                    Decimal::from_f64(v).or_else(|| {
                        warn!("Could not convert f64 {} to Decimal precisely.", v);
                        None // Return None if conversion fails
                    })
                })
        };

        // Convert avg_val, storing None if negative or conversion fails
        let value_avg_decimal_opt = if avg_val >= 0.0 {
            Decimal::from_f64(avg_val).or_else(|| {
                warn!(
                    "Could not convert average f64 {} to Decimal precisely. Storing as NULL.",
                    avg_val
                );
                None // Store None (NULL) if conversion fails
            })
        } else {
            warn!(
                "Negative avg_val {} encountered for sensor {}. Storing as NULL.",
                avg_val, sensor.id
            );
            None // Store None (NULL) if avg_val is negative
        };

        Self {
            id: None,
            location_id: location.id as i64,
            sensor_id: sensor.id as i64,
            sensor_name: sensor.name.clone(),
            location_name: location
                .name
                .clone()
                .unwrap_or_else(|| format!("Location {}", location.id)),
            parameter_id: m.parameter.id,
            parameter_name: m.parameter.name.clone(),
            parameter_display_name: m.parameter.display_name.clone(),
            value_avg: value_avg_decimal_opt, // Assign the Option<Decimal> directly
            value_min: to_decimal_opt(min_val), // Use helper which now filters negatives
            value_max: to_decimal_opt(max_val), // Use helper which now filters negatives
            measurement_count,
            unit: m.parameter.units.clone(),
            date_utc: m.period.datetime_from.utc, // Use the start of the daily period
            date_local: m.period.datetime_from.local.clone(),
            country: location.country.code.clone(),
            city: location.locality.clone(),
            latitude: location.coordinates.latitude, // Use location coordinates
            longitude: location.coordinates.longitude,
            is_mobile: location.is_mobile,
            is_monitor: location.is_monitor,
            owner_name: location.owner.name.clone(),
            provider_name: location.provider.name.clone(),
        }
    }
}

/// Represents the latest measurement value for each pollutant within a specific city.
/// Used as the result type for the "Get Measurements by City" query. Derives `sqlx::FromRow`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CityLatestMeasurements {
    /// The name of the locality (often a city).
    #[sqlx(rename = "city")] // Map the 'city' column from the query result to this field
    pub locality: String,
    /// Latest PM2.5 value (Decimal for precision).
    pub pm25: Option<Decimal>,
    /// Latest PM10 value (Decimal for precision).
    pub pm10: Option<Decimal>,
    /// Latest O3 value (Decimal for precision).
    pub o3: Option<Decimal>,
    /// Latest NO2 value (Decimal for precision).
    pub no2: Option<Decimal>,
    /// Latest SO2 value (Decimal for precision).
    pub so2: Option<Decimal>,
    /// Latest CO value (Decimal for precision).
    pub co: Option<Decimal>,
    /// Timestamp of the most recent measurement update among any parameter for this city.
    pub last_updated: DateTime<Utc>,
}

/// Represents the calculated average air quality metrics for a country over a 5-day period.
/// Used as the result type for the "Calculate Average Air Quality" query.
#[derive(Debug, Serialize, Clone)]
pub struct CountryAirQuality {
    pub country: String,
    pub avg_pm25: Option<f64>,
    pub avg_pm10: Option<f64>,
    pub avg_o3: Option<f64>,
    pub avg_no2: Option<f64>,
    pub avg_so2: Option<f64>,
    pub avg_co: Option<f64>,
    /// The total number of measurements contributing to the averages within the period.
    pub measurement_count: i64,
}

/// Represents the pollution ranking for a country based on a calculated index.
/// Used as the result type for the "Find Most Polluted Country" query.
#[derive(Debug, Serialize, Clone)]
pub struct PollutionRanking {
    pub country: String,
    /// A calculated index representing overall pollution (higher indicates more pollution).
    /// Currently based on weighted average of recent PM2.5 and PM10.
    pub pollution_index: f64,
    /// The average PM2.5 value (µg/m³) used in the index calculation (if available).
    pub pm25_avg: Option<f64>,
    /// The average PM10 value (µg/m³) used in the index calculation (if available).
    pub pm10_avg: Option<f64>,
}

impl PollutionRanking {
    /// Creates a default `PollutionRanking` instance with a zero index,
    /// typically used when no recent pollution data is found for a country.
    pub fn new(country: &str) -> Self {
        Self {
            country: country.to_string(),
            pollution_index: 0.0, // Default to 0 index when no data
            pm25_avg: None,
            pm10_avg: None,
        }
    }
}
