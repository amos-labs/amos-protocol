# Multi-stage build for AMOS Agent
# Autonomous AI agent that registers with harness and runs tasks

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
COPY amos-relay ./amos-relay
COPY amos-packages ./amos-packages

# Build only the agent binary
RUN SQLX_OFFLINE=true cargo build --release --bin amos-agent

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies + security tooling
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    iptables \
    gosu \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/amos-agent /usr/local/bin/

# Create non-root user with workspace directories
RUN useradd -m -u 1000 amos && \
    mkdir -p /app/data /workspace /tmp/amos && \
    chown -R amos:amos /app /workspace /tmp/amos

# Copy entrypoint (runs as root for iptables, then drops to amos)
COPY docker/agent/entrypoint.sh /entrypoint.sh

WORKDIR /app

# Expose HTTP port
EXPOSE 3100

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:3100/health || exit 1

# Entrypoint applies network hardening then drops to amos user
ENTRYPOINT ["/entrypoint.sh"]
CMD ["amos-agent", "--serve"]
