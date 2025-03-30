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
      <a href="https://github.com/mohammadzainabbas/bol-assessment/actions/workflows/ci-rust.yml">
        <img src="https://github.com/mohammadzainabbas/bol-assessment/actions/workflows/ci-rust.yml/badge.svg" alt="CI - Rust">
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
> This repository contains the full implementation of the Air Quality Analysis CLI tool.
>
> **Key features:**
>
> - [x] Fetches air quality data from OpenAQ API v3 for specified countries.
> - [x] Stores normalized data in a PostgreSQL database.
> - [x] Interactive CLI menu for user operations.
> - [x] Database schema initialization.
> - [x] Data import functionality with configurable history duration.
> - [x] Query: Find the most polluted country (NL, DE, FR, GR, ES, PK) based on recent PM2.5/PM10.
> - [x] Query: Calculate 5-day average air quality for a specified country.
> - [x] Query: Retrieve latest measurements grouped by city for a specified country.
> - [x] Docker integration (`Dockerfile`, `docker-compose.yml`) for application and database.
> - [x] GitHub Actions workflow for CI checks (`cargo check`, `cargo fmt -- --check`).
> - [x] Basic unit and integration tests.
> - [x] Logging to `logs/app.log`.

#

## Project Structure

- [**`src/`**](src/) – Rust source code for the CLI application.
  - [`main.rs`](src/main.rs) - Main application entry point, sets up logging and runs the interactive loop.
  - [`api/`](src/api/) - Modules for interacting with external APIs (OpenAQ).
    - [`openaq.rs`](src/api/openaq.rs) - Client for fetching data from the OpenAQ API.
    - [`mock.rs`](src/api/mock.rs) - Provides mock data for fallback scenarios.
  - [`cli/`](src/cli/) - Modules related to the command-line interface.
    - [`commands.rs`](src/cli/commands.rs) - Defines CLI commands and handles user interaction/prompts.
  - [`db/`](src/db/) - Modules for database interaction.
    - [`postgres.rs`](src/db/postgres.rs) - Handles PostgreSQL connection, schema initialization, and data querying/insertion.
  - [`models/`](src/models/) - Data structures and models used throughout the application.
    - [`openaq.rs`](src/models/openaq.rs) - Structs representing data fetched from OpenAQ and stored in the DB.
  - [`error.rs`](src/error.rs) - Defines custom error types for the application.
- [`tests/`](tests/) - Contains integration or end-to-end tests (if any). *Note: Current integration tests are within `src/db/postgres.rs`.*
- [`logs/`](logs/) - Directory where application logs (`app.log`) are stored (created automatically).
- [`Dockerfile`](Dockerfile) - Multi-stage Dockerfile for building an optimized application image.
- [`docker-compose.yml`](docker-compose.yml) - Docker Compose file to orchestrate the application and database services.
- [**`.github/workflows/`**](.github/workflows/) – GitHub Actions CI pipeline.
  - `ci.yml` - Runs `cargo check`, `cargo fmt -- --check`.
- [`Cargo.toml`](Cargo.toml) - Rust project manifest file.
- [`rustfmt.toml`](rustfmt.toml) - Configuration for code formatting.
- [`LICENSE`](LICENSE) - Project license file (MIT).
- [`README.md`](README.md) - This file.

#

## Getting Started

### Prerequisites

