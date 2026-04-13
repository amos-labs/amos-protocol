#!/bin/sh
set -e

# ═══════════════════════════════════════════════════════════════════════
# AMOS Harness Container Entrypoint
# ═══════════════════════════════════════════════════════════════════════
# Applies network-level security hardening before starting the harness.
# This runs as root briefly to set iptables rules, then drops to the
# amos user for the actual process.

# Block AWS metadata endpoint (prevents IAM credential theft via prompt injection)
# Fail hard if iptables is unavailable — running without network protection is unsafe.
if command -v iptables >/dev/null 2>&1; then
    iptables -A OUTPUT -d 169.254.169.254 -j DROP
    iptables -A OUTPUT -d 169.254.170.2 -j DROP
    # Also block IPv6-mapped metadata addresses
    ip6tables -A OUTPUT -d ::ffff:169.254.169.254 -j DROP 2>/dev/null || true
    ip6tables -A OUTPUT -d ::ffff:169.254.170.2 -j DROP 2>/dev/null || true
else
    echo "WARNING: iptables not available, metadata endpoint not blocked at network level" >&2
fi

# Make /proc/1/environ unreadable by non-root after startup.
# The harness runs as uid 1000 (amos), bash subprocesses run as uid 1001 (sandbox).
# This prevents sandbox from reading the harness process's environment variables.
chmod 0000 /proc/1/environ 2>/dev/null || true

# Drop to amos user and exec the harness
exec gosu amos "$@"
