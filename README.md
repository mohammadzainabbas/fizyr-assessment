# Air Quality Analysis CLI

A command-line tool for analyzing air quality data from the OpenAQ API. This project was developed as part of the Fizyr assessment for the Data Pipeline / DevOps Engineer position.

## Features

- Interactive menu for easy operation.
- Fetch and store air quality data from the OpenAQ API (v3) for predefined countries (NL, DE, FR, GR, ES, PK).
- Initialize database schema.
- Import data for a specified number of past days (uses mock data as fallback if API fails).
- Find the most polluted country among the predefined list based on recent PM2.5 and PM10 data.
- Calculate average air quality metrics for a specific country over a specified number of days.
- Display the latest measurements for all parameters, grouped by city, for a specific country.
- Logging to `logs/app.log`.
- Multi-stage Docker build for efficient containerization.
- GitHub Actions workflow for CI checks (check, fmt, clippy).

## Prerequisites

- Rust (latest stable version)
- Docker and Docker Compose
- PostgreSQL (or use the provided Docker container)
- OpenAQ API key (set as OPENAQ_KEY environment variable)

## Setup

### Environment Variables

Create a `.env` file in the project root with the following:

```
DATABASE_URL=postgres://postgres:postgres@localhost:5432/air_quality
OPENAQ_KEY=your_openaq_api_key
RUST_LOG=info
```

### Running with Docker Compose

The easiest way to run the application is using Docker Compose:

```bash
docker-compose up
```

This will:
1. Start a PostgreSQL database
2. Build and run the application
3. Import the last 5 days of air quality data by default

### Manual Setup

1.  **Build the application:**
    ```bash
    cargo build
    ```
2.  **Run the interactive application:**
    ```bash
    cargo run
    ```

## Usage

Run the application using `cargo run`. You will be presented with an interactive menu:

```
Welcome to the Air Quality Analysis CLI!
? What would you like to do? ›
  Initialize Database Schema
  Exit
```

The available options will change based on the application's state (whether the database is initialized and data has been imported).

**Available Actions:**

1.  **Initialize Database Schema:** Creates the necessary `measurements` table and indexes in the PostgreSQL database. If run again, it ensures the schema exists.
2.  **Import Data:** Prompts for the number of past days to fetch data for. It fetches data for the predefined countries (NL, DE, FR, GR, ES, PK) from the OpenAQ API and stores it in the database. Uses mock data as a fallback if the API request fails.
3.  **Find Most Polluted Country:** Analyzes recent data (last 2 days) for PM2.5 and PM10 across the predefined countries and displays the country with the highest calculated pollution index.
4.  **Calculate Average Air Quality:** Prompts for a country code and the number of days. Calculates and displays the average values for various pollutants (PM2.5, PM10, O3, NO2, SO2, CO) for that country over the specified period.
5.  **Get Measurements by City:** Prompts for a country code. Displays a table showing the latest measurement value for each pollutant, grouped by city, within that country.
6.  **Exit:** Terminates the application.

## Development

### Running Tests

```bash
cargo test
```

### Formatting Code

```bash
cargo fmt
```

### Running Lints

```bash
cargo clippy
```

## Docker

The project includes a multi-stage Docker build for efficient containerization. The Dockerfile creates a slim runtime image with only the necessary dependencies.

## GitHub Actions

The repository includes a GitHub Actions workflow that runs:
- `cargo check`
- `cargo fmt -- --check`
- `cargo clippy`

This ensures code quality on every push and pull request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
