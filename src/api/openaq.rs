use crate::error::{AppError, Result};
use crate::models::{Measurement, OpenAQMeasurementResponse};
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode}; // Added StatusCode
                                   // Removed unused Arc import
use tracing::{debug, error, info};

const BASE_URL: &str = "https://api.openaq.org/v3";

/// API client for interacting with the OpenAQ API
pub struct OpenAQClient {
    client: Client,
    api_key: String,
    base_url: String,
}

impl OpenAQClient {
    /// Create a new OpenAQ API client
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: BASE_URL.to_string(),
        }
    }

    /// Create a new OpenAQ API client with a custom base URL (for testing)
    #[cfg(test)]
    pub fn new_with_base_url(api_key: String, base_url: &str) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: base_url.to_string(),
        }
    }

    // Removed unused methods:
    // - get_measurements_for_country
    // - get_measurements_for_countries
    // - get_latest_measurements_for_country

    /// Get measurements for a specific country within a date range
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

        // Try a different endpoint format for API v3
        let url = format!("{}/measurements", self.base_url);

        let response = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .query(&[
                ("country", country.to_string()),
                ("date_from", date_from.to_rfc3339()),
                ("date_to", date_to.to_rfc3339()),
                ("limit", "1000".to_string()),
            ])
            .send()
            .await
            .map_err(|e| {
                error!(
                    "Error fetching measurements for {} in date range: {}",
                    country, e
                );
                AppError::Api(e.into()) // Use renamed variant Api
            })?;

        // Check status and handle error by consuming response with error_for_status()
        let response = match response.error_for_status() {
            Ok(resp) => resp, // Status was success, continue with the response
            Err(e) => {
                // Status was error, error_for_status consumed response and gave us the error
                error!("API returned non-success status: {}", e);
                // Log the body if possible (error might not have body readily available here)
                // let body = e. // Cannot easily get body from error directly in this context
                return Err(AppError::Api(std::sync::Arc::new(e))); // Wrap the error
            },
        };

        // If status was success, parse JSON (consumes response)
        let api_response: OpenAQMeasurementResponse = response.json().await.map_err(|e| {
            error!("Error parsing API response: {}", e);
            AppError::Api(e.into()) // Use renamed variant Api
        })?;

        debug!(
            "Received {} measurements for {} in date range",
            api_response.results.len(),
            country
        );

        Ok(api_response.results)
    }
}
