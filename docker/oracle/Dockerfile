# Multi-stage build for AMOS Oracle daemon
# Mission-aware decision layer — evaluates intake submissions and reviews
# completed bounties against constitutional/strategic mission alignment.

# Stage 1: Builder
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

# Build only the oracle daemon binary
RUN SQLX_OFFLINE=true cargo build --release --bin amos-oracle-agent

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime deps. Bedrock SigV4 signing uses HTTPS, so libssl + ca-certificates.
RUN apt-get update && \
    apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/amos-oracle-agent /usr/local/bin/

# Copy mission documents the Oracle reads at startup. Pulled from the build
# context (not the builder stage — builder only copied workspace crates).
# These are versioned with the binary on purpose — a council-signed prompt
# update ships as a new image.
COPY amos-oracle/prompts /app/amos-oracle/prompts
COPY AGENT_CONTEXT.md /app/AGENT_CONTEXT.md
COPY docs/AMOS_THESIS_AND_STRATEGY.md /app/docs/AMOS_THESIS_AND_STRATEGY.md

# Non-root user
RUN useradd -m -u 1000 amos && \
    chown -R amos:amos /app

USER amos
WORKDIR /app

# ORACLE_PROJECT_ROOT defaults here so AmosMissionSource can locate the docs.
ENV ORACLE_PROJECT_ROOT=/app

# Run the daemon — long-running poll loop. No HTTP port; observability via logs.
CMD ["amos-oracle-agent"]
