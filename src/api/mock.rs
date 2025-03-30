//! Provides a mock data provider for generating plausible air quality measurements.
//!
//! This is used as a fallback mechanism when the real OpenAQ API fails or for testing purposes
//! where consistent, controllable data is needed without hitting the actual API.

use crate::error::Result;
use crate::models::{Coordinates, Dates, Measurement};
use chrono::{DateTime, Duration, Utc};
use rand::distributions::{Distribution, Uniform};
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use tracing::debug; // Added debug logging

/// Generates mock air quality measurement data.
///
/// Simulates data similar to what might be received from the OpenAQ API,
/// using predefined locations and randomized values based on typical ranges
/// and country-specific pollution factors.
pub struct MockDataProvider {
    /// List of supported country codes for mock data generation.
    countries: Vec<String>,
}

impl MockDataProvider {
    /// Creates a new `MockDataProvider`.
    pub fn new() -> Self {
        debug!("Creating MockDataProvider");
        Self {
            // Initialize with the same list of countries used elsewhere in the app
            countries: vec![
                "NL".to_string(), // Netherlands
                "DE".to_string(), // Germany
                "FR".to_string(), // France
                "GR".to_string(), // Greece
                "ES".to_string(), // Spain
                "PK".to_string(), // Pakistan
            ],
        }
    }

    /// Generates a vector of mock `Measurement` data for a specific country and date range.
    ///
    /// If the country code is not supported, returns an empty vector. Otherwise, generates
    /// a random number of measurements within the date range, assigning random parameters,
    /// locations (from a predefined list for the country), and values adjusted by a
    /// country-specific pollution factor.
    ///
    /// # Arguments
    ///
    /// * `country` - The 2-letter country code.
    /// * `date_from` - The start timestamp for the mock data range.
    /// * `date_to` - The end timestamp for the mock data range.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<Measurement>` with the generated mock data,
    /// or an empty Vec if the country is unsupported. Errors are unlikely here
    /// but the `Result` signature matches the API client trait.
    pub fn get_measurements_for_country_in_date_range(
        &self,
        country: &str,
        date_from: DateTime<Utc>,
        date_to: DateTime<Utc>,
    ) -> Result<Vec<Measurement>> {
        // Return empty vec if the requested country isn't in our mock list
        if !self.countries.contains(&country.to_string()) {
            debug!(
                "Mock data requested for unsupported country: {}. Returning empty.",
                country
            );
            return Ok(Vec::new());
        }
        debug!(
            "Generating mock data for country: {} from {} to {}",
            country, date_from, date_to
        );

        let mut rng = thread_rng();
        let param_dist = Uniform::new(0u8, 6u8); // Distribution for selecting parameter index (0-5)

        // Determine a random number of measurements to generate for this period
        let days_diff = (date_to - date_from).num_days().max(1); // Ensure at least 1 day diff
        let num_measurements = rng.gen_range(50..=(days_diff * 50).max(100)); // Scale roughly with duration
        debug!("Generating {} mock measurements.", num_measurements);

        let mut measurements = Vec::with_capacity(num_measurements as usize);
        let locations = self.get_mock_locations_for_country(country); // Get predefined locations

        if locations.is_empty() {
            debug!("No mock locations defined for country: {}", country);
            return Ok(Vec::new()); // Should not happen if country check passed, but defensive
        }

        // Generate each mock measurement
        for i in 0..num_measurements {
            // Generate a random timestamp within the requested date range
            let total_seconds = (date_to - date_from).num_seconds();
            let random_offset_seconds = rng.gen_range(0..=total_seconds.max(0));
            let measurement_date = date_from + Duration::seconds(random_offset_seconds);

            // Select a random location for this country
            let location_idx = rng.gen_range(0..locations.len());
            let (location_name, base_coords) = &locations[location_idx];

            // Add slight random variation to coordinates for realism
            let lat_variation = rng.gen_range(-0.05..0.05);
            let lon_variation = rng.gen_range(-0.05..0.05);
            let current_coords = Coordinates {
                latitude: Some(base_coords.0 + lat_variation),
                longitude: Some(base_coords.1 + lon_variation),
            };

            // Select a random parameter
            let parameter = match param_dist.sample(&mut rng) {
                0 => "pm25",
                1 => "pm10",
                2 => "o3",
                3 => "no2",
                4 => "so2",
                _ => "co", // 5 and above map to 'co'
            };

            // Generate a plausible value based on parameter and country pollution factor
            let value = self.generate_mock_value(country, parameter, &mut rng);

            // Create the mock Measurement struct
            let measurement = Measurement {
                // Generate a somewhat unique location ID based on index and iteration
                location_id: (location_idx as i64) * 1000 + (i % 1000),
                location: location_name.to_string(),
                parameter: parameter.to_string(),
                value,
                date: Dates {
                    utc: measurement_date,
                    local: measurement_date.to_rfc3339(), // Use standard format
                },
                unit: self.get_unit_for_parameter(parameter),
                coordinates: Some(current_coords),
                country: country.to_string(),
                city: Some(location_name.clone()), // Assume location name is the city
            };

            measurements.push(measurement);
        }

        // Sort by date for consistency (API might not guarantee order)
        measurements.sort_by(|a, b| a.date.utc.cmp(&b.date.utc));
        debug!(
            "Finished generating {} mock measurements.",
            measurements.len()
        );
        Ok(measurements)
    }

