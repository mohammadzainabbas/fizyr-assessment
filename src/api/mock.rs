use crate::error::Result;
use crate::models::{Coordinates, Dates, Measurement};
use chrono::{DateTime, Duration, Utc};
use rand::distributions::{Distribution, Uniform};
use rand::{thread_rng, Rng};
use std::collections::HashMap;

/// Mock data provider for OpenAQ API
/// Used as a fallback when the actual API is not available or for testing
pub struct MockDataProvider {
    countries: Vec<String>,
}

impl MockDataProvider {
    /// Create a new mock data provider
    pub fn new() -> Self {
        Self {
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

    /// Generate mock measurements for a specific country within a date range
    pub fn get_measurements_for_country_in_date_range(
        &self,
        country: &str,
        date_from: DateTime<Utc>,
        date_to: DateTime<Utc>,
    ) -> Result<Vec<Measurement>> {
        // Check if country is valid
        if !self.countries.contains(&country.to_string()) {
            return Ok(Vec::new());
        }

        let mut rng = thread_rng();
        let param_dist = Uniform::new(0, 6); // Changed upper bound to 6 (exclusive) to include 5

        // Generate a reasonable number of measurements for the date range
        let days_diff = (date_to - date_from).num_days();
        let num_measurements = rng.gen_range(50..200);

        let mut measurements = Vec::with_capacity(num_measurements as usize);

        // Create mock locations for each country
        let locations = self.get_mock_locations_for_country(country);

        // For each measurement, generate a random date within the range
        for _ in 0..num_measurements {
            let random_days = rng.gen_range(0..days_diff);
            let random_hours = rng.gen_range(0..24);
            let random_mins = rng.gen_range(0..60);

            let measurement_date = date_from
                + Duration::days(random_days)
                + Duration::hours(random_hours)
                + Duration::minutes(random_mins);

            // Get a random location for this country
            let location_idx = rng.gen_range(0..locations.len());
            let (location_name, coords) = &locations[location_idx];

            // Add some variation to the coordinates
            let lat_variation = rng.gen_range(-0.01..0.01);
            let lon_variation = rng.gen_range(-0.01..0.01);

            let parameter = match param_dist.sample(&mut rng) {
                0 => "pm25",
                1 => "pm10",
                2 => "o3",
                3 => "no2",
                4 => "so2",
                _ => "co",
            };

            // Generate a reasonable value based on parameter and country
            let value = self.generate_mock_value(country, parameter, &mut rng);

            // Create a measurement
            let measurement = Measurement {
                location_id: (location_idx as i64) + 1000, // Start from 1000
                location: location_name.to_string(),
                parameter: parameter.to_string(),
                value,
                date: Dates {
                    utc: measurement_date,
                    local: measurement_date.to_rfc3339(),
                },
                unit: self.get_unit_for_parameter(parameter),
                coordinates: Some(Coordinates {
                    latitude: Some(coords.0 + lat_variation),
                    longitude: Some(coords.1 + lon_variation),
                }),
                country: country.to_string(),
                city: Some(location_name.clone()),
            };

            measurements.push(measurement);
        }

        // Sort by date
        measurements.sort_by(|a, b| a.date.utc.cmp(&b.date.utc));

        Ok(measurements)
    }

    /// Get mock locations for a specific country
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
            _ => vec![("Unknown".to_string(), (0.0, 0.0))],
        }
    }

    /// Generate a mock value for a parameter based on country
    /// The values are based on realistic AQI values for each parameter
    fn generate_mock_value(&self, country: &str, parameter: &str, rng: &mut impl Rng) -> f64 {
        // Base value ranges for parameters
        let base_ranges: HashMap<&str, (f64, f64)> = [
            ("pm25", (5.0, 35.0)),
            ("pm10", (10.0, 50.0)),
            ("o3", (30.0, 100.0)),
            ("no2", (10.0, 60.0)),
            ("so2", (5.0, 30.0)),
            ("co", (400.0, 1500.0)),
        ]
        .iter()
        .cloned()
        .collect();

        // Country pollution factors (1.0 is neutral, higher is more polluted)
        let country_factor = match country {
            "NL" => 0.8, // Netherlands - cleaner air
            "DE" => 0.9, // Germany
            "FR" => 0.9, // France
            "GR" => 1.1, // Greece
            "ES" => 0.9, // Spain
            "PK" => 1.5, // Pakistan - more polluted
            _ => 1.0,
        };

        let (min, max) = base_ranges.get(parameter).unwrap_or(&(0.0, 10.0));
        let adjusted_min = min * country_factor;
        let adjusted_max = max * country_factor;

        // Generate a random value within the adjusted range
        rng.gen_range(adjusted_min..adjusted_max)
    }

    /// Get the unit for a parameter
    fn get_unit_for_parameter(&self, parameter: &str) -> String {
        match parameter {
            "pm25" => "µg/m³".to_string(),
            "pm10" => "µg/m³".to_string(),
            "o3" => "µg/m³".to_string(),
            "no2" => "µg/m³".to_string(),
            "so2" => "µg/m³".to_string(),
            "co" => "µg/m³".to_string(),
            _ => "unknown".to_string(),
        }
    }
}
