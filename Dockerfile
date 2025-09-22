# Dockerfile for Kuma Rust workspace
FROM rust:1.89-bookworm AS builder

ARG BINARY

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy Cargo files first for better caching
COPY . .

# Build specific binary based on BINARY arg
RUN cargo build --release --bin $BINARY

# Runtime stage
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y \
    ca-certificates \
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

ENV BINARY_NAME=$BINARY

CMD ["/bin/sh", "-c", "/usr/local/bin/${BINARY_NAME}"]
