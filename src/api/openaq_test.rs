#[cfg(test)]
mod tests {
    use crate::api::OpenAQClient;
    use crate::error::Result;
    use crate::models::{
        Coordinates, CountryBase, DatetimeObject, EntityBase, InstrumentBase, Latest,
        LatestResponse, Location, LocationsResponse, MeasurementV3, MeasurementsResponse, MetaV3,
        ParameterBase, Period, ProviderBase, SensorBase,
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

    // Helper to create a dummy MeasurementV3 for testing
    fn create_test_measurement_v3(
        param_id: i32,
        param_name: &str,
        value: f64,
        timestamp: DateTime<Utc>,
    ) -> MeasurementV3 {
        MeasurementV3 {
            value,
            parameter: ParameterBase {
                id: param_id,
                name: param_name.to_string(),
                units: "µg/m³".to_string(),
                display_name: Some(param_name.to_uppercase()),
            },
            period: Some(Period {
                label: "hour".to_string(),
                interval: "PT1H".to_string(),
                datetime_from: Some(DatetimeObject {
                    utc: timestamp,
                    local: timestamp.to_rfc3339(),
                }),
                datetime_to: Some(DatetimeObject {
                    utc: timestamp + Duration::hours(1),
                    local: (timestamp + Duration::hours(1)).to_rfc3339(),
                }),
            }),
            coordinates: Some(Coordinates {
                latitude: Some(50.2),
                longitude: Some(5.2),
            }),
        }
    }

    #[tokio::test]
    async fn test_get_locations_for_country_success() -> Result<()> {
        let server_url = mockito::server_url();
        let api_key = "test_key".to_string();
        let client = OpenAQClient::new_with_base_url(api_key.clone(), &server_url);
        let country_code = "NL";

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
                Matcher::UrlEncoded("iso".into(), country_code.into()),
                Matcher::UrlEncoded("limit".into(), "1000".into()), // Assuming default limit in function
                Matcher::UrlEncoded("page".into(), "1".into()),
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
                Matcher::UrlEncoded("iso".into(), country_code.into()),
                Matcher::UrlEncoded("limit".into(), "1000".into()),
                Matcher::UrlEncoded("page".into(), "2".into()), // Page 2
            ]))
            .match_header("X-API-Key", &api_key)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&response_body_page2).unwrap())
            .create();

        // Call the function
        let locations = client.get_locations_for_country(country_code).await?;

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

        // Mock an API error response (e.g., 401 Unauthorized)
        let _m = mock("GET", "/v3/locations")
            .match_query(Matcher::UrlEncoded("iso".into(), country_code.into()))
            .match_header("X-API-Key", &api_key)
            .with_status(401) // Simulate Unauthorized
            .with_body(r#"{"detail":"Invalid API key"}"#)
            .create();

        // Call the function
        let result = client.get_locations_for_country(country_code).await;

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
        let start_date = Utc::now() - Duration::days(1);
        let end_date = Utc::now();

        // Mock the API response (single page for simplicity)
        let mock_measurements = vec![
            create_test_measurement_v3(1, "pm25", 20.1, start_date + Duration::hours(1)),
            create_test_measurement_v3(1, "pm25", 22.3, start_date + Duration::hours(2)),
        ];
        let response_body = MeasurementsResponse {
            meta: default_meta(Some(2)),
            results: mock_measurements,
        };
        let _m = mock("GET", &*format!("/v3/sensors/{}/measurements", sensor_id))
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("date_from".into(), start_date.to_rfc3339().into()),
                Matcher::UrlEncoded("date_to".into(), end_date.to_rfc3339().into()),
                Matcher::UrlEncoded("limit".into(), "10000".into()), // Assuming limit in function
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

        // Assertions
        assert_eq!(measurements.len(), 2);
        assert_eq!(measurements[0].parameter.name, "pm25");
        assert!((measurements[0].value - 20.1).abs() < 1e-6);
        assert_eq!(measurements[1].parameter.name, "pm25");
        assert!((measurements[1].value - 22.3).abs() < 1e-6);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_measurements_for_sensor_pagination() -> Result<()> {
        let server_url = mockito::server_url();
        let api_key = "test_key_measure_page".to_string();
        let client = OpenAQClient::new_with_base_url(api_key.clone(), &server_url);
        let sensor_id = 789;
        let start_date = Utc::now() - Duration::days(2);
        let end_date = Utc::now();
        let limit = 1; // Use limit 1 to force pagination

        // Mock page 1
        let mock_m_p1 = vec![create_test_measurement_v3(
            2,
            "pm10",
            30.0,
            start_date + Duration::hours(1),
        )];
        let resp_p1 = MeasurementsResponse {
            meta: default_meta(Some(2)),
            results: mock_m_p1,
        }; // Found 2 total
        let _m1 = mock("GET", &*format!("/v3/sensors/{}/measurements", sensor_id))
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("date_from".into(), start_date.to_rfc3339().into()),
                Matcher::UrlEncoded("date_to".into(), end_date.to_rfc3339().into()),
                Matcher::UrlEncoded("limit".into(), limit.to_string().into()), // Use custom limit
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .match_header("X-API-Key", &api_key)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&resp_p1).unwrap())
            .create();

        // Mock page 2
        let mock_m_p2 = vec![create_test_measurement_v3(
            2,
            "pm10",
            35.5,
            start_date + Duration::hours(2),
        )];
        let resp_p2 = MeasurementsResponse {
            meta: default_meta(Some(2)),
            results: mock_m_p2,
        };
        let _m2 = mock("GET", &*format!("/v3/sensors/{}/measurements", sensor_id))
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("date_from".into(), start_date.to_rfc3339().into()),
                Matcher::UrlEncoded("date_to".into(), end_date.to_rfc3339().into()),
                Matcher::UrlEncoded("limit".into(), limit.to_string().into()),
                Matcher::UrlEncoded("page".into(), "2".into()), // Page 2
            ]))
            .match_header("X-API-Key", &api_key)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&resp_p2).unwrap())
            .create();

        // Temporarily modify the client's function to use the smaller limit for testing pagination
        // This is a bit hacky; ideally, the limit would be configurable or passed in.
        // For this test, we assume the function uses a limit internally that we match in the mock.
        // Let's assume the function uses 10000, we need to adjust the mock setup instead.

        // Re-setup mocks assuming the function uses limit=10000
        let limit_in_func = 10000;
        let _m1_retry = mock("GET", &*format!("/v3/sensors/{}/measurements", sensor_id))
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("date_from".into(), start_date.to_rfc3339().into()),
                Matcher::UrlEncoded("date_to".into(), end_date.to_rfc3339().into()),
                Matcher::UrlEncoded("limit".into(), limit_in_func.to_string().into()), // Match function's limit
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .match_header("X-API-Key", &api_key)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&resp_p1).unwrap()) // Return only 1 result
            .create();

        let _m2_retry = mock("GET", &*format!("/v3/sensors/{}/measurements", sensor_id))
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("date_from".into(), start_date.to_rfc3339().into()),
                Matcher::UrlEncoded("date_to".into(), end_date.to_rfc3339().into()),
                Matcher::UrlEncoded("limit".into(), limit_in_func.to_string().into()),
                Matcher::UrlEncoded("page".into(), "2".into()), // Page 2
            ]))
            .match_header("X-API-Key", &api_key)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&resp_p2).unwrap()) // Return the second result
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
        assert_eq!(measurements[1].parameter.name, "pm10");
        assert!((measurements[1].value - 35.5).abs() < 1e-6);

        Ok(())
    }
}
