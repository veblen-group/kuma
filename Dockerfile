# Dockerfile for Kuma Rust workspace
FROM rust:1.88-bookworm AS builder

ARG BINARY

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy Cargo files first for better caching
COPY Cargo.toml Cargo.lock ./
COPY crates/cli/Cargo.toml ./crates/cli/
COPY crates/core/Cargo.toml ./crates/core/
COPY crates/kumad/Cargo.toml ./crates/kumad/
COPY crates/backend/Cargo.toml ./crates/backend/

RUN cargo fetch --locked

# Copy source code
COPY . .

# Build specific binary based on BINARY arg
RUN cargo build --release --bin $BINARY

# Runtime stage
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

ARG BINARY

# Copy the specific binary from builder stage
COPY --from=builder /app/target/release/$BINARY /usr/local/bin/$BINARY

# Copy configuration files
COPY kuma.yaml /app/kuma.yaml
COPY tokens.*.json /app/

# Create non-root user
RUN useradd --create-home --shell /bin/bash kuma
USER kuma

# Set the binary as environment variable for runtime
ENV BINARY_NAME=$BINARY

CMD ["/bin/sh", "-c", "/usr/local/bin/${BINARY_NAME}"]