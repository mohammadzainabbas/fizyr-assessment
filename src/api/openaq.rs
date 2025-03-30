//! Provides an asynchronous client for interacting with the OpenAQ v3 API.
//!
//! Defines the `OpenAQClient` for fetching air quality measurements.

use crate::error::{AppError, Result};
// Updated model imports for v3
#[allow(unused_imports)] // Allow imports used only in signatures
use crate::models::{
    Latest, LatestResponse, Location, LocationsResponse, MeasurementV3, MeasurementsResponse,
};
use chrono::{DateTime, Utc};
use reqwest::Client;
use tracing::{debug, error, info}; // Removed unused 'warn'

/// Base URL for the OpenAQ API v3.
const BASE_URL: &str = "https://api.openaq.org/v3";

/// An asynchronous client for fetching air quality data from the OpenAQ API v3.
///
/// Holds a `reqwest::Client` instance for making HTTP requests and the API key.
pub struct OpenAQClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl OpenAQClient {
    /// Creates a new `OpenAQClient` using the provided API key and the default base URL.
    ///
    /// # Arguments
    ///
    /// * `api_key` - The API key obtained from OpenAQ.
    pub fn new(api_key: String) -> Self {
        info!("Creating OpenAQClient");
        Self {
            client: Client::new(), // Create a new reqwest client instance
            api_key,
            base_url: BASE_URL.to_string(),
        }
    }

    /// Creates a new `OpenAQClient` with a custom base URL, primarily for testing.
    #[cfg(test)]
    #[allow(dead_code)] // Allow unused function in non-test builds
    pub fn new_with_base_url(api_key: String, base_url: &str) -> Self {
        info!("Creating OpenAQClient with custom base URL: {}", base_url);
        Self {
            client: Client::new(),
            api_key,
            base_url: base_url.to_string(),
        }
    }

    /// Fetches all locations for a given country code from the OpenAQ v3 API.
    ///
    /// Handles pagination to retrieve all available locations.
    ///
    /// # Arguments
    ///
    /// * `country_code` - The 2-letter ISO 3166-1 alpha-2 country code (e.g., "NL").
    ///
    /// # Errors
    ///
    /// Returns `AppError::Api` if the request fails, the API returns an error,
    /// or the response cannot be parsed.
    pub async fn get_locations_for_country(
        &self,
        country_code: &str,
    ) -> Result<Vec<crate::models::Location>> {
        info!("Fetching locations for country code: {}", country_code);
        let mut all_locations = Vec::new();
        let mut page = 1;
        let limit = 1000; // Fetch 1000 locations per page

        loop {
            let url = format!("{}/locations", self.base_url);
            debug!("Requesting locations URL: {} (page {})", url, page);

            let response_result = self
                .client
                .get(&url)
                .header("X-API-Key", &self.api_key)
                .query(&[
                    ("iso", country_code),
                    ("limit", &limit.to_string()),
                    ("page", &page.to_string()),
                    // Add other relevant filters if needed, e.g., parameter_id
                ])
                .send()
                .await;

            let response = match response_result {
                Ok(resp) => resp,
                Err(e) => {
                    error!(
                        "Network request failed for locations (page {}): {}",
                        page, e
                    );
                    return Err(AppError::Api(e.into()));
                },
            };

            let response = match response.error_for_status() {
                Ok(resp) => resp,
                Err(e) => {
                    let status = e.status().unwrap_or_default();
                    let error_url = e.url().map(|u| u.as_str()).unwrap_or(&url);
                    error!(
                        "API request for locations (page {}) to {} failed with status {}: {}",
                        page, error_url, status, e
                    );
                    // Add specific warnings like before if desired
                    return Err(AppError::Api(std::sync::Arc::new(e)));
                },
            };

            // Read the response body as text first for better error diagnosis if JSON parsing fails
            let response_text = match response.text().await {
                Ok(text) => text,
                Err(e) => {
                    error!(
                        "Failed to read response body for locations (page {}): {}",
                        page, e
                    );
                    return Err(AppError::Api(e.into())); // Network error reading body
                },
            };

            let api_response: crate::models::LocationsResponse =
                match serde_json::from_str(&response_text) {
                    Ok(parsed) => parsed,
                    Err(e) => {
                        error!(
                            "Failed to parse locations JSON response (page {}): {}. Body: {}",
                            page, e, response_text
                        );
                        // Use the new JsonParse variant with .into()
                        return Err(AppError::JsonParse(e.into()));
                    },
                };

            let found_count = api_response.results.len();
            debug!("Fetched {} locations on page {}", found_count, page);
            all_locations.extend(api_response.results);

            // Check if we need to fetch the next page
            let total_found = api_response.meta.found.unwrap_or(0) as usize;
            if all_locations.len() >= total_found || found_count < limit as usize {
                break; // Exit loop if we have all results or the last page was not full
            }

            page += 1;
        }

        info!(
            "Successfully fetched {} total locations for {}",
            all_locations.len(),
            country_code
        );
        Ok(all_locations)
    }