- **Docker & Docker Compose:** Required for running the application and database via containers. [Install Docker](https://docs.docker.com/get-docker/), [Install Docker Compose](https://docs.docker.com/compose/install/).
- **OpenAQ API Key:** Needed to fetch real data. Sign up at [OpenAQ](https://openaq.org/) and obtain an API key.
- **Rust Toolchain:** Required for local development and building. [Install Rust](https://www.rust-lang.org/tools/install).

### Running with Docker Compose (Recommended)

This is the easiest way to run the application and its database dependency.

1.  **Clone the repository:**
    ```bash
    git clone <repository_url> # Replace with the actual URL
    cd fizyr-assessment
    ```

2.  **Set Environment Variable:**
    You need to provide your OpenAQ API key. The application expects it in the `OPENAQ_KEY` environment variable. You can either:
    *   Export it in your shell: `export OPENAQ_KEY='your_api_key'`
    *   Create a `.env` file in the project root:
        ```dotenv
        # .env
        OPENAQ_KEY=your_api_key
        ```
        *Note: `docker-compose.yml` is configured to pass this variable to the `app` container.*

3.  **Start Database Service:**
    Run the database container in the background. It uses a named volume (`postgres_data`) for persistence.
    ```bash
    docker-compose up -d database
    ```
    Wait a few seconds for the database to initialize.

4.  **Run Application Interactively:**
    Use `docker-compose run` to start the application container interactively. This connects to the running database. The `--rm` flag removes the container on exit.
    ```bash
    docker-compose run --rm --build app
    ```
    You should see the welcome message and the interactive menu. Use your keyboard to navigate and select options.

    *Why `run` instead of `up app`?* While `docker-compose up app` is typically used for foreground services, interaction issues were observed on some systems. `docker-compose run` provides a more reliable interactive experience in this case, connecting to the persistent database started in the previous step.

5.  **Using the CLI:**
    Follow the prompts in the interactive menu:
    *   **Initialize Database Schema:** Run this first to create the necessary table.
    *   **Import Data:** Fetch data from OpenAQ for a specified number of days.
    *   **Query Options:** Explore the available analysis features.

6.  **Stopping Services:**
    *   To stop the interactive `app` container, exit the application via its menu or press `Ctrl+C`.
    *   To stop the background database container:
        ```bash
        docker-compose down
        ```
    *   To stop the database and *remove its data volume*:
        ```bash
        docker-compose down -v
        ```

### Local Development Setup

1.  **Setup Environment:**
    *   Ensure PostgreSQL is installed and running.
    *   Create the database (e.g., `createdb air_quality`).
    *   Set environment variables (either export or use a `.env` file):
        ```dotenv
        # .env
        DATABASE_URL=postgres://your_user:your_password@localhost:5432/air_quality # Adjust connection string
        OPENAQ_KEY=your_api_key
        RUST_LOG=info # Optional: Adjust log level (e.g., debug, trace)
        ```

2.  **Build & Run:**
    ```bash
    # Build the project
    cargo build

    # Run the interactive application
    cargo run
    ```

3.  **Run Tests:**
    *   **Unit Tests:** (Currently minimal, primarily integration tests exist)
        ```bash
        cargo test
        ```
    *   **Database Integration Tests:** These tests require a running database configured via `DATABASE_URL`. They are marked with `#[cfg(feature = "integration-tests")]` and use the `sqlx::test` macro.
        ```bash
        # 1. Start the database service (if not already running)
        docker-compose up -d database

        # 2. Run the integration tests using the feature flag
        cargo test --features integration-tests

        # 3. Stop the database service when done
        docker-compose down
        ```

#

## Implementation Overview

### Core Logic

The application fetches air quality measurements from the OpenAQ API for a predefined list of countries (NL, DE, FR, GR, ES, PK). The data includes parameters like PM2.5, PM10, O3, etc., along with location details and timestamps.

Fetched data is stored in a PostgreSQL database (`measurements` table). The CLI provides an interactive menu allowing users to:
1.  Initialize the database schema.
2.  Import historical data for a specified period.
3.  Perform analysis queries on the stored data.

### Database Schema

A single table `measurements` stores the data. Key columns include location details, parameter, value, unit, timestamps (UTC and local), and coordinates. Indexes are created on `country`, `parameter`, and `date_utc` to optimize query performance. Schema initialization is handled by the `init_schema` function in `src/db/postgres.rs`, triggered via the CLI menu.

### API Interaction

The `src/api/openaq.rs` module uses the `reqwest` library to interact with the OpenAQ API v3 `/measurements` endpoint. It handles pagination and rate limiting (basic delay). Mock data is provided in `src/api/mock.rs` as a fallback if API calls fail.

### CLI Interface

The `dialoguer` crate provides the interactive menu. The `clap` crate (though primarily used for parsing here via `Commands` enum) structures the available actions. State management (`AppState` in `src/cli/commands.rs`) dynamically adjusts the menu options based on whether the schema is initialized and data is imported.

#

## Development Decisions & Design Choices

- **Language Choice (Rust):** Chosen for its performance, safety features, and strong ecosystem for CLI tools and web services, aligning with requirements for robust data pipeline components.
- **Database (PostgreSQL):** A powerful open-source relational database suitable for structured data and complex queries.
- **Containerization (Docker):** Ensures a consistent environment for development and deployment, simplifying setup. Multi-stage builds optimize the final image size.
- **API Client (`reqwest`):** A popular and ergonomic HTTP client for Rust.
- **CLI Interaction (`dialoguer`):** Provides a user-friendly interactive menu experience.
- **Error Handling:** Custom error types (`AppError` in `src/error.rs`) provide specific contexts for failures (API, DB, IO, etc.). `thiserror` crate is used for boilerplate implementation.
- **Modularity:** Code is organized into modules (`api`, `cli`, `db`, `models`) for better separation of concerns.
- **Testing:** Includes basic unit tests and database integration tests (gated by the `integration-tests` feature flag and using the `sqlx::test` macro for automatic transaction management and database setup/teardown).
- **Data Import Strategy:** Data is fetched per country and parameter to handle potential API limitations and allow for incremental updates (though current implementation fetches all requested days at once). `ON CONFLICT DO NOTHING` is used during insertion to handle potential duplicate measurements gracefully.
- **Pollution Index Calculation:** A simple weighted index (`pm2.5 * 1.5 + pm10`) is used for the "most polluted" calculation, prioritizing PM2.5 based on common health impact assessments.

#

## Future Improvements

- **More Sophisticated Error Handling:** Implement more robust retry logic for API calls with exponential backoff.
- **Configuration File:** Move settings like country list, API base URL, database connection details etc., to a configuration file (e.g., TOML) instead of relying solely on environment variables or hardcoding.
- **Advanced Database Features:** Explore partitioning or more complex indexing for very large datasets if performance becomes an issue.
- **Asynchronous Data Ingestion:** Implement background tasks or a separate service for continuous data fetching and updates.
- **Expanded CLI Options:** Add command-line flags for non-interactive use (e.g., `air-quality-cli import --days 30`), filtering options for queries (e.g., specific parameters, date ranges).
- **Enhanced Testing:** Increase unit test coverage, add end-to-end tests simulating user interaction via the CLI.
- **Deployment Strategy:** Document steps for deploying the application (e.g., as a standalone binary, container in a cloud environment).

#

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
