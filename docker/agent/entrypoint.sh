#!/bin/sh
set -e

# ═══════════════════════════════════════════════════════════════════════
# AMOS Agent Container Entrypoint
# ═══════════════════════════════════════════════════════════════════════
# Same network hardening as the harness — block metadata endpoint.

# Block AWS metadata endpoint (prevents IAM credential theft)
if command -v iptables >/dev/null 2>&1; then
    iptables -A OUTPUT -d 169.254.169.254 -j DROP
    iptables -A OUTPUT -d 169.254.170.2 -j DROP
    ip6tables -A OUTPUT -d ::ffff:169.254.169.254 -j DROP 2>/dev/null || true
    ip6tables -A OUTPUT -d ::ffff:169.254.170.2 -j DROP 2>/dev/null || true
else
    echo "WARNING: iptables not available, metadata endpoint not blocked at network level" >&2
fi

# Make /proc/1/environ unreadable by non-root
chmod 0000 /proc/1/environ 2>/dev/null || true

# Drop to amos user and exec the agent
exec gosu amos "$@"
