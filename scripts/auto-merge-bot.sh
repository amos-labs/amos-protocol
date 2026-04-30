#!/usr/bin/env bash
#
# AMOS Auto-Merge Bot — OPS-AUTOMERGE-001.
#
# Closes the META-007 phase 6 gap (and the prior settled≠merged class of bug
# documented in feedback_settled_neq_merged.md). Once a bounty is approved
# and on-chain settled, this bot lands the worker's PR onto main, so
# "settled" becomes a structural guarantee that the code is integrated.
#
# Architecture:
#   1. Poll relay for bounties with status=approved, settlement_status=settled,
#      merge_commit_sha IS NULL.
#   2. Extract proof_receipt.github.pr_url + head_sha.
#   3. Verify the PR's current head SHA matches the receipt SHA — refuses
#      stale receipts (worker pushed new commits after Oracle reviewed).
#   4. Verify CI is green and not pending.
#   5. gh pr merge --squash --delete-branch.
#   6. POST /api/v1/bounties/{id}/record-merge with the merge commit SHA.
#
# Bounties without a proof_receipt (council-override approvals) are skipped:
# the bot doesn't know which PR to merge, and a human already had to override
# the proof gate to approve, so they can land the PR manually.

set -euo pipefail

RELAY_URL="${RELAY_URL:-http://localhost:4100}"
RELAY_API_KEY="${RELAY_API_KEY:?RELAY_API_KEY is required}"
GITHUB_REPO="${GITHUB_REPO:-amos-labs/amos-platform-2.0}"
BOT_NAME="${BOT_NAME:-auto-merge-bot}"

AUTH_HEADER="Authorization: Bearer ${RELAY_API_KEY}"

log() { echo "[automerge $(date -u +%H:%M:%S)] $*"; }

# POST the merge SHA back to the relay. Idempotent on the relay side, so
# replays are safe if the workflow re-runs after a partial failure.
record_merge_sha() {
  local bounty_id="$1"
  local merge_sha="$2"
  local code
  code=$(curl -s -o /tmp/automerge_resp.json -w "%{http_code}" \
    -X POST "${RELAY_URL}/api/v1/bounties/${bounty_id}/record-merge" \
    -H "${AUTH_HEADER}" \
    -H "Content-Type: application/json" \
    -d "{\"merge_commit_sha\":\"${merge_sha}\",\"merged_by\":\"${BOT_NAME}\"}")
  if [ "$code" = "200" ]; then
    log "   recorded merge SHA on relay (HTTP 200)"
  else
    log "   WARN: record-merge HTTP $code — body: $(cat /tmp/automerge_resp.json 2>/dev/null | head -c 200)"
  fi
}

# Bash exports functions to subshells (the while-loop pipeline runs in one)
# only via `export -f`. Without this the loop body cannot call the helper.
export -f record_merge_sha log
export RELAY_URL RELAY_API_KEY AUTH_HEADER BOT_NAME

# ── Step 1: Fetch approved bounties, filter to settled+unmerged with PR ─────
log "Polling relay for settled-but-unmerged bounties..."
APPROVED=$(curl -sf "${RELAY_URL}/api/v1/bounties?status=approved&limit=200" \
  -H "${AUTH_HEADER}" 2>/dev/null || echo "[]")

