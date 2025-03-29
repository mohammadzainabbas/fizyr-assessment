use crate::error::{AppError, Result};
use crate::models::{Measurement, OpenAQMeasurementResponse};
use chrono::{DateTime, Utc};
use reqwest::Client;
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

    /// Get all measurements for a specific country
    pub async fn get_measurements_for_country(&self, country: &str) -> Result<Vec<Measurement>> {
        info!("Fetching measurements for country: {}", country);
        
        let url = format!("{}/measurements", self.base_url);
        
        let response = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .query(&[("country", country), ("limit", "1000")])
            .send()
            .await
            .map_err(|e| {
                error!("Error fetching measurements for {}: {}", country, e);
                AppError::ApiError(e)
            })?;
            
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("API returned non-success status: {} with body: {}", status, body);
            return Err(AppError::ConfigError(format!("API error: {}", status)));
        }
        
        let api_response: OpenAQMeasurementResponse = response.json().await.map_err(|e| {
            error!("Error parsing API response: {}", e);
            AppError::ApiError(e)
        })?;
        
        debug!("Received {} measurements for {}", api_response.results.len(), country);
        
        Ok(api_response.results)
    }

    /// Get measurements for a list of countries with parallel processing
    pub async fn get_measurements_for_countries(&self, countries: &[&str]) -> Result<Vec<Measurement>> {
        info!("Fetching measurements for {} countries", countries.len());
        
        let mut all_measurements = Vec::new();
        
        for country in countries {
            match self.get_measurements_for_country(country).await {
                Ok(measurements) => {
                    all_measurements.extend(measurements);
                }
                Err(e) => {
                    error!("Error fetching measurements for {}: {:?}", country, e);
                    // Continue with other countries even if one fails
                }
            }
        }
        
        info!("Total measurements collected: {}", all_measurements.len());
        Ok(all_measurements)
    }

    /// Get latest measurements for a specific country
    pub async fn get_latest_measurements_for_country(&self, country: &str) -> Result<Vec<Measurement>> {
        info!("Fetching latest measurements for country: {}", country);
        
        let url = format!("{}/latest", self.base_url);
        
        let response = self
            .client
            .get(&url)
            .header("X-API-Key", &self.api_key)
            .query(&[("country", country), ("limit", "1000")])
            .send()
            .await
            .map_err(|e| {
                error!("Error fetching latest measurements for {}: {}", country, e);
                AppError::ApiError(e)
            })?;
            
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("API returned non-success status: {} with body: {}", status, body);
            return Err(AppError::ConfigError(format!("API error: {}", status)));
        }
        
        let api_response: OpenAQMeasurementResponse = response.json().await.map_err(|e| {
            error!("Error parsing API response: {}", e);
            AppError::ApiError(e)
        })?;
        
        debug!("Received {} latest measurements for {}", api_response.results.len(), country);
        
        Ok(api_response.results)
    }

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
                error!("Error fetching measurements for {} in date range: {}", country, e);
                AppError::ApiError(e)
            })?;
            
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("API returned non-success status: {} with body: {}", status, body);
            return Err(AppError::ConfigError(format!("API error: {}", status)));
        }
        
        let api_response: OpenAQMeasurementResponse = response.json().await.map_err(|e| {
            error!("Error parsing API response: {}", e);
            AppError::ApiError(e)
        })?;
        
        debug!(
            "Received {} measurements for {} in date range",
            api_response.results.len(),
            country
        );
        
        Ok(api_response.results)
    }
}