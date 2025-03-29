//! Provides a client for interacting with the OpenAQ v3 API.
//!
//! This module defines the `OpenAQClient` struct and its methods for fetching
//! air quality measurements from the OpenAQ platform.

use crate::error::{AppError, Result};
use crate::models::{Measurement, OpenAQMeasurementResponse};
use chrono::{DateTime, Utc};
use reqwest::Client;
use tracing::{debug, error, info};

const BASE_URL: &str = "https://api.openaq.org/v3";

/// An asynchronous client for fetching data from the OpenAQ API v3.
pub struct OpenAQClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl OpenAQClient {
    /// Creates a new `OpenAQClient` with the provided API key.
    ///
    /// Uses the default OpenAQ v3 base URL.
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: BASE_URL.to_string(),
        }
    }

    /// Creates a new `OpenAQClient` with a custom base URL.
    ///
    /// This is primarily intended for testing purposes (e.g., using a mock server).
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn new_with_base_url(api_key: String, base_url: &str) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: base_url.to_string(),
        }
    }

    /// Fetches air quality measurements for a specific country within a given date range.
    ///
    /// Corresponds to the `/v3/measurements` endpoint of the OpenAQ API.
    pub async fn get_measurements_for_country_in_date_range(
        &self,
        country: &str, // The 2-letter country code (e.g., "NL", "DE").
        date_from: DateTime<Utc>,
        date_to: DateTime<Utc>,
    ) -> Result<Vec<Measurement>> {
        info!(
            "Fetching measurements for country: {} from {} to {}",
            country, date_from, date_to
        );

        let url = format!("{}/measurements", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key) // API key required by v3
            .query(&[
                ("country_id", country.to_string()), // v3 uses country_id
                ("dateFrom", date_from.to_rfc3339()),
                ("dateTo", date_to.to_rfc3339()),
                ("limit", "1000".to_string()), // Fetch up to 1000 records per request
            ])
            .send()
            .await
            .map_err(|e| {
                error!(
                    "Error fetching measurements for {} in date range: {}",
                    country, e
                );
                AppError::Api(e.into())
            })?;

        // Check HTTP status code and handle potential API errors
        let response = match response.error_for_status() {
            Ok(resp) => resp, // Success status, proceed
            Err(e) => {
                // Error status, log details and return specific AppError
                let status = e.status();
                let url = e.url().map(|u| u.as_str()).unwrap_or("unknown URL");
                error!(
                    "API request to {} failed with status {}: {}",
                    url,
                    status.unwrap_or(reqwest::StatusCode::default()),
                    e
                );
                // Provide more specific user feedback based on common errors
                if status == Some(reqwest::StatusCode::NOT_FOUND) {
                    error!("Received 404 Not Found. Check API endpoint/parameters and OPENAQ_KEY validity for v3.");
                } else if status == Some(reqwest::StatusCode::UNAUTHORIZED)
                    || status == Some(reqwest::StatusCode::FORBIDDEN)
                {
                    error!("Received 401/403. Check OPENAQ_KEY validity and permissions.");
                }
                // Return the underlying reqwest error wrapped in AppError::Api
                return Err(AppError::Api(std::sync::Arc::new(e)));
            },
        };

        // Parse the successful JSON response
        let api_response: OpenAQMeasurementResponse = response.json().await.map_err(|e| {
            error!("Error parsing API response JSON: {}", e);
            AppError::Api(e.into())
        })?;

        debug!(
            "Received {} measurements for {} in date range",
            api_response.results.len(),
            country
        );

        Ok(api_response.results)
    }
}
