<div align="center">
    <h3> Air Quality Analysis CLI <code>Assessment</code> 💻 </h3>
</div>

#

> [!NOTE]
> This project is a Rust CLI tool developed as a take-home assessment for the `Data Pipeline / DevOps Engineer` position at **Fizyr**. It fetches air quality data from the [OpenAQ API](https://openaq.org/), stores it in a PostgreSQL database, and provides query capabilities.

<div align="center">

<table>
  <tr>
    <td><strong>CI</strong></td>
    <td>
      <a href="https://github.com/mohammadzainabbas/fizyr-assessment/actions/workflows/ci-rust.yml">
        <img src="https://github.com/mohammadzainabbas/fizyr-assessment/actions/workflows/ci-rust.yml/badge.svg" alt="CI - Rust">
      </a>
    </td>
  </tr>
  <tr>
    <td><strong>Meta</strong></td>
    <td>
      <a href="https://www.rust-lang.org/">
        <img src="https://img.shields.io/badge/Rust-Stable-orange.svg" alt="Rust">
      </a>
      <a href="https://www.postgresql.org/">
        <img src="https://img.shields.io/badge/PostgreSQL-16-blue.svg" alt="PostgreSQL">
      </a>
      <a href="https://www.docker.com/">
        <img src="https://img.shields.io/badge/Docker-Enabled-blue.svg" alt="Docker">
      </a>
      <a href="https://spdx.org/licenses/">
        <img src="https://img.shields.io/badge/license-MIT-9400d3.svg" alt="License - MIT">
      </a>
      <a href="https://rust-reportcard.xuri.me/report/github/mohammadzainabbas/fizyr-assessment">
        <img src="https://rust-reportcard.xuri.me/badge/github.com/mohammadzainabbas/fizyr-assessment" alt="Rust Report Card">
      </a>
    </td>
  </tr>
</table>

</div>

> [!IMPORTANT]
> This repository contains the full implementation of the Air Quality Analysis CLI tool, designed to fetch, store, and analyze air quality data.
>
> **Key features:**
>
> - [x] Fetches air quality data from OpenAQ API v3 (locations, sensors, measurements) for specified countries (NL, DE, FR, GR, ES, PK).
> - [x] Stores normalized data in a PostgreSQL database.
> - [x] Interactive CLI menu for user operations (schema init, data import, queries).
> - [x] Query: Find the most polluted country based on recent PM2.5/PM10.
> - [x] Query: Calculate 5-day average air quality for a specified country.
> - [x] Query: Retrieve latest measurements grouped by locality for a specified country.
> - [x] Docker integration (`Dockerfile`, `docker-compose.yml`) for easy setup, including a custom network.
> - [x] GitHub Actions workflow for CI checks (`cargo check`, `cargo fmt -- --check`).
> - [x] Unit tests for CLI logic and integration tests for database operations.
> - [x] Logging to `logs/app.log`.

#

## Project Structure

- [**`src/`**](src/) – Rust source code for the CLI application.
  - [`main.rs`](src/main.rs) - Application entry point, logging setup, interactive loop.
  - [`api/`](src/api/) - Modules for interacting with external APIs (OpenAQ).
    - [`openaq.rs`](src/api/openaq.rs) - Client for the OpenAQ API.
    - [`mock.rs`](src/api/mock.rs) - Mock data provider (fallback/testing).
  - [`cli/`](src/cli/) - Command-line interface logic.
    - [`commands.rs`](src/cli/commands.rs) - Command definitions, state management, user prompts.
  - [`db/`](src/db/) - Database interaction logic.
    - [`postgres.rs`](src/db/postgres.rs) - PostgreSQL connection, schema, queries, insertion.
  - [`models/`](src/models/) - Data structures (API responses, DB records, output structs).
    - [`openaq.rs`](src/models/openaq.rs) - Defines `DailyMeasurement`, `DbMeasurement`, etc.
  - [`error.rs`](src/error.rs) - Custom application error types (`AppError`).
- [`logs/`](logs/) - Directory for application logs (created automatically).
- [`Dockerfile`](Dockerfile) - Defines the container image build process.
- [`docker-compose.yml`](docker-compose.yml) - Orchestrates the `app` and `database` services using a custom network.
- [**`.github/workflows/`**](.github/workflows/) – GitHub Actions CI pipeline (`ci-rust.yml`).
- [`Cargo.toml`](Cargo.toml) & [`Cargo.lock`](Cargo.lock) - Rust project dependencies.
- [`rustfmt.toml`](rustfmt.toml) - Code formatting configuration.
- [`LICENSE`](LICENSE) - Project license (MIT).

#

## Getting Started

### Prerequisites

- **Docker & Docker Compose:** Essential for the containerized setup.
  - [Install Docker](https://docs.docker.com/get-docker/)
  - [Install Docker Compose](https://docs.docker.com/compose/install/)
- **OpenAQ API Key:** Required for fetching real-time data from OpenAQ.
  - Sign up at [openaq.org](https://openaq.org/) to get your key.
- **`gh` CLI (Optional):** For cloning using the `gh repo clone` command. [Install gh](https://cli.github.com/).
- **Rust Toolchain (Optional):** Only needed if you plan to run or develop locally outside Docker. [Install Rust](https://www.rust-lang.org/tools/install).

### Running with Docker Compose (Recommended Workflow)

This method ensures the application runs in a consistent environment with its database dependency on a dedicated network.

1.  **Clone the Repository:**

```bash
# Using gh CLI
gh repo clone mohammadzainabbas/fizyr-assessment

# Or using standard git
git clone https://github.com/mohammadzainabbas/fizyr-assessment.git

cd fizyr-assessment
```

2.  **Configure API Key (Recommended: Use `.env` file):**

> [!IMPORTANT]
> The application requires your OpenAQ API key. The recommended way to provide it is via an `.env` file in the project root.

Create a file named `.env`:

```dotenv
# .env
OPENAQ_KEY=your_actual_api_key_here
```

The `docker-compose.yml` file is configured to read this file and pass the `OPENAQ_KEY` variable to the `app` container. Alternatively, you can export `OPENAQ_KEY` in your shell environment before running Docker Compose.

3.  **Start Database Service:**

Run the PostgreSQL database container in detached (background) mode. Data persists in the `postgres_data` named volume.

```bash
docker-compose up -d database
```

> [!TIP]
> Allow a few seconds for the database container to initialize fully before proceeding. You can check logs with `docker-compose logs -f database`.

4.  **Run Application Interactively:**

Use `docker-compose run` to build (if needed) and start the application interactively. This command creates a temporary container for the `app` service, connects it to the running `database` service on the shared network, and attaches your terminal.

```bash
docker-compose run --rm --build app
```

- `--rm`: Automatically removes the container when the application exits.
- `--build`: Rebuilds the application image if source code or `Dockerfile` changes.

> [!NOTE]
> You should see the welcome message and the interactive menu. Use your keyboard (arrow keys, Enter) to navigate. Using `docker-compose run` is generally preferred for interactive CLI applications like this over `docker-compose up app`, as it often handles terminal interactions (TTY) more reliably.

5.  **Using the CLI:**

Once the application starts, follow the menu prompts:

*   **Initialize Database Schema:** **Run this first!** Creates the `locations`, `sensors`, and `measurements` tables.
*   **Import Data:** Fetches top 10 locations/country, saves locations/sensors, then fetches daily measurements for sensors for the specified number of days (7-365). Includes retries for measurement fetching.
*   **Query Options:** Perform analysis like finding the most polluted country, calculating averages, or viewing city-specific data.

6.  **Stopping Services:**
*   **App Container:** Exit the application using the "Exit" menu option or press `Ctrl+C` in the terminal where `docker-compose run` is active. The container will be removed automatically due to `--rm`.
*   **Database Container:** Stop the background database service:

```bash
docker-compose down
```

> [!CAUTION]
> To stop the database AND **delete all stored air quality data**, use:
> ```bash
> docker-compose down -v
> ```

### Local Development Setup (Alternative)

If you prefer to run outside Docker:

1.  **Setup Environment:**
*   Install and run PostgreSQL locally.
*   Create a database (e.g., `createdb air_quality`).
*   Set environment variables (export in your shell or use a `.env` file and a tool like `dotenv-cli`):

```dotenv
# .env (Example - Adjust DATABASE_URL for your local setup)
DATABASE_URL=postgres://your_user:your_password@localhost:5432/air_quality
OPENAQ_KEY=your_actual_api_key_here
RUST_LOG=info # Optional: Set log level (e.g., debug, trace)
```

2.  **Build & Run:**

```bash
# Build the project
cargo build

# Run the interactive application (make sure the database is running first)
# If using .env, you might need a tool like dotenv-cli:
# dotenv cargo run
cargo run
```

> [!CAUTION]
> Ensure your PostgreSQL server is running before running `cargo run` and accessible via the `DATABASE_URL` environment variable. The application will not start if it cannot connect to the database.

Follow the CLI menu prompts as described in the Docker section.

3.  **Run Tests:**
*   **Unit Tests:** (Located in `src/cli/commands.rs`)

```bash
cargo test
```
*   **Database Integration Tests:** (Located in `src/db/postgres.rs`)
> [!IMPORTANT]
> These tests require a running PostgreSQL database accessible via the `DATABASE_URL` environment variable. This can be your local instance or the Dockerized one.

```bash
# 1. Ensure a PostgreSQL database is running and accessible via DATABASE_URL.
#    Example using Docker Compose:
#    docker-compose up -d database
#    export DATABASE_URL="postgres://postgres:postgres@localhost:5432/air_quality" # Set for local shell

# 2. Run only the integration tests using the feature flag:
cargo test --features integration-tests

# 3. Stop the Dockerized database if you started it just for the tests:
#    docker-compose down
```

#

## Implementation Overview

### Core Logic

The application fetches air quality data for a predefined list of countries (NL, DE, FR, GR, ES, PK) using the [OpenAQ API v3](https://docs.openaq.org/). The import process involves:

1. Fetching the top 10 locations for each country.
2. Saving these locations and their associated sensor details into dedicated database tables (`locations`, `sensors`).
3. Fetching daily aggregated measurements for each saved sensor within the user-specified date range, with retry logic for API errors.
4. Saving the fetched measurements into the `measurements` table.

The core functionality is exposed through an interactive Command Line Interface (CLI) built using `dialoguer`, allowing users to:

1.  Initialize the database schema.
2.  Import historical data.
3.  Perform analysis queries on the stored data.

### Database Schema

The database uses three main tables:

- **`locations`:** Stores information about each fetched location (ID, name, coordinates, country, etc.). `id` is the primary key.
- **`sensors`:** Stores details about each sensor (ID, name, parameter info) and includes a foreign key (`location_id`) linking back to the `locations` table. `id` is the primary key.
- **`measurements`:** Stores the daily aggregated air quality measurements.
  - **Columns:** Include `id`, `location_id` (denormalized), `sensor_id` (denormalized, corresponds to `sensors.id`), `location_name` (denormalized), `parameter_id` (denormalized), `parameter_name` (denormalized), `value_avg` (`NUMERIC`, nullable), `value_min` (`NUMERIC`, nullable), `value_max` (`NUMERIC`, nullable), `measurement_count` (`INT`, nullable), `unit` (denormalized), `date_utc` (`TIMESTAMPTZ`), `date_local` (`TEXT`), `country` (denormalized), `city` (denormalized locality), `latitude` (denormalized), `longitude` (denormalized), `is_mobile` (denormalized), `is_monitor` (denormalized), `owner_name` (denormalized), `provider_name` (denormalized), and `created_at`.
  - **Constraint:** A `UNIQUE` constraint exists on `(sensor_id, date_utc)` to prevent duplicate daily entries for the same sensor.
- **Initialization:** All tables are created idempotently (`CREATE TABLE IF NOT EXISTS`) by the `init_schema` function in `src/db/postgres.rs`, triggered via the CLI.
- **Indexes:** Created on relevant columns in `measurements` (e.g., `country`, `parameter_name`, `date_utc`, `sensor_id`) to optimize query performance.

### API Interaction (`src/api/`)

- **Client:** `OpenAQClient` in `openaq.rs` uses `reqwest` to make asynchronous GET requests to the relevant OpenAQ v3 endpoints (e.g., `/v3/locations`, `/v3/sensors/{id}/measurements/daily`).
- **Authentication:** Uses the `X-API-Key` header as required by OpenAQ API v3.
- **Error Handling:** Includes checks for network errors and non-success HTTP status codes (4xx, 5xx), logging relevant details. Pagination is handled within the client methods.
- **Fallback:** Mock data provider is no longer used for import fallback. API errors during import are logged, and processing may skip affected countries/sensors.

### CLI Interface (`src/cli/`)

- **Interaction:** `dialoguer` provides interactive prompts (text input, selection menus).
- **Commands:** Defined in the `Commands` enum. `clap` is used implicitly via the enum structure but full argument parsing is not implemented.
- **State Management:** `AppState` enum tracks whether the database is initialized and if data has been imported, dynamically adjusting the available menu options presented to the user in `main.rs`.
- **Output:** `comfy-table` is used to display query results in formatted tables. `colored` enhances terminal output. `indicatif` provides spinners and progress bars for long-running operations.

#

## Development Decisions & Design Choices

- **Language (Rust):** Chosen for performance, memory safety, strong typing, and its excellent ecosystem for CLI tools (`clap`, `dialoguer`, `indicatif`) and asynchronous operations (`tokio`, `reqwest`, `sqlx`).
- **Database (PostgreSQL):** A robust, open-source relational database well-suited for structured time-series data and analytical queries. `sqlx` provides compile-time checked SQL queries.
- **Containerization (Docker):** Simplifies setup and ensures environment consistency using `Dockerfile` (multi-stage build for smaller image) and `docker-compose.yml`. A custom network (`air_quality_net`) isolates the application and database services.
- **API Client (`reqwest`):** A mature and widely used asynchronous HTTP client in the Rust ecosystem.
- **Error Handling (`thiserror`, Custom Enum):** Centralized error handling using the `AppError` enum and `thiserror` provides clear, context-specific error types, improving debugging and robustness. `Arc` is used to wrap non-`Clone` errors.
- **Modularity:** The codebase is organized into logical modules (`api`, `cli`, `db`, `models`, `error`) promoting separation of concerns and maintainability.
- **Testing:**
    - Unit tests (`src/cli/commands.rs`) use mocking (`MockDatabase`) to test CLI command logic in isolation.
    - Integration tests (`src/db/postgres.rs`) use the `sqlx::test` macro for transactional tests against a real database instance, gated by the `integration-tests` feature flag.
- **Data Import:** Fetches top 10 locations per country, saves locations and sensors to dedicated tables, then fetches daily measurements for each sensor (with retries) and saves them. Uses `ON CONFLICT (id) DO NOTHING` for locations/sensors and `ON CONFLICT (sensor_id, date_utc) DO NOTHING` for measurements to handle duplicates.
- **Pollution Index:** Implements a simple weighted index (`pm2.5 * 1.5 + pm10`) for the "most polluted" feature, prioritizing PM2.5.

#

## Future Improvements

- **Robust Error Handling:** Add more sophisticated retry logic (e.g., with exponential backoff) for transient network or API errors during import.
- **Configuration File:** Move settings (country list, API URL, DB connection details) to a configuration file (e.g., `config.toml`) instead of environment variables or hardcoding.
- **Database Migrations:** Use a dedicated migration tool (like `sqlx-cli` or `refinery`) for more robust schema management instead of `CREATE TABLE IF NOT EXISTS`.
- **Non-Interactive Mode:** Add command-line flags (using `clap` more extensively) for running commands non-interactively (e.g., `air-quality-cli import --days 30`).
- **Query Filtering:** Allow users to specify date ranges or parameters for queries via CLI options.
- **Enhanced Testing:** Increase unit test coverage, particularly for edge cases. Add end-to-end tests simulating full CLI interaction.

#

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
