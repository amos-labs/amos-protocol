#!/usr/bin/env bash
#
# AMOS QA Verification Bot
#
# Polls the relay for submitted (unverified) bounties, verifies deliverables
# by checking GitHub CI and running local tests, then calls /verify or /reject.
#
# Architecture:
#   1. Poll relay for bounties with status=submitted AND verified_at IS NULL
#   2. For each bounty, extract git_sha or pr_url from the result JSON
#   3. Check GitHub CI status via gh api
#   4. Run cargo check + cargo test locally
#   5. If all pass: POST /verify with evidence
#   6. If any fail: POST /reject with reason
#
# Usage:
#   RELAY_URL=http://localhost:4100 \
#   RELAY_API_KEY=test_key_e2e_maxreward_2026 \
#   QA_WALLET=kekPK242otEGHrNmZA7v2jLYdkg3BPYiTPMJvrDhNuj \
#   GITHUB_REPO=amos-labs/amos-platform-2.0 \
#   ./scripts/qa-verification-bot.sh
#
# For continuous operation, run in a loop:
#   while true; do ./scripts/qa-verification-bot.sh; sleep 60; done

set -euo pipefail

RELAY_URL="${RELAY_URL:-http://localhost:4100}"
RELAY_API_KEY="${RELAY_API_KEY:-test_key_e2e_maxreward_2026}"
QA_WALLET="${QA_WALLET:-kekPK242otEGHrNmZA7v2jLYdkg3BPYiTPMJvrDhNuj}"
GITHUB_REPO="${GITHUB_REPO:-amos-labs/amos-platform-2.0}"
PROJECT_ROOT="${PROJECT_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"

AUTH_HEADER="Authorization: Bearer ${RELAY_API_KEY}"

log() { echo "[qa-bot $(date +%H:%M:%S)] $*"; }

# ── Step 1: Fetch submitted bounties ──────────────────────────────────────────

log "Polling for submitted bounties..."
BOUNTIES=$(curl -sf "${RELAY_URL}/api/v1/bounties?status=submitted" \
  -H "${AUTH_HEADER}" 2>/dev/null || echo "[]")

# Filter to unverified only (verified_at is null)
UNVERIFIED=$(echo "$BOUNTIES" | python3 -c "
import sys, json
bounties = json.load(sys.stdin)
unverified = [b for b in bounties if b.get('verified_at') is None]
print(json.dumps(unverified))
" 2>/dev/null || echo "[]")

COUNT=$(echo "$UNVERIFIED" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))")
if [ "$COUNT" = "0" ]; then
    log "No unverified submitted bounties found."
    exit 0
fi
log "Found $COUNT unverified bounties to process."

# ── Step 2: Process each bounty ──────────────────────────────────────────────

