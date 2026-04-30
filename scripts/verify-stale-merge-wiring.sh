#!/usr/bin/env bash
#
# verify-stale-merge-wiring.sh — read-only verifier for the
# settled-but-unmerged backlog observability stack.
#
# This script IS the bounty's `test_command`. Read-only against AWS + the
# repo (no curl, no put-metric-data). Exits 0 when everything is wired,
# non-zero with a single diagnostic line on the first failure.
#
# Verifies:
#   1. scripts/emit-stale-merge-metric.sh exists, is executable
#   2. .github/workflows/emit-stale-merge-metric.yml exists with a 5-min cron
#   3. infra/cloudwatch/alarms.json contains amos-bounty-settled-unmerged-backlog
#   4. infra/cloudwatch/dashboard.json contains a widget for BountySettledUnmerged
#   5. The alarm exists in CloudWatch (deploy was actually run)
#   6. The custom metric has at least one datapoint (emitter has fired at least once)

set -euo pipefail

REGION="${AWS_REGION:-us-east-1}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ALARM_NAME="amos-bounty-settled-unmerged-backlog"

fail() { echo "FAIL: $*" >&2; exit 1; }
log() { echo "[verify-stale-merge $(date -u +%H:%M:%S)] $*"; }

# ── Check 1: emitter script ───────────────────────────────────────────
EMITTER="$ROOT/scripts/emit-stale-merge-metric.sh"
[ -f "$EMITTER" ] || fail "missing $EMITTER"
[ -x "$EMITTER" ] || fail "$EMITTER is not executable"
log "✓ emitter script present + executable"

# ── Check 2: GHA workflow with 5-min cron ─────────────────────────────
WF="$ROOT/.github/workflows/emit-stale-merge-metric.yml"
[ -f "$WF" ] || fail "missing $WF"
grep -qE "cron:[[:space:]]*['\"]\\*/5 \\* \\* \\* \\*['\"]" "$WF" \
    || fail "$WF: 5-min cron schedule not found (expected '*/5 * * * *')"
log "✓ workflow file present with 5-min cron"

# ── Check 3: alarms.json entry ────────────────────────────────────────
ALARMS_JSON="$ROOT/infra/cloudwatch/alarms.json"
python3 -c "
import json, sys
with open('$ALARMS_JSON') as f:
    data = json.load(f)
names = [a['AlarmName'] for a in data]
if '$ALARM_NAME' not in names:
    print('alarms.json missing entry: $ALARM_NAME', file=sys.stderr)
    sys.exit(1)
entry = next(a for a in data if a['AlarmName'] == '$ALARM_NAME')
if entry.get('MetricName') != 'BountySettledUnmerged':
    print(f'alarms.json $ALARM_NAME has wrong MetricName: {entry.get(\"MetricName\")}', file=sys.stderr)
    sys.exit(1)
if entry.get('Namespace') != 'AMOS/Relay':
    print(f'alarms.json $ALARM_NAME has wrong Namespace: {entry.get(\"Namespace\")}', file=sys.stderr)
    sys.exit(1)
" || fail "alarms.json validation failed"
log "✓ alarms.json contains $ALARM_NAME with correct metric/namespace"

# ── Check 4: dashboard widget ─────────────────────────────────────────
DASHBOARD_JSON="$ROOT/infra/cloudwatch/dashboard.json"
grep -q '"BountySettledUnmerged"' "$DASHBOARD_JSON" \
    || fail "dashboard.json: no widget references BountySettledUnmerged"
log "✓ dashboard.json includes BountySettledUnmerged widget"

# ── Check 5: alarm deployed to CloudWatch ─────────────────────────────
DEPLOYED=$(aws cloudwatch describe-alarms --region "$REGION" \
    --alarm-names "$ALARM_NAME" \
    --query 'MetricAlarms[0].AlarmName' --output text 2>/dev/null || echo "")
if [ "$DEPLOYED" != "$ALARM_NAME" ]; then
    fail "alarm '$ALARM_NAME' not found in CloudWatch in $REGION (deploy script not run yet?)"
fi
log "✓ alarm '$ALARM_NAME' is deployed to CloudWatch"

# ── Check 6: metric has at least one datapoint ────────────────────────
# Look back 1 hour for any data on the metric.
DATAPOINTS=$(aws cloudwatch get-metric-statistics --region "$REGION" \
    --namespace AMOS/Relay \
    --metric-name BountySettledUnmerged \
    --start-time "$(date -u -v -1H +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -d '1 hour ago' +%Y-%m-%dT%H:%M:%SZ)" \
    --end-time "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --period 300 \
    --statistics Maximum \
    --query 'length(Datapoints)' --output text 2>/dev/null || echo "0")

if [ "$DATAPOINTS" = "0" ] || [ -z "$DATAPOINTS" ]; then
    fail "no AMOS/Relay/BountySettledUnmerged datapoints in the last hour — emitter has not fired yet (run scripts/emit-stale-merge-metric.sh once locally, or trigger the workflow_dispatch)"
fi
log "✓ metric has $DATAPOINTS datapoint(s) in the last hour"

echo
log "All checks passed: settled-but-unmerged observability is wired and emitting."
exit 0
