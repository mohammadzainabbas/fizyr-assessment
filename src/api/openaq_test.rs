#[cfg(test)]
mod tests {
    use crate::api::OpenAQClient;
    use crate::error::Result;
    use crate::models::{
        Coordinates, CountryBase, Coverage, DailyMeasurement, DailyMeasurementResponse,
        DatetimeObject, EntityBase, InstrumentBase, Latest, LatestResponse, Location,
        LocationsResponse, MetaV3, ParameterBase, Period, ProviderBase, SensorBase, Summary,
    };
    use chrono::{Duration, TimeZone, Utc};
    use mockito::{mock, Matcher};
    use serde_json::json; // For creating JSON bodies easily

    // Helper to create a default MetaV3 for mock responses
    fn default_meta(found: Option<u32>) -> MetaV3 {
        MetaV3 {
            name: "openaq-api".to_string(),
            website: "/".to_string(),
            page: 1,
            limit: 100, // Default limit used in mocks
            found,
        }
    }

    // Helper to create a dummy Location for testing
    fn create_test_location(id: i32, country_code: &str) -> Location {
        Location {
            id,
            name: Some(format!("Test Location {}", id)),
            locality: Some(format!("Test City {}", country_code)),
            timezone: "UTC".to_string(),
            country: CountryBase {
                id: Some(1),
                code: country_code.to_string(),
                name: format!("Country {}", country_code),
            },
            owner: EntityBase {
                id: 1,
                name: "Test Owner".to_string(),
            },
            provider: ProviderBase {
                id: 1,
                name: "Test Provider".to_string(),
            },
            is_mobile: false,
            is_monitor: true,
            instruments: vec![InstrumentBase {
                id: 1,
                name: "Test Instrument".to_string(),
            }],
            sensors: vec![SensorBase {
                id: id * 10, // Simple sensor ID based on location ID
                name: format!("Sensor {}", id * 10),
                parameter: ParameterBase {
                    id: 1,
                    name: "pm25".to_string(),
                    units: "µg/m³".to_string(),
                    display_name: Some("PM2.5".to_string()),
                },
            }],
            coordinates: Coordinates {
                latitude: Some(50.0),
                longitude: Some(5.0),
            },
            bounds: vec![4.0, 49.0, 6.0, 51.0],
            distance: None,
            datetime_first: Some(DatetimeObject {
                utc: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
                local: "2024-01-01T00:00:00Z".to_string(),
            }),
            datetime_last: Some(DatetimeObject {
                utc: Utc::now(),
                local: Utc::now().to_rfc3339(),
            }),
        }
    }

    // Helper to create a dummy Latest measurement for testing
    fn create_test_latest(location_id: i32, sensor_id: i32, value: f64) -> Latest {
        Latest {
            datetime: DatetimeObject {
                utc: Utc::now(),
                local: Utc::now().to_rfc3339(),
            },
            value,
            coordinates: Coordinates {
                latitude: Some(50.1),
                longitude: Some(5.1),
            },
            sensors_id: sensor_id,
            locations_id: location_id,
        }
    }

    // Helper to create a dummy DailyMeasurement for testing
    fn create_test_daily_measurement(
        param_id: i32,
        param_name: &str,
        avg_value: f64,
        min_value: Option<f64>,
        max_value: Option<f64>,
        obs_count: Option<i32>,
        timestamp: DateTime<Utc>, // Represents the start of the daily period
    ) -> DailyMeasurement {
        DailyMeasurement {
            value: avg_value, // The top-level value is the average
            parameter: ParameterBase {
                id: param_id,
                name: param_name.to_string(),
                units: "µg/m³".to_string(),
                display_name: Some(param_name.to_uppercase()),
            },
            period: Period {
                label: "1 day".to_string(),
                interval: "24:00:00".to_string(),
                datetime_from: DatetimeObject {
                    utc: timestamp,
                    local: timestamp.to_rfc3339(),
                },
                datetime_to: DatetimeObject {
                    utc: timestamp + Duration::days(1),
                    local: (timestamp + Duration::days(1)).to_rfc3339(),
                },
            },
            coordinates: Some(Coordinates {
                // Can be null, but let's add some dummy data
                latitude: Some(50.3),
                longitude: Some(5.3),
            }),
            summary: Some(Summary {
                min: min_value,
                q02: None, // Not strictly needed for tests
                q25: None,
                median: None,
                q75: None,
                q98: None,
                max: max_value,
                avg: Some(avg_value),
                sd: None,
            }),
            coverage: Some(Coverage {
                expected_count: Some(24), // Assuming hourly base data
                expected_interval: Some("24:00:00".to_string()),
                observed_count: obs_count,
                observed_interval: None, // Not strictly needed
                percent_complete: None,
                percent_coverage: None,
                datetime_from: None, // Not strictly needed
                datetime_to: None,
            }),
        }
    }

