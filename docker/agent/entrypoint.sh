#!/bin/sh
set -e

# ═══════════════════════════════════════════════════════════════════════
# AMOS Agent Container Entrypoint
# ═══════════════════════════════════════════════════════════════════════
# Same network hardening as the harness — block metadata endpoint.

# Block AWS metadata endpoint (prevents IAM credential theft)
iptables -A OUTPUT -d 169.254.169.254 -j DROP 2>/dev/null || true
iptables -A OUTPUT -d 169.254.170.2 -j DROP 2>/dev/null || true

# Drop to amos user and exec the agent
exec su-exec amos "$@"
