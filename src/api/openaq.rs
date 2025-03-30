//! Provides an asynchronous client for interacting with the OpenAQ v3 API.
//!
//! Defines the `OpenAQClient` for fetching air quality measurements.

use crate::error::Result;
#[allow(unused_imports)] // Allow imports used only in signatures
use crate::models::{DailyMeasurement, DailyMeasurementResponse, Location, LocationsResponse};
use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;
use reqwest::Client;
use std::time::Duration as StdDuration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

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

    /// Checks rate limit headers and sleeps if necessary.
    async fn handle_rate_limit(&self, headers: &HeaderMap) {
        let remaining = headers
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok());

        let reset = headers
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        debug!(
            "Rate limit info: Remaining={:?}, Reset={:?}",
            remaining, reset
        );

        // Sleep if remaining requests are very low (e.g., 1 or 0)
        if let Some(rem) = remaining {
            if rem <= 1 {
                if let Some(reset_secs) = reset {
                    let sleep_duration = StdDuration::from_secs(reset_secs + 1); // Add 1s buffer
                    info!(
                        "Rate limit low ({} remaining). Sleeping for {:?} seconds...",
                        rem, sleep_duration
                    );
                    sleep(sleep_duration).await;
                } else {
                    // Fallback sleep if reset header is missing/invalid but remaining is low
                    warn!("Rate limit low ({} remaining) but reset header missing/invalid. Sleeping for 10s as fallback.", rem);
                    sleep(StdDuration::from_secs(10)).await;
                }
            }
        } else {
            // If remaining header is missing, add a small default sleep to be cautious
            debug!("Rate limit 'remaining' header missing, adding small default sleep.");
            sleep(StdDuration::from_millis(200)).await; // e.g., 200ms default sleep
        }

        // Add a small fixed sleep after every request regardless of remaining count
        // to avoid bursting the limit.
        sleep(StdDuration::from_millis(100)).await; // e.g., 100ms sleep after each request
    }

    /// Fetches the top 10 locations (based on OpenAQ default sorting) for given country IDs from the OpenAQ v3 API.
    ///
    /// Fetches only the first page with a limit of 10.
    ///
    /// # Arguments
    ///
    /// * `country_ids` - A slice of country IDs (e.g., [109, 94, 50]).
    ///
    /// # Errors
    ///
    /// Returns `AppError::Api` if the request fails, the API returns an error,
    /// or the response cannot be parsed.
    pub async fn get_locations_for_country(
        &self,
        country_ids: &[u32], // Takes slice of country IDs
    ) -> Result<Vec<crate::models::Location>> {
        // Fetch only the top 10 locations per country ID, no pagination needed.
        info!(
            "Fetching top 10 locations for country IDs: {:?}",
            country_ids
        );
        let limit = 10;
        let page = 1; // Always fetch page 1

        let url = format!("{}/locations", self.base_url);
        debug!("Requesting locations URL: {} (page {})", url, page);

        // Build query parameters
        let mut query_params = Vec::new();
        for &id in country_ids {
            query_params.push(("countries_id", id.to_string()));
        }
        query_params.push(("limit", limit.to_string()));
        query_params.push(("page", page.to_string()));
        query_params.push(("monitor", "true".to_string()));
        query_params.push(("mobile", "false".to_string()));

        let response_result = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .query(&query_params)
            .send()
            .await;

        let response = match response_result {
            Ok(resp) => resp,
            Err(e) => {
                error!("Network request failed for locations: {}", e);
                return Err(e.into());
            },
        };

        // Check status code
        let response = match response.error_for_status() {
            Ok(resp) => resp,
            Err(e) => {
                let status = e.status().unwrap_or_default();
                let error_url = e.url().map(|u| u.as_str()).unwrap_or(&url);
                error!(
                    "API request for locations to {} failed with status {}: {}",
                    error_url, status, e
                );
                return Err(e.into());
            },
        };

        // Clone headers before consuming the body
        let headers = response.headers().clone();

        // Attempt to parse the successful JSON response
        let api_response: crate::models::LocationsResponse = match response.json().await {
            Ok(parsed) => parsed,
            Err(e) => {
                error!("Failed to parse locations JSON response: {}", e);
                return Err(e.into());
            },
        };

        // Handle rate limiting using cloned headers
        self.handle_rate_limit(&headers).await;

        let locations = api_response.results;
        info!(
            "Successfully fetched {} locations for country IDs: {:?}",
            locations.len(),
            country_ids
        );
        Ok(locations) // Return the fetched locations directly
    }

    /// Fetches daily aggregated measurements for a specific sensor within a given date range.
    ///
    /// Handles pagination to retrieve all available daily measurements within the range.
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
    ) -> Result<Vec<crate::models::DailyMeasurement>> {
        info!(
            "Fetching measurements for sensor ID: {} from {} to {}",
            sensor_id, date_from, date_to
        );
        let mut all_measurements = Vec::new();
        let mut page = 1;
        let limit = 100; // Fetch 100 measurements per page (matches example URL)

        loop {
            let url = format!("{}/sensors/{}/measurements/daily", self.base_url, sensor_id);
            debug!("Requesting measurements URL: {} (page {})", url, page);

            let response_result = self
                .client
                .get(&url)
                .header("X-API-Key", &self.api_key)
                .query(&[
                    ("datetime_from", date_from.to_rfc3339()), // Corrected parameter name
                    ("datetime_to", date_to.to_rfc3339()),     // Corrected parameter name
                    ("limit", limit.to_string()),
                    ("page", page.to_string()),
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
                    return Err(e.into()); // Use From impl
                },
            };

            // Check status code
            let response = match response.error_for_status() {
                Ok(resp) => resp,
                Err(e) => {
                    // Reverted: Log the error and return it directly using From impl
                    let status = e.status().unwrap_or_default();
                    let error_url = e.url().map(|u| u.as_str()).unwrap_or(&url);
                    error!(
                        "API request for measurements (sensor {}, page {}) to {} failed with status {}: {}",
                        sensor_id, page, error_url, status, e
                    );
                    return Err(e.into());
                },
            };

            // Clone headers before consuming the body
            let headers = response.headers().clone();

            // Attempt to parse the successful JSON response directly
            let api_response: crate::models::DailyMeasurementResponse = match response.json().await
            {
                Ok(parsed) => parsed,
                Err(e) => {
                    error!(
                        "Failed to parse measurements JSON response (sensor {}, page {}): {}",
                        sensor_id, page, e
                    );
                    // Attempt to read body as text for logging, even on JSON parse error
                    // This might fail if the body was already partially consumed or invalid.
                    // We clone the builder earlier to potentially retry reading body as text, but reqwest doesn't easily support that.
                    // For now, we just return the reqwest error.
                    return Err(e.into()); // Let From<reqwest::Error> handle it
                },
            };

            // Handle rate limiting using cloned headers
            self.handle_rate_limit(&headers).await;

            let found_count = api_response.results.len();
            debug!("Fetched {} measurements on page {}", found_count, page);
            all_measurements.extend(api_response.results);

            // Check if we need to fetch the next page
            // Handle 'found' field which is Option<u32> after custom deserialization
            let total_found = api_response.meta.found.unwrap_or(0) as usize; // Default to 0 if None

            // Stop if the last page wasn't full OR if total_found is reported and we have enough.
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
}
