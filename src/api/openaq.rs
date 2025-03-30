//! Provides an asynchronous client for interacting with the OpenAQ v3 API.
//!
//! Defines the `OpenAQClient` for fetching air quality measurements.

use crate::error::{AppError, Result};
use crate::models::{Measurement, OpenAQMeasurementResponse};
use chrono::{DateTime, Utc};
use reqwest::Client;
use tracing::{debug, error, info, warn}; // Added warn

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

    /// Fetches air quality measurements for a specific country within a given date range.
    ///
    /// Makes a GET request to the `/v3/measurements` endpoint. Handles potential
    /// network errors, API errors (non-2xx status codes), and JSON parsing errors.
    /// Logs detailed error information.
    ///
    /// # Arguments
    ///
    /// * `country` - The 2-letter country code (ISO 3166-1 alpha-2, e.g., "NL").
    /// * `date_from` - The start timestamp (inclusive) for the query range (UTC).
    /// * `date_to` - The end timestamp (inclusive) for the query range (UTC).
    ///
    /// # Errors
    ///
    /// Returns `AppError::Api` wrapping the underlying `reqwest::Error` if the request fails,
    /// the API returns a non-success status code, or the JSON response cannot be parsed.
    pub async fn get_measurements_for_country_in_date_range(
        &self,
        country: &str,
        date_from: DateTime<Utc>,
        date_to: DateTime<Utc>,
    ) -> Result<Vec<Measurement>> {
        info!(
            "Fetching measurements for country: {} from {} to {}",
            country, date_from, date_to
        );

        let url = format!("{}/measurements", self.base_url);
        debug!("Requesting URL: {}", url);

        // Build and send the request
        let response_result = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key) // v3 requires API key in header
            .query(&[
                ("country_id", country.to_string()), // Parameter name for v3 API
                ("date_from", date_from.to_rfc3339()), // Format dates as ISO 8601 / RFC3339
                ("date_to", date_to.to_rfc3339()),
                ("limit", "1000".to_string()), // Request up to 1000 records per page (adjust if pagination needed)
                                               // Note: Add `page` parameter here if implementing pagination
            ])
            .send()
            .await;

        // Handle potential network/request errors
        let response = match response_result {
            Ok(resp) => resp,
            Err(e) => {
                error!("Network request failed for {}: {}", url, e);
                // Consider adding specific checks, e.g., e.is_timeout(), e.is_connect()
                return Err(AppError::Api(e.into()));
            },
        };

        // Check HTTP status code for API-level errors
        let response = match response.error_for_status() {
            Ok(resp) => resp, // Success status (2xx), proceed
            Err(e) => {
                // Log details of the API error status
                let status = e.status().unwrap_or_default(); // Get status code
                let error_url = e.url().map(|u| u.as_str()).unwrap_or(&url); // Get URL from error or original
                error!(
                    "API request to {} failed with status {}: {}",
                    error_url, status, e
                );

                // Provide more specific warnings based on common HTTP errors
                match status {
                    reqwest::StatusCode::NOT_FOUND => {
                        warn!("API returned 404 Not Found. Check endpoint/parameters.");
                    },
                    reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                        warn!("API returned 401/403. Check OPENAQ_KEY validity and permissions.");
                    },
                    reqwest::StatusCode::TOO_MANY_REQUESTS => {
                        warn!("API returned 429 Too Many Requests. Rate limit likely exceeded.");
                    },
                    _ => {
                        // Handle other non-success statuses
                        warn!("API returned non-success status: {}", status);
                    },
                }
                // Return the underlying reqwest error wrapped in AppError::Api
                return Err(AppError::Api(std::sync::Arc::new(e)));
            },
        };

        // Attempt to parse the successful JSON response
        debug!("Attempting to parse JSON response from {}", url);
        let api_response: OpenAQMeasurementResponse = match response.json().await {
            Ok(parsed) => parsed,
            Err(e) => {
                error!("Failed to parse JSON response from {}: {}", url, e);
                // Consider logging response body here if possible (beware of large responses)
                return Err(AppError::Api(e.into()));
            },
        };

        info!(
            "Successfully fetched and parsed {} measurements for {}",
            api_response.results.len(),
            country
        );

        Ok(api_response.results)
    }
}
