use chrono::{DateTime, Utc};
use num_traits::FromPrimitive; // Correct import path
use serde::{Deserialize, Serialize};
use sqlx::types::Decimal; // Correct import: Decimal

/// Response from OpenAQ API for measurements
#[derive(Debug, Deserialize)]
pub struct OpenAQMeasurementResponse {
    #[serde(rename = "meta")] // Keep original name for deserialization
    pub _meta: Meta, // Prefixed with underscore
    pub results: Vec<Measurement>,
}

/// Metadata from OpenAQ API response
#[derive(Debug, Deserialize)]
pub struct Meta {
    #[serde(rename = "name")]
    pub _name: String, // Prefixed
    #[serde(rename = "license")]
    pub _license: String, // Prefixed
    #[serde(rename = "website")]
    pub _website: String, // Prefixed
    #[serde(rename = "page")]
    pub _page: i32, // Prefixed
    #[serde(rename = "limit")]
    pub _limit: i32, // Prefixed
    #[serde(rename = "found")]
    pub _found: i32, // Prefixed
}

/// Response from OpenAQ API for countries
#[derive(Debug, Deserialize)]
pub struct OpenAQCountryResponse {
    #[serde(rename = "meta")]
    pub _meta: Meta, // Prefixed
    #[serde(rename = "results")]
    pub _results: Vec<Country>, // Prefixed
}

/// Country information from OpenAQ API
#[derive(Debug, Deserialize)]
pub struct Country {
    #[serde(rename = "code")]
    pub _code: String, // Prefixed
    #[serde(rename = "name")]
    pub _name: String, // Prefixed
    #[serde(rename = "locations")]
    pub _locations: i32, // Prefixed
    #[serde(rename = "count")]
    pub _count: i64, // Prefixed
}

/// Measurement coordinate
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Coordinates {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// Date information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dates {
    pub utc: DateTime<Utc>,
    pub local: String,
}

/// Measurement value
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Value {
    pub value: f64,
}

/// Air quality measurement
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Measurement {
    #[serde(rename = "locationId")]
    pub location_id: i64,
    pub location: String,
    pub parameter: String,
    pub value: f64,
    pub date: Dates,
    pub unit: String,
    pub coordinates: Option<Coordinates>,
    pub country: String,
    pub city: Option<String>,
}

/// Database representation of a measurement
#[derive(Debug, Serialize, Clone, sqlx::FromRow)] // Added sqlx::FromRow
pub struct DbMeasurement {
    pub id: Option<i32>,
    pub location_id: i64,
    pub location: String,
    pub parameter: String,
    pub value: Decimal, // Changed to Decimal
    pub unit: String,
    pub date_utc: DateTime<Utc>,
    pub date_local: String,
    pub country: String,
    pub city: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

impl From<Measurement> for DbMeasurement {
    fn from(m: Measurement) -> Self {
        Self {
            id: None,
            location_id: m.location_id,
            location: m.location,
            parameter: m.parameter,
            value: Decimal::from_f64(m.value).unwrap_or_default(), // Convert f64 to Decimal
            unit: m.unit,
            date_utc: m.date.utc,
            date_local: m.date.local,
            country: m.country,
            city: m.city,
            latitude: m.coordinates.as_ref().and_then(|c| c.latitude),
            longitude: m.coordinates.as_ref().and_then(|c| c.longitude),
        }
    }
}

/// Country air quality summary
#[derive(Debug, Serialize, Clone)] // Added Clone
pub struct CountryAirQuality {
    pub country: String,
    pub avg_pm25: Option<f64>,
    pub avg_pm10: Option<f64>,
    pub avg_o3: Option<f64>,
    pub avg_no2: Option<f64>,
    pub avg_so2: Option<f64>,
    pub avg_co: Option<f64>,
    pub measurement_count: i64,
}

/// Pollution ranking by country
#[derive(Debug, Serialize, Clone)] // Added Clone
pub struct PollutionRanking {
    pub country: String,
    pub pollution_index: f64,
    pub pm25_avg: Option<f64>,
    pub pm10_avg: Option<f64>,
}

impl PollutionRanking {
    pub fn new(country: &str) -> Self {
        Self {
            country: country.to_string(),
            pollution_index: 0.0,
            pm25_avg: None,
            pm10_avg: None,
        }
    }
}