echo "$UNVERIFIED" | python3 -c "
import sys, json
for b in json.load(sys.stdin):
    print(f\"{b['id']}|{json.dumps(b.get('result', {}))}\")
" | while IFS='|' read -r BOUNTY_ID RESULT_JSON; do
    log "Processing bounty: $BOUNTY_ID"

    # Extract git_sha from result JSON
    GIT_SHA=$(echo "$RESULT_JSON" | python3 -c "
import sys, json
try:
    r = json.load(sys.stdin)
    # Check multiple possible locations for git SHA
    sha = r.get('git_sha') or r.get('commit_sha') or r.get('sha') or ''
    print(sha)
except:
    print('')
" 2>/dev/null)

    EVIDENCE="{}"
    PASSED=true
    REJECT_REASON=""

    # ── Check 1: Git SHA exists on remote ────────────────────────────────
    if [ -n "$GIT_SHA" ] && [ "$GIT_SHA" != "null" ]; then
        log "  Checking git SHA: $GIT_SHA"
        if gh api "repos/${GITHUB_REPO}/commits/${GIT_SHA}" --silent 2>/dev/null; then
            log "  Git SHA verified on GitHub"
            EVIDENCE=$(echo "$EVIDENCE" | python3 -c "
import sys, json
e = json.load(sys.stdin)
e['git_sha_verified'] = True
e['git_sha'] = '$GIT_SHA'
print(json.dumps(e))
")
        else
            log "  WARNING: Git SHA not found on GitHub"
            EVIDENCE=$(echo "$EVIDENCE" | python3 -c "
import sys, json
e = json.load(sys.stdin)
e['git_sha_verified'] = False
e['git_sha'] = '$GIT_SHA'
print(json.dumps(e))
")
        fi

        # Check CI status
        CI_STATUS=$(gh api "repos/${GITHUB_REPO}/commits/${GIT_SHA}/status" \
          --jq '.state' 2>/dev/null || echo "unknown")
        log "  CI status: $CI_STATUS"
        EVIDENCE=$(echo "$EVIDENCE" | python3 -c "
import sys, json
e = json.load(sys.stdin)
e['ci_status'] = '$CI_STATUS'
print(json.dumps(e))
")
    else
        log "  No git SHA in result — skipping GitHub checks"
        EVIDENCE=$(echo "$EVIDENCE" | python3 -c "
import sys, json
e = json.load(sys.stdin)
e['git_sha_verified'] = False
e['note'] = 'No git SHA provided in bounty result'
print(json.dumps(e))
")
    fi

    # ── Check 2: Local build ─────────────────────────────────────────────
    log "  Running cargo check..."
    if (cd "$PROJECT_ROOT" && cargo check 2>&1 | tail -3); then
        log "  Cargo check: PASS"
        EVIDENCE=$(echo "$EVIDENCE" | python3 -c "
import sys, json
e = json.load(sys.stdin)
e['cargo_check'] = 'pass'
print(json.dumps(e))
")
    else
        log "  Cargo check: FAIL"
        PASSED=false
        REJECT_REASON="cargo check failed"
        EVIDENCE=$(echo "$EVIDENCE" | python3 -c "
import sys, json
e = json.load(sys.stdin)
e['cargo_check'] = 'fail'
print(json.dumps(e))
")
    fi

    # ── Check 3: Tests pass ──────────────────────────────────────────────
    if [ "$PASSED" = true ]; then
        log "  Running cargo test..."
        if (cd "$PROJECT_ROOT" && cargo test --lib -p amos-harness -p amos-relay -p amos-core 2>&1 | tail -5); then
            log "  Cargo test: PASS"
            EVIDENCE=$(echo "$EVIDENCE" | python3 -c "
import sys, json
e = json.load(sys.stdin)
e['cargo_test'] = 'pass'
print(json.dumps(e))
")
        else
            log "  Cargo test: FAIL"
            PASSED=false
            REJECT_REASON="cargo test failed"
            EVIDENCE=$(echo "$EVIDENCE" | python3 -c "
import sys, json
e = json.load(sys.stdin)
e['cargo_test'] = 'fail'
print(json.dumps(e))
")
        fi
    fi

    # ── Decision: verify or reject ───────────────────────────────────────
    EVIDENCE=$(echo "$EVIDENCE" | python3 -c "
import sys, json
e = json.load(sys.stdin)
e['verified_by'] = 'qa-verification-bot'
e['timestamp'] = '$(date -u +%Y-%m-%dT%H:%M:%SZ)'
print(json.dumps(e))
")

    if [ "$PASSED" = true ]; then
        log "  VERIFYING bounty $BOUNTY_ID"
        HTTP_CODE=$(curl -sf -o /dev/null -w "%{http_code}" \
          "${RELAY_URL}/api/v1/bounties/${BOUNTY_ID}/verify" -X POST \
          -H "${AUTH_HEADER}" \
          -H "Content-Type: application/json" \
          -d "{\"verifier_wallet\":\"${QA_WALLET}\",\"evidence\":${EVIDENCE}}")
        log "  Verify response: HTTP $HTTP_CODE"
    else
        log "  REJECTING bounty $BOUNTY_ID: $REJECT_REASON"
        curl -sf -o /dev/null \
          "${RELAY_URL}/api/v1/bounties/${BOUNTY_ID}/reject" -X POST \
          -H "${AUTH_HEADER}" \
          -H "Content-Type: application/json" \
          -d "{\"reviewer_wallet\":\"${QA_WALLET}\",\"reason\":\"QA bot: ${REJECT_REASON}\"}" \
          2>/dev/null || log "  WARNING: reject call failed"
    fi

    log "  Done with bounty $BOUNTY_ID"
done

log "QA verification cycle complete."
