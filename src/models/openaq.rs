//! Defines data structures for the application.
//!
//! Includes structs for:
//! - Deserializing OpenAQ API v3 responses.
//! - Representing data stored in the database (`DbMeasurement`).
//! - Structuring results for CLI output (`CityLatestMeasurements`, `CountryAirQuality`, `PollutionRanking`).

use chrono::{DateTime, Utc};
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};
use sqlx::types::Decimal;
use tracing::warn; // Use warn for potential conversion issues

// --- V3 API Response Structs ---

/// Generic Metadata for V3 API responses.
#[allow(dead_code)] // Fields might not all be used currently
#[derive(Debug, Deserialize, Clone)]
pub struct MetaV3 {
    pub name: String,
    pub website: String,
    pub page: u32,
    pub limit: u32,
    pub found: Option<u32>, // 'found' might not always be present
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
pub struct LocationsResponse {
    pub meta: MetaV3,
    pub results: Vec<Location>,
}

/// Represents a single location from the `/v3/locations` endpoint.
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)] // Fields might not all be used currently
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

/// Response structure for the `/v3/locations/{id}/latest` endpoint.
#[allow(dead_code)] // Fields might not all be used currently
#[derive(Debug, Deserialize, Clone)]
pub struct LatestResponse {
    pub meta: MetaV3,
    pub results: Vec<Latest>,
}

/// Represents a single latest measurement value for a sensor at a location.
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)] // Fields might not all be used currently
#[serde(rename_all = "camelCase")]
pub struct Latest {
    pub datetime: DatetimeObject,
    pub value: f64,
    pub coordinates: Coordinates, // Coordinates of the specific sensor/measurement
    pub sensors_id: i32,          // Link back to the sensor
    pub locations_id: i32,        // Link back to the location
                                  // Note: Parameter info is implicitly tied via the sensor_id,
                                  // you might need to fetch sensor details separately if needed here.
}

/// Response structure for the `/v3/sensors/{id}/measurements` endpoint.
#[derive(Debug, Deserialize, Clone)]
pub struct MeasurementsResponse {
    pub meta: MetaV3,
    pub results: Vec<MeasurementV3>,
}

/// Represents a single measurement from the `/v3/sensors/{id}/measurements` endpoint.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MeasurementV3 {
    pub value: f64,
    // pub flag_info: FlagInfo, // Simplified for now
    pub parameter: ParameterBase, // Parameter details included here
    pub period: Option<Period>,   // Included for aggregated results
    pub coordinates: Option<Coordinates>, // Coordinates for this specific measurement/sensor
                                  // pub summary: Option<Summary>, // Simplified for now
                                  // pub coverage: Option<Coverage>, // Simplified for now
                                  // Need to add datetime information - assuming it's part of 'period' or needs separate fetch context
                                  // Let's assume we need the timestamp from the request context or fetch separately for now.
                                  // We will need location_id and country_code from the context of the request as well.
}

/// Represents the time period for aggregated measurements.
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)] // Fields might not all be used currently
#[serde(rename_all = "camelCase")]
pub struct Period {
    pub label: String,
    pub interval: String,
    pub datetime_from: Option<DatetimeObject>,
    pub datetime_to: Option<DatetimeObject>,
}

// --- Database and Output Structs (Potentially need adjustments later) ---

/// Represents a measurement structured for storage in the PostgreSQL database.
/// Derives `sqlx::FromRow` for easy mapping from query results.
#[derive(Debug, Serialize, Clone, sqlx::FromRow)]
pub struct DbMeasurement {
    /// Primary key (auto-generated by the database, None before insertion).
    pub id: Option<i32>,
    pub location_id: i64, // Changed from i32 to i64 to match potential API types if needed, check DB schema
    pub sensor_id: Option<i64>, // Added sensor ID
    pub location_name: String, // Changed from 'location'
    pub parameter_id: i32, // Added parameter ID
    pub parameter_name: String, // Changed from 'parameter'
    /// Measurement value stored as `Decimal` for precision.
    pub value: Decimal,
    pub unit: String,
    pub date_utc: DateTime<Utc>,
    pub date_local: String,
    pub country: String, // Country code
    pub city: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub is_mobile: bool,       // Added field
    pub is_monitor: bool,      // Added field
    pub owner_name: String,    // Added field
    pub provider_name: String, // Added field
}

impl DbMeasurement {
    /// Creates a `DbMeasurement` from an API `MeasurementV3` and its associated `Location` context.
    ///
    /// Handles potential conversion issues (e.g., f64 to Decimal) and extracts relevant fields.
    /// Requires the sensor ID and measurement timestamp to be passed explicitly, as they are
    /// not directly part of the `MeasurementV3` struct itself in all API responses.
    pub fn from_v3_measurement(
        m: &MeasurementV3,
        location: &Location,
        sensor_id: i32,
        measurement_time: DateTime<Utc>, // Explicitly pass the measurement time
    ) -> Self {
        // Determine the best available local timestamp string
        let date_local_str = m
            .period
            .as_ref()
            .and_then(|p| p.datetime_from.as_ref()) // Prefer datetime_from if available (e.g., hourly aggregates)
            .map(|dt| dt.local.clone())
            .unwrap_or_else(|| measurement_time.to_rfc3339()); // Fallback to UTC RFC3339 string

        // Determine best coordinates, preferring measurement-specific, fallback to location
        let latitude = m
            .coordinates
            .as_ref()
            .and_then(|c| c.latitude)
            .or(location.coordinates.latitude);
        let longitude = m
            .coordinates
            .as_ref()
            .and_then(|c| c.longitude)
            .or(location.coordinates.longitude);

        Self {
            id: None, // ID is generated by the database
            location_id: location.id as i64, // Cast to i64 if DB expects bigint
            sensor_id: Some(sensor_id as i64), // Cast to i64 if DB expects bigint
            location_name: location.name.clone().unwrap_or_else(|| format!("Location {}", location.id)), // Use ID if name is null
            parameter_id: m.parameter.id,
            parameter_name: m.parameter.name.clone(),
            value: Decimal::from_f64(m.value).unwrap_or_else(|| {
                warn!(
                    "Could not convert f64 {} to Decimal precisely for parameter {} (sensor {}, location {}). Storing as 0.",
                    m.value, m.parameter.name, sensor_id, location.id
                );
                Decimal::ZERO // Use Decimal::ZERO for safety
            }),
            unit: m.parameter.units.clone(),
            date_utc: measurement_time,
            date_local: date_local_str,
            country: location.country.code.clone(),
            city: location.locality.clone(), // Use locality as city name
            latitude,
            longitude,
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
    /// The name of the city.
    pub city: String,
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