    #[tokio::test]
    async fn test_get_locations_for_country_success() -> Result<()> {
        let server_url = mockito::server_url();
        let api_key = "test_key".to_string();
        let client = OpenAQClient::new_with_base_url(api_key.clone(), &server_url);
        let country_code = "NL";
        let country_id = 94; // ID for Netherlands as per TASK.md

        // Mock the API response for page 1
        let mock_locations_page1 = vec![
            create_test_location(1, country_code),
            create_test_location(2, country_code),
        ];
        let response_body_page1 = LocationsResponse {
            meta: default_meta(Some(3)), // Indicate 3 total found
            results: mock_locations_page1,
        };
        let _m1 = mock("GET", "/v3/locations")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("countries_id".into(), country_id.to_string()),
                Matcher::UrlEncoded("limit".into(), "100".into()), // Updated to match new limit
                Matcher::UrlEncoded("page".into(), "1".into()),
                Matcher::UrlEncoded("monitor".into(), "true".into()),
                Matcher::UrlEncoded("mobile".into(), "false".into()),
            ]))
            .match_header("X-API-Key", &api_key)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&response_body_page1).unwrap())
            .create();

        // Mock the API response for page 2
        let mock_locations_page2 = vec![create_test_location(3, country_code)];
        let response_body_page2 = LocationsResponse {
            meta: default_meta(Some(3)),
            results: mock_locations_page2,
        };
        let _m2 = mock("GET", "/v3/locations")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("countries_id".into(), country_id.to_string()),
                Matcher::UrlEncoded("limit".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "2".into()), // Page 2
                Matcher::UrlEncoded("monitor".into(), "true".into()),
                Matcher::UrlEncoded("mobile".into(), "false".into()),
            ]))
            .match_header("X-API-Key", &api_key)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&response_body_page2).unwrap())
            .create();

        // Call the function with country ID
        let locations = client.get_locations_for_country(&[country_id]).await?;

        // Assertions
        assert_eq!(locations.len(), 3, "Should fetch locations from both pages");
        assert_eq!(locations[0].id, 1);
        assert_eq!(locations[1].id, 2);
        assert_eq!(locations[2].id, 3);
        assert_eq!(locations[0].country.code, country_code);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_locations_for_country_api_error() -> Result<()> {
        let server_url = mockito::server_url();
        let api_key = "test_key_error".to_string();
        let client = OpenAQClient::new_with_base_url(api_key.clone(), &server_url);
        let country_code = "DE";
        let country_id = 50; // ID for Germany as per TASK.md

        // Mock an API error response (e.g., 401 Unauthorized)
        let _m = mock("GET", "/v3/locations")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("countries_id".into(), country_id.to_string()),
                Matcher::UrlEncoded("monitor".into(), "true".into()),
                Matcher::UrlEncoded("mobile".into(), "false".into()),
            ]))
            .match_header("X-API-Key", &api_key)
            .with_status(401) // Simulate Unauthorized
            .with_body(r#"{"detail":"Invalid API key"}"#)
            .create();

        // Call the function with country ID
        let result = client.get_locations_for_country(&[country_id]).await;

        // Assertions
        assert!(result.is_err(), "Should return an error on API failure");
        // Optionally check the specific error type or message
        // assert!(matches!(result.err().unwrap(), AppError::Api(_)));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_latest_for_location_success() -> Result<()> {
        let server_url = mockito::server_url();
        let api_key = "test_key_latest".to_string();
        let client = OpenAQClient::new_with_base_url(api_key.clone(), &server_url);
        let location_id = 123;
        let sensor_id = 1230;

        // Mock the API response
        let mock_latest = vec![create_test_latest(location_id, sensor_id, 42.5)];
        let response_body = LatestResponse {
            meta: default_meta(Some(1)),
            results: mock_latest,
        };
        let _m = mock("GET", &*format!("/v3/locations/{}/latest", location_id))
            .match_header("X-API-Key", &api_key)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&response_body).unwrap())
            .create();

        // Call the function
        let latest_data = client.get_latest_for_location(location_id).await?;

        // Assertions
        assert_eq!(latest_data.len(), 1);
        assert_eq!(latest_data[0].locations_id, location_id);
        assert_eq!(latest_data[0].sensors_id, sensor_id);
        assert!((latest_data[0].value - 42.5).abs() < 1e-6);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_measurements_for_sensor_success() -> Result<()> {
        let server_url = mockito::server_url();
        let api_key = "test_key_measure".to_string();
        let client = OpenAQClient::new_with_base_url(api_key.clone(), &server_url);
        let sensor_id = 456;
        let start_date = Utc.with_ymd_and_hms(2024, 3, 10, 0, 0, 0).unwrap(); // Use fixed date for consistency
        let end_date = Utc.with_ymd_and_hms(2024, 3, 12, 0, 0, 0).unwrap();

        // Mock the API response for daily measurements (single page)
        let mock_daily_measurements = vec![
            create_test_daily_measurement(
                1,
                "pm25",
                20.1,
                Some(15.0),
                Some(25.0),
                Some(20),
                start_date,
            ),
            create_test_daily_measurement(
                1,
                "pm25",
                22.3,
                Some(18.0),
                Some(28.0),
                Some(22),
                start_date + Duration::days(1),
            ),
        ];
        let response_body = DailyMeasurementResponse {
            // Use DailyMeasurementResponse
            meta: default_meta(Some(2)),
            results: mock_daily_measurements,
        };
        let _m = mock(
            "GET",
            &*format!("/v3/sensors/{}/measurements/daily", sensor_id),
        ) // Use daily endpoint
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("date_from".into(), start_date.to_rfc3339().into()),
            Matcher::UrlEncoded("date_to".into(), end_date.to_rfc3339().into()),
            Matcher::UrlEncoded("limit".into(), "10000".into()), // Match function's internal limit
            Matcher::UrlEncoded("page".into(), "1".into()),
        ]))
        .match_header("X-API-Key", &api_key)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&response_body).unwrap())
        .create();

        // Call the function
        let measurements = client
            .get_measurements_for_sensor(sensor_id, start_date, end_date)
            .await?;

        // Assertions for DailyMeasurement
        assert_eq!(measurements.len(), 2);
        assert_eq!(measurements[0].parameter.name, "pm25");
        assert!((measurements[0].value - 20.1).abs() < 1e-6); // Check average value
        assert_eq!(measurements[0].period.label, "1 day");
        assert_eq!(measurements[1].parameter.name, "pm25");
        assert!((measurements[1].value - 22.3).abs() < 1e-6); // Check average value
        assert_eq!(
            measurements[1].period.datetime_from.utc,
            start_date + Duration::days(1)
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_get_measurements_for_sensor_pagination() -> Result<()> {
        let server_url = mockito::server_url();
        let api_key = "test_key_measure_page".to_string();
        let client = OpenAQClient::new_with_base_url(api_key.clone(), &server_url);
        let sensor_id = 789;
        let start_date = Utc.with_ymd_and_hms(2024, 3, 10, 0, 0, 0).unwrap(); // Use fixed date
        let end_date = Utc.with_ymd_and_hms(2024, 3, 12, 0, 0, 0).unwrap();
        let limit_in_func = 10000; // The limit used internally by the function

        // Mock page 1 - return only one result, but indicate more exist
        let mock_m_p1 = vec![create_test_daily_measurement(
            2,
            "pm10",
            30.0,
            Some(25.0),
            Some(35.0),
            Some(23),
            start_date,
        )];
        let resp_p1 = DailyMeasurementResponse {
            meta: default_meta(Some(2)), // Indicate 2 total found
            results: mock_m_p1,
        };
        let _m1 = mock(
            "GET",
            &*format!("/v3/sensors/{}/measurements/daily", sensor_id),
        ) // Use daily endpoint
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("date_from".into(), start_date.to_rfc3339().into()),
            Matcher::UrlEncoded("date_to".into(), end_date.to_rfc3339().into()),
            Matcher::UrlEncoded("limit".into(), limit_in_func.to_string().into()), // Match function's limit
            Matcher::UrlEncoded("page".into(), "1".into()),
        ]))
        .match_header("X-API-Key", &api_key)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&resp_p1).unwrap())
        .create();

        // Mock page 2 - return the second result
        let mock_m_p2 = vec![create_test_daily_measurement(
            2,
            "pm10",
            35.5,
            Some(30.0),
            Some(40.0),
            Some(24),
            start_date + Duration::days(1),
        )];
        let resp_p2 = DailyMeasurementResponse {
            meta: default_meta(Some(2)), // Indicate 2 total found
            results: mock_m_p2,
        };
        let _m2 = mock(
            "GET",
            &*format!("/v3/sensors/{}/measurements/daily", sensor_id),
        ) // Use daily endpoint
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("date_from".into(), start_date.to_rfc3339().into()),
            Matcher::UrlEncoded("date_to".into(), end_date.to_rfc3339().into()),
            Matcher::UrlEncoded("limit".into(), limit_in_func.to_string().into()),
            Matcher::UrlEncoded("page".into(), "2".into()), // Page 2
        ]))
        .match_header("X-API-Key", &api_key)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(serde_json::to_string(&resp_p2).unwrap())
        .create();

        // Call the function
        let measurements = client
            .get_measurements_for_sensor(sensor_id, start_date, end_date)
            .await?;

        // Assertions
        assert_eq!(
            measurements.len(),
            2,
            "Should fetch measurements from both pages"
        );
        assert_eq!(measurements[0].parameter.name, "pm10");
        assert!((measurements[0].value - 30.0).abs() < 1e-6);
        assert_eq!(measurements[0].period.datetime_from.utc, start_date);
        assert_eq!(measurements[1].parameter.name, "pm10");
        assert!((measurements[1].value - 35.5).abs() < 1e-6);
        assert_eq!(
            measurements[1].period.datetime_from.utc,
            start_date + Duration::days(1)
        );

        Ok(())
    }
}
