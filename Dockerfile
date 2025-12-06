# Build stage
FROM rust:1.83-slim AS builder

WORKDIR /app

# Install dependencies for building
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy source to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src target/release/deps/app*

# Copy actual source
COPY src ./src

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/app /usr/local/bin/

# Create directories
RUN mkdir -p /app/public /app/config

# Default config
COPY config.yaml /app/config/config.yaml

EXPOSE 8080

ENTRYPOINT ["app"]
CMD ["serve", "--config", "/app/config/config.yaml"]
