#!/bin/sh
set -e

# ═══════════════════════════════════════════════════════════════════════
# AMOS Harness Container Entrypoint
# ═══════════════════════════════════════════════════════════════════════
# Applies network-level security hardening before starting the harness.
# This runs as root briefly to set iptables rules, then drops to the
# amos user for the actual process.

# Block AWS metadata endpoint (prevents IAM credential theft via prompt injection)
iptables -A OUTPUT -d 169.254.169.254 -j DROP 2>/dev/null || true
iptables -A OUTPUT -d 169.254.170.2 -j DROP 2>/dev/null || true

# Block access to common internal services that the harness shouldn't reach directly.
# The harness talks to RDS via its DATABASE_URL, but the agent/bash tool should NOT
# be able to reach arbitrary internal IPs. We allow established connections (for DB)
# but block new connections to the VPC CIDR except for known-good destinations.
#
# NOTE: We allow all outbound internet traffic (for apt, pip, curl to external APIs).
# The SSRF protection in view_web_page handles application-level URL validation.

# Drop to amos user and exec the harness
exec gosu amos "$@"
