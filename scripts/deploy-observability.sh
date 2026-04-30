#!/usr/bin/env bash
#
# Deploy AMOS protocol observability to CloudWatch.
#
# Idempotent: re-running updates existing resources, doesn't duplicate.
# All resources prefixed `amos-*` for safe scoped teardown.
#
# Resources created:
#   1. Log metric filters extracting custom metrics from /ecs/amos-relay
#      and /ecs/amos-oracle log groups (AMOS/Relay + AMOS/Oracle namespaces)
#   2. CloudWatch dashboard `amos-protocol-health`
#   3. CloudWatch alarms (no SNS yet — wire actions: amos-alerts topic later)
#
# Usage:
#   ./scripts/deploy-observability.sh                # deploy/update
#   ./scripts/deploy-observability.sh --dry-run      # show actions only
#   ./scripts/deploy-observability.sh --teardown     # remove everything

set -euo pipefail

REGION="${AWS_REGION:-us-east-1}"
DASHBOARD_NAME="amos-protocol-health"
INFRA_DIR="$(cd "$(dirname "$0")/.." && pwd)/infra/cloudwatch"

DRY_RUN=false
TEARDOWN=false
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=true ;;
        --teardown) TEARDOWN=true ;;
        *) echo "unknown arg: $arg" >&2; exit 2 ;;
    esac
done

run() {
    if $DRY_RUN; then
        echo "[dry-run] $*"
    else
        "$@"
    fi
}

# ── Teardown path ─────────────────────────────────────────────────────────
if $TEARDOWN; then
    echo "Tearing down AMOS observability resources..."

    # Alarms
    ALARM_NAMES=$(python3 -c "
import json
with open('$INFRA_DIR/alarms.json') as f:
    print(' '.join(a['AlarmName'] for a in json.load(f)))
")
    if [ -n "$ALARM_NAMES" ]; then
        run aws cloudwatch delete-alarms --region "$REGION" --alarm-names $ALARM_NAMES
    fi

    # Dashboard
    run aws cloudwatch delete-dashboards --region "$REGION" --dashboard-names "$DASHBOARD_NAME" 2>/dev/null || true

    # Metric filters
    python3 -c "
import json
with open('$INFRA_DIR/metric-filters.json') as f:
    for f_ in json.load(f):
        print(f\"{f_['logGroupName']}|{f_['filterName']}\")
" | while IFS='|' read -r LG FN; do
        run aws logs delete-metric-filter --region "$REGION" --log-group-name "$LG" --filter-name "$FN" 2>/dev/null || true
    done

    echo "Teardown complete."
    exit 0
fi

# ── Deploy path ───────────────────────────────────────────────────────────
echo "Deploying AMOS observability to $REGION..."

# 1. Log metric filters
echo
echo "── Log metric filters ────────────────────────────────────────"
python3 -c "
import json
with open('$INFRA_DIR/metric-filters.json') as f:
    for f_ in json.load(f):
        mt = f_['metricTransformations'][0]
        print('|'.join([
            f_['logGroupName'],
            f_['filterName'],
            f_['filterPattern'],
            mt['metricName'],
            mt['metricNamespace'],
            mt['metricValue'],
            str(mt.get('defaultValue', 0)),
        ]))
" | while IFS='|' read -r LG FN PATTERN MN MNS MV DV; do
    echo "  put: $FN ($LG) → $MNS/$MN"
    run aws logs put-metric-filter \
        --region "$REGION" \
        --log-group-name "$LG" \
        --filter-name "$FN" \
        --filter-pattern "$PATTERN" \
        --metric-transformations \
            "metricName=$MN,metricNamespace=$MNS,metricValue=$MV,defaultValue=$DV"
done

# 2. Dashboard
echo
echo "── Dashboard ────────────────────────────────────────────────"
echo "  put: $DASHBOARD_NAME"
run aws cloudwatch put-dashboard \
    --region "$REGION" \
    --dashboard-name "$DASHBOARD_NAME" \
    --dashboard-body "file://$INFRA_DIR/dashboard.json" \
    --output text > /dev/null

# 3. Alarms
echo
echo "── Alarms ───────────────────────────────────────────────────"
python3 -c "
import json
with open('$INFRA_DIR/alarms.json') as f:
    for a in json.load(f):
        print(json.dumps(a))
" | while IFS= read -r ALARM_JSON; do
    NAME=$(echo "$ALARM_JSON" | python3 -c "import sys,json;print(json.load(sys.stdin)['AlarmName'])")
    echo "  put: $NAME"
    if ! $DRY_RUN; then
        ARGS=$(echo "$ALARM_JSON" | python3 -c "
import sys, json, shlex
a = json.load(sys.stdin)
parts = []
parts.extend(['--alarm-name', a['AlarmName']])
parts.extend(['--alarm-description', a['AlarmDescription']])
parts.extend(['--metric-name', a['MetricName']])
parts.extend(['--namespace', a['Namespace']])
parts.extend(['--statistic', a['Statistic']])
parts.extend(['--period', str(a['Period'])])
parts.extend(['--evaluation-periods', str(a['EvaluationPeriods'])])
parts.extend(['--threshold', str(a['Threshold'])])
parts.extend(['--comparison-operator', a['ComparisonOperator']])
parts.extend(['--treat-missing-data', a['TreatMissingData']])
print(' '.join(shlex.quote(p) for p in parts))
")
        eval aws cloudwatch put-metric-alarm --region "$REGION" $ARGS
    fi
done

echo
echo "✓ Deployed."
echo
DASHBOARD_URL="https://${REGION}.console.aws.amazon.com/cloudwatch/home?region=${REGION}#dashboards:name=${DASHBOARD_NAME}"
echo "Dashboard: $DASHBOARD_URL"
echo "Alarms:    https://${REGION}.console.aws.amazon.com/cloudwatch/home?region=${REGION}#alarmsV2:?search=amos-"
echo
echo "Note: alarms have no notification action wired yet (no SNS topic). Add"
echo "      --alarm-actions <SNS topic ARN> to put-metric-alarm calls when ready."
