#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use crate::api::{MockDataProvider, OpenAQClient};
    use crate::error::Result;
    
    #[test]
    fn test_mock_data_provider() -> Result<()> {
        let mock = MockDataProvider::new();
        
        let end = Utc::now();
        let start = end - Duration::days(5);
        
        // Test for Netherlands
        let nl_data = mock.get_measurements_for_country_in_date_range("NL", start, end)?;
        assert!(!nl_data.is_empty(), "Should generate mock data for NL");
        
        // Test for invalid country - should return empty vector
        let invalid_data = mock.get_measurements_for_country_in_date_range("XX", start, end)?;
        assert!(invalid_data.is_empty(), "Should return empty vector for invalid country code");
        
        // Verify country codes in measurements
        for measurement in nl_data {
            assert_eq!(measurement.country, "NL", "Country code should match");
        }
        
        Ok(())
    }
    
    #[tokio::test]
    #[ignore] // Requires API key to run, so ignored by default
    async fn test_openaq_client() -> Result<()> {
        // This test is ignored by default as it requires a valid API key
        // To run: cargo test -- --ignored
        
        let api_key = std::env::var("OPENAQ_KEY").unwrap_or_else(|_| "test_key".to_string());
        let client = OpenAQClient::new(api_key);
        
        let end = Utc::now();
        let start = end - Duration::days(1);
        
        let result = client.get_measurements_for_country_in_date_range("NL", start, end).await;
        
        // We don't assert success since the API might be unavailable
        // Instead, print the result for manual verification
        println!("API test result: {:?}", result);
        
        Ok(())
    }
}