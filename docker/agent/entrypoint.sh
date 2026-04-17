#!/bin/sh
set -e

# ═══════════════════════════════════════════════════════════════════════
# AMOS Agent Container Entrypoint
# ═══════════════════════════════════════════════════════════════════════
# Same network hardening as the harness — block metadata endpoint.

# Block AWS metadata endpoint (prevents IAM credential theft via prompt injection)
# On Fargate without NET_ADMIN capability, iptables will fail — log and continue.
if command -v iptables >/dev/null 2>&1; then
    if iptables -A OUTPUT -d 169.254.169.254 -j DROP 2>/dev/null && \
       iptables -A OUTPUT -d 169.254.170.2 -j DROP 2>/dev/null; then
        ip6tables -A OUTPUT -d ::ffff:169.254.169.254 -j DROP 2>/dev/null || true
        ip6tables -A OUTPUT -d ::ffff:169.254.170.2 -j DROP 2>/dev/null || true
        echo "iptables: metadata endpoint blocked (SSRF protection active)"
    else
        echo "WARNING: iptables rules failed (no NET_ADMIN capability) — metadata endpoint not blocked" >&2
        echo "         Running without network-level SSRF protection. IAM role should have minimal permissions." >&2
    fi
else
    echo "WARNING: iptables not available, metadata endpoint not blocked at network level" >&2
fi

# Make /proc/1/environ unreadable by non-root
chmod 0000 /proc/1/environ 2>/dev/null || true

# Drop to amos user and exec the agent
exec gosu amos "$@"
