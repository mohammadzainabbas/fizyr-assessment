use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Response from OpenAQ API for measurements
#[derive(Debug, Deserialize)]
pub struct OpenAQMeasurementResponse {
    pub meta: Meta,
    pub results: Vec<Measurement>,
}

/// Metadata from OpenAQ API response
#[derive(Debug, Deserialize)]
pub struct Meta {
    pub name: String,
    pub license: String,
    pub website: String,
    pub page: i32,
    pub limit: i32,
    pub found: i32,
}

/// Response from OpenAQ API for countries
#[derive(Debug, Deserialize)]
pub struct OpenAQCountryResponse {
    pub meta: Meta,
    pub results: Vec<Country>,
}

/// Country information from OpenAQ API
#[derive(Debug, Deserialize)]
pub struct Country {
    pub code: String,
    pub name: String,
    pub locations: i32,
    pub count: i64,
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
#[derive(Debug, Serialize)]
pub struct DbMeasurement {
    pub id: Option<i32>,
    pub location_id: i64,
    pub location: String,
    pub parameter: String,
    pub value: f64,
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
            value: m.value,
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
