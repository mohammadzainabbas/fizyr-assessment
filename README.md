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

1.  **Initialize Database Schema:** Creates the necessary `measurements` table and indexes in the PostgreSQL database. If run again, it ensures the schema exists. (See [Database Schema](#database-schema) section for details).
2.  **Import Data:** Prompts for the number of past days to fetch data for (minimum 7 days, maximum 365 days). It fetches data for the predefined countries (Netherlands, Germany, France, Greece, Spain, Pakistan) from the OpenAQ API and stores it in the database. Uses mock data as a fallback if the API request fails.

    *Example:* Importing data for the last 365 days:
    ```
    ✔ What would you like to do? · Import Data

    ---

    ✔ Enter number of days for history (min 7, max 365) · 365
    Importing data for the last 365 days
      [00:00:01] [########################################] 12/12 (100%) Data import completed successfully!
    ```
    *Manual Verification (Date Range):* To check the date range of the imported data, connect to the database using `psql` (see [Manual Verification](#manual-verification) under Database Schema) and run:
    ```sql
    SELECT
        MIN(date_utc) AS earliest_date,
        MAX(date_utc) AS latest_date,
        (MAX(date_utc)::date - MIN(date_utc)::date) AS days_span
    FROM
        measurements;
    ```
    This query shows the earliest and latest timestamps and calculates the number of days spanned by the data.

3.  **Find Most Polluted Country:** Analyzes recent data (last 7 days) for PM2.5 and PM10 across the predefined countries (Netherlands, Germany, France, Greece, Spain, Pakistan) and displays the country (full name and code) with the highest calculated pollution index.
4.  **Calculate Average Air Quality:** Prompts for a country (selecting from a list showing full names and codes) and the number of days. Calculates and displays the average values for various pollutants (PM2.5, PM10, O3, NO2, SO2, CO) for that country over the specified period. Displays the full country name and code in the output.
5.  **Get Measurements by City:** Prompts for a country (selecting from a list showing full names and codes). Displays a table showing the latest measurement value for each pollutant, grouped by city, within that country. Displays the full country name and code in the output header.
6.  **Exit:** Terminates the application.

## Database Schema

The application uses a PostgreSQL database to store air quality measurements.

### Initialization

The database schema can be initialized using the interactive CLI. When you run the application (`cargo run`), select the "Initialize Database Schema" option:

```
Welcome to the Air Quality Analysis CLI!
✔ What would you like to do? · Initialize Database Schema

---

Initializing database schema...
⠏ Database schema initialized successfully!
---
```

This command creates the `measurements` table if it doesn't exist.

### `measurements` Table Structure

The schema consists of a single table named `measurements` with the following structure:

| Column Name | Data Type                 | Nullable | Description                                      |
|-------------|---------------------------|----------|--------------------------------------------------|
| id          | integer                   | NO       | Primary key (auto-incrementing)                  |
| location_id | bigint                    | NO       | OpenAQ location ID                               |
| location    | text                      | NO       | OpenAQ location name                             |
| parameter   | text                      | NO       | Pollutant parameter (e.g., 'pm25', 'o3')         |
| value       | numeric                   | NO       | Measured value                                   |
| unit        | text                      | NO       | Unit of measurement (e.g., 'µg/m³')              |
| date_utc    | timestamp with time zone  | NO       | Measurement timestamp in UTC                     |
| date_local  | text                      | NO       | Measurement timestamp in local time (ISO format) |
| country     | text                      | NO       | Country code (e.g., 'NL', 'DE')                  |
| city        | text                      | YES      | City name                                        |
| latitude    | double precision          | YES      | Latitude coordinate                              |
| longitude   | double precision          | YES      | Longitude coordinate                             |
| created_at  | timestamp with time zone  | NO       | Timestamp when the record was inserted           |

*Note: The `id` column is managed by a sequence (`measurements_id_seq`), and there is an index on `(location_id, parameter, date_utc)` for efficient querying.*

### Manual Verification

You can manually verify the schema using `psql`. First, connect to the database (ensure your `DATABASE_URL` in `.env` is correct):

```bash
# Example using the default connection string
psql postgres://postgres:postgres@localhost:5432/air_quality
```

Then, run the following SQL query to inspect the `measurements` table structure:

```sql
SELECT
    column_name,
    data_type,
    is_nullable
FROM
    information_schema.columns
WHERE
    table_schema = 'public' AND table_name = 'measurements'
ORDER BY
    ordinal_position;
```

Alternatively, you can use the `psql` shortcut command:

```sql
\d measurements
```

This will display the table definition, including columns, types, indexes, and constraints.

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
