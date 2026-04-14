# Dockerfile for Kuma Rust workspace
#
# Uses cargo-chef to cache dependency compilation separately from source changes.
# After the first build, subsequent builds that only change workspace source
# (not Cargo.toml / Cargo.lock) skip the slow dep-compile step entirely.

FROM lukemathwalker/cargo-chef:latest-rust-1.91-bookworm AS chef

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# --- Planner: extract the dependency recipe from the workspace ---
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# --- Builder: compile deps (cached), then the requested binary ---
FROM chef AS builder

ARG BINARY

COPY --from=planner /app/recipe.json recipe.json
# This layer is cached as long as Cargo.toml / Cargo.lock are unchanged.
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release --bin $BINARY

# --- Runtime: minimal image with just the binary ---
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

ARG BINARY

COPY --from=builder /app/target/release/$BINARY /usr/local/bin/$BINARY

COPY kuma.yaml /app/kuma.yaml
COPY tokens.*.json /app/

RUN useradd --create-home --shell /bin/bash kuma
USER kuma

ENV BINARY_NAME=$BINARY

CMD ["/bin/sh", "-c", "/usr/local/bin/${BINARY_NAME}"]