    /// Fetches the latest measurement data for a specific location ID.
    ///
    /// # Arguments
    ///
    /// * `location_id` - The numeric ID of the location.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Api` if the request fails, the API returns an error,
    /// or the response cannot be parsed.
    #[allow(dead_code)] // This function is not currently used by any CLI command
    pub async fn get_latest_for_location(
        &self,
        location_id: i32,
    ) -> Result<Vec<crate::models::Latest>> {
        info!("Fetching latest data for location ID: {}", location_id);
        let url = format!("{}/locations/{}/latest", self.base_url, location_id);
        debug!("Requesting latest URL: {}", url);

        let response_result = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            // No query parameters needed for basic latest endpoint
            .send()
            .await;

        let response = match response_result {
            Ok(resp) => resp,
            Err(e) => {
                error!(
                    "Network request failed for latest data (location {}): {}",
                    location_id, e
                );
                return Err(AppError::Api(e.into()));
            },
        };

        let response = match response.error_for_status() {
            Ok(resp) => resp,
            Err(e) => {
                let status = e.status().unwrap_or_default();
                let error_url = e.url().map(|u| u.as_str()).unwrap_or(&url);
                error!(
                    "API request for latest data (location {}) to {} failed with status {}: {}",
                    location_id, error_url, status, e
                );
                return Err(AppError::Api(std::sync::Arc::new(e)));
            },
        };

        // Read the response body as text first for better error diagnosis if JSON parsing fails
        let response_text = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                error!(
                    "Failed to read response body for latest data (location {}): {}",
                    location_id, e
                );
                return Err(AppError::Api(e.into())); // Network error reading body
            },
        };

        let api_response: crate::models::LatestResponse = match serde_json::from_str(&response_text)
        {
            Ok(parsed) => parsed,
            Err(e) => {
                error!(
                    "Failed to parse latest data JSON response (location {}): {}. Body: {}",
                    location_id, e, response_text
                );
                // Use the new JsonParse variant with .into()
                return Err(AppError::JsonParse(e.into()));
            },
        };

        info!(
            "Successfully fetched {} latest records for location {}",
            api_response.results.len(),
            location_id
        );
        Ok(api_response.results)
    }

    /// Fetches measurements for a specific sensor within a given date range.
    ///
    /// Handles pagination to retrieve all available measurements within the range.
    ///
    /// # Arguments
    ///
    /// * `sensor_id` - The numeric ID of the sensor.
    /// * `date_from` - The start timestamp (inclusive) for the query range (UTC).
    /// * `date_to` - The end timestamp (inclusive) for the query range (UTC).
    ///
    /// # Errors
    ///
    /// Returns `AppError::Api` if the request fails, the API returns an error,
    /// or the response cannot be parsed.
    pub async fn get_measurements_for_sensor(
        &self,
        sensor_id: i32,
        date_from: DateTime<Utc>,
        date_to: DateTime<Utc>,
    ) -> Result<Vec<crate::models::MeasurementV3>> {
        info!(
            "Fetching measurements for sensor ID: {} from {} to {}",
            sensor_id, date_from, date_to
        );
        let mut all_measurements = Vec::new();
        let mut page = 1;
        let limit = 10000; // Fetch 10k measurements per page (adjust as needed)

        loop {
            let url = format!("{}/sensors/{}/measurements", self.base_url, sensor_id);
            debug!("Requesting measurements URL: {} (page {})", url, page);

            let response_result = self
                .client
                .get(&url)
                .header("X-API-Key", &self.api_key)
                .query(&[
                    ("date_from", date_from.to_rfc3339()),
                    ("date_to", date_to.to_rfc3339()),
                    ("limit", limit.to_string()), // Removed &
                    ("page", page.to_string()),   // Removed &
                ])
                .send()
                .await;

            let response = match response_result {
                Ok(resp) => resp,
                Err(e) => {
                    error!(
                        "Network request failed for measurements (sensor {}, page {}): {}",
                        sensor_id, page, e
                    );
                    return Err(AppError::Api(e.into()));
                },
            };

            let response = match response.error_for_status() {
                Ok(resp) => resp,
                Err(e) => {
                    let status = e.status().unwrap_or_default();
                    let error_url = e.url().map(|u| u.as_str()).unwrap_or(&url);
                    error!(
                        "API request for measurements (sensor {}, page {}) to {} failed with status {}: {}",
                        sensor_id, page, error_url, status, e
                    );
                    return Err(AppError::Api(std::sync::Arc::new(e)));
                },
            };

            // Read the response body as text first for better error diagnosis
            let response_text = match response.text().await {
                Ok(text) => text,
                Err(e) => {
                    error!(
                        "Failed to read response body for measurements (sensor {}, page {}): {}",
                        sensor_id, page, e
                    );
                    return Err(AppError::Api(e.into()));
                },
            };

            let api_response: crate::models::MeasurementsResponse = match serde_json::from_str(
                &response_text,
            ) {
                Ok(parsed) => parsed,
                Err(e) => {
                    error!("Failed to parse measurements JSON response (sensor {}, page {}): {}. Body: {}", sensor_id, page, e, response_text);
                    // Use the new JsonParse variant with .into()
                    return Err(AppError::JsonParse(e.into()));
                },
            };

            let found_count = api_response.results.len();
            debug!("Fetched {} measurements on page {}", found_count, page);
            all_measurements.extend(api_response.results);

            // Check if we need to fetch the next page
            let total_found = api_response.meta.found.unwrap_or(0) as usize;
            // Stop if the last page wasn't full OR if total_found is reported and we have enough.
            // The found field might not be reliable for measurements, so primarily rely on found_count < limit.
            if found_count < limit as usize
                || (total_found > 0 && all_measurements.len() >= total_found)
            {
                break;
            }

            page += 1;
        }

        info!(
            "Successfully fetched {} total measurements for sensor {}",
            all_measurements.len(),
            sensor_id
        );
        Ok(all_measurements)
    }

    // TODO: Implement functions to fetch data using v3 location/sensor-based endpoints
    // - get_locations_for_country(country_code: &str) -> Result<Vec<Location>>
    // - get_latest_for_location(location_id: i32) -> Result<Vec<Latest>>
    // - get_measurements_for_sensor(sensor_id: i32, date_from: DateTime<Utc>, date_to: DateTime<Utc>) -> Result<Vec<Measurement>>
    // (Need to update models in src/models/openaq.rs first)
}
