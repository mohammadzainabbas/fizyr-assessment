# Air Quality Analysis CLI

A command-line tool for analyzing air quality data from the OpenAQ API. This project was developed as part of the Fizyr assessment for the Data Pipeline / DevOps Engineer position.

## Features

- Fetch and store air quality data from the OpenAQ API
- Find the most polluted country among a predefined list
- Calculate 5-day average air quality for a specific country
- Display all available measurements for a specific country

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

1. Install dependencies:

```bash
cargo build
```

2. Initialize the database:

```bash
cargo run -- init-db
```

3. Import data:

```bash
cargo run -- import --days 5
```

## Usage

### Find the Most Polluted Country

```bash
cargo run -- most-polluted
```

This will analyze the latest data and determine which country among Netherlands, Germany, France, Greece, Spain, and Pakistan has the highest pollution index.

### Calculate Average Air Quality

```bash
cargo run -- average --country NL --days 5
```

This will calculate the 5-day average air quality for the Netherlands. You can change the country code to one of: NL, DE, FR, GR, ES, or PK.

### Get All Measurements for a Country

```bash
cargo run -- measurements --country DE
```

This will display all available measurements for Germany from the database.

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