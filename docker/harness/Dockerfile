# Multi-stage build for AMOS Harness
# Per-customer AI business OS

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

# Build harness with all package features compiled in.
# Runtime control via AMOS_PACKAGES env var determines which packages are active.
RUN RUST_MIN_STACK=16777216 SQLX_OFFLINE=true cargo build --release --bin amos-harness \
    --features "pkg-education,pkg-autoresearch"

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies + security tooling
# iptables: network-level SSRF protection (block metadata endpoint, internal IPs)
# gosu: lightweight setuid to drop from root after iptables setup
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    fonts-liberation \
    iptables \
    gosu \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/amos-harness /usr/local/bin/

# Copy static assets (chat UI) into runtime image
COPY --from=builder /app/amos-harness/static /app/static

# Create non-root user with workspace directories
# sandbox (uid 1001) is an unprivileged user for bash tool subprocesses —
# it cannot read /proc/*/environ of the amos (uid 1000) harness process.
RUN useradd -m -u 1000 amos && \
    useradd -m -u 1001 -s /bin/sh sandbox && \
    mkdir -p /app/data /app/uploads /workspace /tmp/amos && \
    chown -R amos:amos /app /workspace /tmp/amos && \
    chown sandbox:sandbox /workspace /tmp/amos

# Copy entrypoint (runs as root for iptables, then drops to amos)
COPY docker/harness/entrypoint.sh /entrypoint.sh

WORKDIR /app

# Expose HTTP port
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

# Entrypoint applies network hardening then drops to amos user
ENTRYPOINT ["/entrypoint.sh"]
CMD ["amos-harness"]