    /// Returns a predefined list of mock location names and coordinates for a given country.
    fn get_mock_locations_for_country(&self, country: &str) -> Vec<(String, (f64, f64))> {
        match country {
            "NL" => vec![
                ("Amsterdam".to_string(), (52.3676, 4.9041)),
                ("Rotterdam".to_string(), (51.9244, 4.4777)),
                ("Utrecht".to_string(), (52.0907, 5.1214)),
                ("The Hague".to_string(), (52.0705, 4.3007)),
            ],
            "DE" => vec![
                ("Berlin".to_string(), (52.5200, 13.4050)),
                ("Munich".to_string(), (48.1351, 11.5820)),
                ("Hamburg".to_string(), (53.5511, 9.9937)),
                ("Frankfurt".to_string(), (50.1109, 8.6821)),
            ],
            "FR" => vec![
                ("Paris".to_string(), (48.8566, 2.3522)),
                ("Marseille".to_string(), (43.2965, 5.3698)),
                ("Lyon".to_string(), (45.7640, 4.8357)),
                ("Toulouse".to_string(), (43.6047, 1.4442)),
            ],
            "GR" => vec![
                ("Athens".to_string(), (37.9838, 23.7275)),
                ("Thessaloniki".to_string(), (40.6401, 22.9444)),
                ("Patras".to_string(), (38.2466, 21.7345)),
                ("Heraklion".to_string(), (35.3387, 25.1442)),
            ],
            "ES" => vec![
                ("Madrid".to_string(), (40.4168, -3.7038)),
                ("Barcelona".to_string(), (41.3851, 2.1734)),
                ("Valencia".to_string(), (39.4699, -0.3763)),
                ("Seville".to_string(), (37.3891, -5.9845)),
            ],
            "PK" => vec![
                ("Karachi".to_string(), (24.8607, 67.0011)),
                ("Lahore".to_string(), (31.5204, 74.3587)),
                ("Islamabad".to_string(), (33.6844, 73.0479)),
                ("Peshawar".to_string(), (34.0151, 71.5249)),
            ],
            // Fallback for any unexpected country codes
            _ => {
                debug!(
                    "Using default mock location for unsupported country: {}",
                    country
                );
                vec![("Unknown Location".to_string(), (0.0, 0.0))]
            },
        }
    }

    /// Generates a plausible mock measurement value for a given parameter and country.
    ///
    /// Uses base value ranges typical for each pollutant and adjusts them based on a
    /// predefined country-specific pollution factor (higher factor means higher values).
    fn generate_mock_value(&self, country: &str, parameter: &str, rng: &mut impl Rng) -> f64 {
        // Define typical baseline ranges for each parameter (units assumed µg/m³)
        let base_ranges: HashMap<&str, (f64, f64)> = [
            ("pm25", (5.0, 35.0)),
            ("pm10", (10.0, 50.0)),
            ("o3", (30.0, 100.0)), // Ozone often higher
            ("no2", (10.0, 60.0)),
            ("so2", (2.0, 20.0)),    // SO2 usually lower
            ("co", (200.0, 1200.0)), // CO has different scale/unit implicitly
        ]
        .iter()
        .cloned()
        .collect();

        // Apply a simple factor based on general pollution levels for the country
        let country_factor = match country {
            "NL" | "DE" | "FR" | "ES" => 0.9, // Slightly cleaner Western Europe
            "GR" => 1.1,                      // Slightly higher baseline assumed
            "PK" => 1.8,                      // Significantly higher baseline assumed
            _ => 1.0,                         // Default factor
        };

        // Get the base range or a default if parameter is unknown
        let (min_base, max_base) = base_ranges.get(parameter).unwrap_or(&(0.0, 10.0));

        // Adjust range based on country factor
        let adjusted_min = (min_base * country_factor).max(0.0); // Ensure min is not negative
        let adjusted_max = (max_base * country_factor).max(adjusted_min + 1.0); // Ensure max > min

        // Generate a random value within the adjusted range
        rng.gen_range(adjusted_min..adjusted_max)
    }

    /// Returns the standard unit string for a given air quality parameter.
    fn get_unit_for_parameter(&self, parameter: &str) -> String {
        // Assuming µg/m³ for all parameters for simplicity in mock data.
        // Real API might provide different units (e.g., ppm for CO).
        match parameter {
            "pm25" | "pm10" | "o3" | "no2" | "so2" | "co" => "µg/m³".to_string(),
            _ => "unknown".to_string(),
        }
    }
}