CANDIDATES=$(echo "$APPROVED" | python3 -c "
import sys, json
out = []
for b in json.load(sys.stdin):
    if b.get('settlement_status') != 'settled':
        continue
    if b.get('merge_commit_sha'):
        continue
    receipt = b.get('proof_receipt') or {}
    gh = receipt.get('github') or {}
    pr_url = gh.get('pr_url')
    head_sha = gh.get('head_sha')
    if not pr_url or not head_sha:
        continue
    out.append({'id': b['id'], 'pr_url': pr_url, 'head_sha': head_sha,
                'title': b.get('title','')})
print(json.dumps(out))
")

COUNT=$(echo "$CANDIDATES" | python3 -c "import sys,json;print(len(json.load(sys.stdin)))")
if [ "$COUNT" = "0" ]; then
  log "No settled-unmerged bounties with proof_receipt PR URLs."
  exit 0
fi
log "Found $COUNT candidate bounty/PR pairs."

# ── Step 2: Process each candidate ─────────────────────────────────────────
echo "$CANDIDATES" | python3 -c "
import sys, json
for c in json.load(sys.stdin):
    print(f\"{c['id']}|{c['pr_url']}|{c['head_sha']}|{c['title']}\")
" | while IFS='|' read -r BOUNTY_ID PR_URL RECEIPT_SHA TITLE; do
  log "→ ${BOUNTY_ID:0:8} | $TITLE"
  log "   PR: $PR_URL  receipt: ${RECEIPT_SHA:0:12}"

  # Bash regex (rather than `grep -oE | grep -oE` piped substitution, which
  # under `set -euo pipefail` exits the whole script on no-match and aborts
  # the rest of the queue). Some bounties carry a /commit/<sha> URL instead
  # of a /pull/<n> URL — those should be skipped gracefully so subsequent
  # bounties in the same cycle still process.
  PR_NUMBER=""
  if [[ "$PR_URL" =~ /pull/([0-9]+) ]]; then
    PR_NUMBER="${BASH_REMATCH[1]}"
  fi
  if [ -z "$PR_NUMBER" ]; then
    log "   SKIP: pr_url is not a /pull/<n> URL (probably /commit/<sha>) — bot has no PR to merge"
    continue
  fi

  PR_JSON=$(gh pr view "$PR_NUMBER" --repo "$GITHUB_REPO" \
    --json state,mergeable,headRefOid,statusCheckRollup,isDraft,mergeCommit 2>/dev/null || echo "")
  if [ -z "$PR_JSON" ]; then
    log "   SKIP: gh pr view failed (PR may not exist or auth issue)"
    continue
  fi

  STATE=$(echo "$PR_JSON" | python3 -c "import sys,json;print(json.load(sys.stdin).get('state',''))")
  IS_DRAFT=$(echo "$PR_JSON" | python3 -c "import sys,json;print(json.load(sys.stdin).get('isDraft',False))")
  CURRENT_SHA=$(echo "$PR_JSON" | python3 -c "import sys,json;print(json.load(sys.stdin).get('headRefOid','') or '')")
  EXISTING_MERGE_SHA=$(echo "$PR_JSON" | python3 -c "
import sys,json
d=json.load(sys.stdin); mc=d.get('mergeCommit') or {}
print(mc.get('oid','') or '')
")

  # Already merged on GitHub? Just record the SHA back to relay.
  if [ "$STATE" = "MERGED" ]; then
    if [ -n "$EXISTING_MERGE_SHA" ] && [ ${#EXISTING_MERGE_SHA} = 40 ]; then
      log "   PR already merged on GitHub — recording back to relay"
      record_merge_sha "$BOUNTY_ID" "$EXISTING_MERGE_SHA"
    else
      log "   WARN: PR merged but mergeCommit SHA not retrievable"
    fi
    continue
  fi

  if [ "$STATE" != "OPEN" ]; then
    log "   SKIP: PR state is $STATE"
    continue
  fi
  if [ "$IS_DRAFT" = "True" ]; then
    log "   SKIP: PR is draft"
    continue
  fi

  # ── Verify head SHA matches receipt ─────────────────────────────────
  if [ "$CURRENT_SHA" != "$RECEIPT_SHA" ]; then
    log "   SKIP: head ${CURRENT_SHA:0:12} != receipt ${RECEIPT_SHA:0:12} (worker pushed after Oracle review)"
    continue
  fi

  # ── Verify CI is green and complete ─────────────────────────────────
  CI_BAD=$(echo "$PR_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
checks = d.get('statusCheckRollup') or []
bad = [c for c in checks
       if c.get('status') == 'COMPLETED'
       and c.get('conclusion') not in ('SUCCESS','SKIPPED','NEUTRAL', None)]
print(len(bad))
")
  if [ "$CI_BAD" != "0" ]; then
    log "   SKIP: $CI_BAD failing/cancelled CI checks"
    continue
  fi
  CI_PENDING=$(echo "$PR_JSON" | python3 -c "
import sys, json
d = json.load(sys.stdin)
checks = d.get('statusCheckRollup') or []
pending = [c for c in checks if c.get('status') != 'COMPLETED']
print(len(pending))
")
  if [ "$CI_PENDING" != "0" ]; then
    log "   SKIP: $CI_PENDING CI checks still pending"
    continue
  fi

  # ── Merge ───────────────────────────────────────────────────────────
  log "   merging…"
  if ! gh pr merge "$PR_NUMBER" --repo "$GITHUB_REPO" --squash --delete-branch >/dev/null 2>&1; then
    log "   FAIL: gh pr merge errored"
    continue
  fi

  # GitHub takes a moment to surface the mergeCommit SHA via API. Brief poll.
  for i in 1 2 3 4 5; do
    sleep 2
    MERGE_SHA=$(gh pr view "$PR_NUMBER" --repo "$GITHUB_REPO" --json mergeCommit \
      --jq '.mergeCommit.oid // empty' 2>/dev/null || echo "")
    [ -n "$MERGE_SHA" ] && [ ${#MERGE_SHA} = 40 ] && break
  done

  if [ -z "${MERGE_SHA:-}" ] || [ ${#MERGE_SHA} != 40 ]; then
    log "   WARN: merged but mergeCommit SHA not retrievable yet — next tick will pick it up"
    continue
  fi

  log "   MERGED → $MERGE_SHA"
  record_merge_sha "$BOUNTY_ID" "$MERGE_SHA"
done

log "Auto-merge cycle complete."
