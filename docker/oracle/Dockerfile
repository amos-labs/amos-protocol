# Multi-stage build for AMOS Oracle daemon
# Mission-aware decision layer — evaluates intake submissions and reviews
# completed bounties against constitutional/strategic mission alignment.
#
# Builder stage is byte-identical across all four service Dockerfiles.

# syntax=docker/dockerfile:1.7

# Stage 1: Builder (shared across all four service images)
FROM rust:bookworm AS builder

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./

# Copy all workspace crate sources (cargo requires all members for workspace resolution)
COPY amos-core ./amos-core
COPY amos-harness ./amos-harness
COPY amos-cli ./amos-cli
COPY amos-agent ./amos-agent
COPY amos-oracle ./amos-oracle
COPY amos-relay ./amos-relay
COPY amos-packages ./amos-packages

# Build all four service binaries in one RUN with cache mounts on cargo
# registry + target. Cache mounts don't survive into the image layer, so
# we copy the binaries to /out before exiting the RUN.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    RUST_MIN_STACK=16777216 SQLX_OFFLINE=true cargo build --release \
        --bin amos-harness --features "pkg-education,pkg-autoresearch" \
    && cargo build --release --bin amos-agent \
    && cargo build --release --bin amos-relay \
    && cargo build --release --bin amos-oracle-agent \
    && mkdir -p /out \
    && cp target/release/amos-harness /out/ \
    && cp target/release/amos-agent /out/ \
    && cp target/release/amos-relay /out/ \
    && cp target/release/amos-oracle-agent /out/

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime deps. Bedrock SigV4 signing uses HTTPS, so libssl + ca-certificates.
RUN apt-get update && \
    apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from the builder's /out (cache-mount-safe extraction)
COPY --from=builder /out/amos-oracle-agent /usr/local/bin/

# Copy mission documents the Oracle reads at startup. Pulled from the build
# context (not the builder stage — builder only copied workspace crates).
# These are versioned with the binary on purpose — a council-signed prompt
# update ships as a new image.
COPY amos-oracle/prompts /app/amos-oracle/prompts
COPY AGENT_CONTEXT.md /app/AGENT_CONTEXT.md
COPY docs/core/thesis.md /app/docs/core/thesis.md

# Non-root user
RUN useradd -m -u 1000 amos && \
    chown -R amos:amos /app

USER amos
WORKDIR /app

# ORACLE_PROJECT_ROOT defaults here so AmosMissionSource can locate the docs.
ENV ORACLE_PROJECT_ROOT=/app

# Run the daemon — long-running poll loop. No HTTP port; observability via logs.
CMD ["amos-oracle-agent"]
