#!/usr/bin/env bash
#
# emit-stale-merge-metric.sh
#
# Emits a CloudWatch custom metric `BountySettledUnmerged` (count) to the
# `AMOS/Relay` namespace. The value is the number of bounties whose status is
# `approved` AND whose settlement_status is `settled` AND whose merge_commit_sha
# is NULL — i.e. tokens were paid out but the auto-merge bot never landed
# the PR. A non-zero value means the auto-merge loop is broken or stuck.
#
# Intended to run on a 5-minute cron from GitHub Actions (see
# .github/workflows/emit-stale-merge-metric.yml). Idempotent — re-running
# emits a fresh datapoint, no side effects on the relay.
#
# Required env:
#   RELAY_URL          (default: https://relay.amoslabs.com)
#   RELAY_API_KEY      (Bearer token for relay reads)
#   AWS_REGION         (default: us-east-1)

set -euo pipefail

RELAY_URL="${RELAY_URL:-https://relay.amoslabs.com}"
RELAY_API_KEY="${RELAY_API_KEY:?RELAY_API_KEY is required}"
REGION="${AWS_REGION:-us-east-1}"
NAMESPACE="AMOS/Relay"
METRIC_NAME="BountySettledUnmerged"

log() { echo "[stale-merge $(date -u +%H:%M:%S)] $*"; }

# Pull approved bounties; filter client-side to settled-but-unmerged.
log "Querying approved bounties from $RELAY_URL..."
APPROVED=$(curl -sf "${RELAY_URL}/api/v1/bounties?status=approved&limit=500" \
    -H "Authorization: Bearer ${RELAY_API_KEY}" 2>/dev/null || echo "[]")

COUNT=$(echo "$APPROVED" | python3 -c "
import sys, json
try:
    items = json.load(sys.stdin)
except Exception:
    items = []
n = 0
for b in items:
    if b.get('settlement_status') != 'settled':
        continue
    if b.get('merge_commit_sha'):
        continue
    n += 1
print(n)
")

log "Settled-but-unmerged count: $COUNT"

aws cloudwatch put-metric-data \
    --region "$REGION" \
    --namespace "$NAMESPACE" \
    --metric-name "$METRIC_NAME" \
    --value "$COUNT" \
    --unit Count

log "Published $NAMESPACE/$METRIC_NAME = $COUNT"
