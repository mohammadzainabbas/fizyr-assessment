# Use a Rust base image
FROM rust:latest AS builder

# Create a new empty shell project
WORKDIR /usr/src/app

# Copy manifests
COPY Cargo.toml ./

# Copy source code
COPY src ./src/
COPY rustfmt.toml ./

# Build dependencies and application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim AS runtime

# Install required runtime libraries:
# - libpq5: PostgreSQL client library needed by sqlx
# - ca-certificates: For verifying TLS certificates (used by reqwest with rustls)
RUN apt-get update && \
    apt-get install -y --no-install-recommends libpq5 ca-certificates && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Copy the binary and rename it
COPY --from=builder /usr/src/app/target/release/fizyr-assessment /usr/local/bin/air-quality-cli

# Set the entrypoint
ENTRYPOINT ["air-quality-cli"]
