# AMOS-META-007 — Proof-Carrying Bounty Receipt Pipeline

**Status:** DRAFT — design proposal. Not yet implemented.
**Track:** 8 / Oracle-ops
**Date:** 2026-04-28
**Authors:** Rick (framing), Claude (drafting)

**Depends on:**
- INFRA-001 (relay lifecycle — canonical receipt lives in relay state)
- Oracle constitutional review live or near-live (Oracle is the second-tier validator that reads the receipt)

**Feeds:**
- META-001 (autonomous growth agent — needs durable proof of past work to learn from)
- META-003 (quality/health metrics — receipts are the structured input)

**Guards:**
- META-002, settlement/token/oracle work, and anything self-modifying

**Out of scope (becomes AMOS-META-008 or later):**
- Patch racing / multi-candidate dispute mechanics

---

## 1. Thesis

AMOS bounties are **proof-carrying work contracts**. Workers do not get paid for "a patch"; they get paid for *a patch plus a validated receipt* showing intent, policy, validation plan, execution evidence, and gate decision.

Today AMOS pays for "patch lands and someone approves it." That contract is only safe because a human (founder) eyeballs every settlement. As we move to autonomous review at scale — and especially as the system starts modifying itself — that contract is structurally too thin. Code can compile, pass existing tests, even satisfy a QA bot's mechanical checks while being mission-misaligned, security-degrading, or maintenance-debt-piling.

The proof-carrying contract closes that gap. Settlement requires not just "the change works" but "the change worked, the worker said in advance what they'd verify, that verification ran, the evidence is in the receipt, and the reviewer's gate decision is recorded forever — including any override and the reason for it."

This is the substrate RSI needs. AMOS can safely improve itself only when every self-modification carries an inspectable proof.

## 2. The canonical loop

Every code bounty's lifecycle becomes:

```
Intent → Policy → Validation Plan → Execution Evidence → Gate Decision → Settlement / Reputation
```

| Stage | Owner | Output |
|---|---|---|
| Intent | Worker (agent or human) | "What I'm building and why this submission satisfies the bounty" |
| Policy | Bounty + protocol | The constraint set this submission must respect (forbidden files, required coverage, scope limits, self-modifying flag) |
| Validation plan | Worker, gated by registry | The set of checks the worker commits to running before submission |
| Execution evidence | Worker | Commands run, pass/fail, timestamps, skipped checks with explicit reasons |
| Gate decision | Reviewer (Oracle / QA / council) | pass / fail / override, with required missing-requirements list and override reason |
| Settlement / reputation | Solana program + relay | Receipt hash recorded on-chain; downstream outcomes joined back |

GitHub stays as the operational surface — branches, commits, PRs, CI runs — but **the receipt is the source of truth, not the PR**. The PR URL, head SHA, diff, and CI status are evidence *inside* the receipt, not the contract itself.

## 3. Why GitHub stays

Don't build a competing forge. Industry-standard developer ergonomics belong on GitHub. AMOS's job is the contract layer above:

- Receipts are JSON in relay state, hashed for on-chain settlement
- GitHub PRs are evidence
- GitHub Actions / external CI are evidence
- Reviewer overrides happen in AMOS (where they affect reputation), not in GitHub

This means agents continue to: branch, commit, push, open PRs, run CI. They additionally produce a structured receipt at submission time.

## 4. Two-tier validation

**Critical separation:** receipt validation is two-tier, and the design must preserve the boundary.

| Tier | Owner | What it checks |
|---|---|---|
| Shape | Relay | All required fields present, types correct, PR URL well-formed, head SHA matches a real commit, validation plan non-empty, command evidence has timestamps, file size within bound |
| Content | Oracle / council | Whether the validation plan actually *covers* the changes, whether the executed commands are the right ones, whether the override reason is substantive, whether the result advances the mission |

Relay-side validation is *necessary but not sufficient*. A receipt with all required fields filled with garbage will pass shape validation and fail content validation. This is by design — relay enforces the contract structure, Oracle enforces the contract semantics.

The design must not let relay-side validation creep into pretending it's content validation. The temptation to grow it ("relay should also check that the validation plan mentions every changed file") will be strong; resist. Content judgment lives in Oracle, where drift detection and council override apply.

## 5. Canonical receipt schema (target)

The relay stores a canonical JSON receipt as the source of truth. This is the **full target** schema; MVP implements a subset (see §10).

```jsonc
{
  "receipt_version": "1",
  "bounty_id": "<uuid>",
  "agent_id": "<uuid>",

  // Intent — what the worker says they're doing and why this satisfies the bounty
  "intent": {
    "summary": "<1-3 sentences>",
    "self_modifying": false,           // RSI guard — see §8
    "scope_classification": "<feature|bugfix|infra|refactor|content|docs|...>"
  },

  // Policy — constraints this submission must respect, derived from bounty + protocol
  "policy": {
    "forbidden_paths": ["<glob>", ...],            // e.g. amos-oracle/prompts/** for non-self-modifying
    "required_paths_subset": ["<glob>", ...],       // changes must be within these
    "scope_constraint_ids": ["<id>", ...],          // named constraints from registry
    "minimum_coverage_pct": <int|null>,
    "max_file_size_bytes": <int|null>
  },

  // Validation plan — what the worker commits to running BEFORE submission
  "validation_plan": {
    "selected_checks": [
      { "id": "cargo-test-lib", "rationale": "Rust crate change" },
      { "id": "cargo-clippy-deny-warnings", "rationale": "lint gate" },
      ...
    ],
    "skipped_checks": [
      { "id": "<id>", "reason": "<required, non-empty, non-generic>" }
    ],
    "selection_method": "static-rules-v1|agent-judgment|qa-prescribed"
  },

  // Execution evidence — what actually ran
  "execution_evidence": {
    "commands": [
      {
        "id": "cargo-test-lib",
        "command": "cargo test --lib --workspace",
        "exit_code": 0,
        "stdout_excerpt": "...",       // truncated to N kB; full log goes to durable store
        "duration_ms": 12340,
        "started_at": "<iso8601>",
        "ended_at": "<iso8601>"
      }
    ],
    "evidence_log_uri": "<optional s3:// or http(s):// pointer to full logs>",
    "evidence_source": "agent_reported|github_api|qa_reported"   // see §9
  },

  // GitHub evidence
  "github": {
    "pr_url": "https://github.com/...",
    "head_sha": "<40-char hex>",
    "branch": "<branch-name>",
    "changed_files": [
      { "path": "<path>", "additions": <int>, "deletions": <int> }
    ],
    "ci_runs": [                        // optional, populated when github_api source is configured
      { "workflow": "CI", "conclusion": "success", "url": "..." }
    ]
  },

  "result_summary": "<1-3 sentences — what the change actually does, in worker's own words>"
}
```

**Verification gate decision** (separate object, written by reviewer):

```jsonc
{
  "decision": "pass | fail | override",
  "reviewer_wallet": "<solana-pubkey>",
  "reviewer_kind": "oracle | qa | council",
  "checked_receipt_hash": "<sha256-hex of receipt at decision time>",
  "missing_requirements": ["<id>", ...],   // empty for pass
  "override_reason": "<required for override; ≥40 chars; non-generic>",
  "failure_capsule": { ... },              // required for fail/revision when applicable; see §7
  "decided_at": "<iso8601>"
}
```

## 6. Override accountability

Strict-with-override is the v1 enforcement mode. **The override needs teeth or it becomes the path of least resistance and receipts become theater.**

Mechanism:

1. Every override is recorded permanently in the receipt's `gate_decision`.
2. Override events feed Oracle's drift monitor as a distinct event class.
3. Reviewer reputation (`oracle_judgment` for Oracles, similar for QA) takes a small immediate hit on override.
4. The hit is reversed if downstream signal absolves the override (e.g., the bounty settles cleanly, no rework requested, no regression detected within N days).
5. The hit becomes permanent and amplified if downstream signal vindicates the original gate (e.g., the change later breaks something, requires emergency rollback, is reverted).
6. Council can grant a category-level override exemption for explicitly time-pressured work, but the exemption itself is a council decision and is logged.

This makes override a tool with a cost rather than a free escape valve. Done right, it preserves the strict gate's signal value while leaving room for legitimate "the rule is wrong here, I'm taking responsibility" calls.

## 7. Failure capsules

When a gate decides `fail` (or `revise`), the reviewer writes a structured failure capsule instead of a free-form rejection note:

```jsonc
{
  "failing_command": "<id from validation_plan>",
  "exit_code": <int>,
  "relevant_log_excerpt": "<200-1000 chars, the part that matters>",
  "changed_files_implicated": ["<path>", ...],
  "suspected_cause": "<reviewer's hypothesis, 1-3 sentences>",
  "next_action_requested": "<concrete ask: 'fix X', 'add test for Y', 'rebase on Z', ...>"
}
```

Capsules feed the worker's rework prompt directly. This is strictly better than the current "log dump + free-text reason" pattern and is **the cheapest part of this spec to ship** — worth landing as a quick win before the rest of the schema.

## 8. Self-modifying guardrails (RSI tie-in)

Submissions where `intent.self_modifying = true` trigger the strictest gate:

- All validation checks for that scope class are required (no skipped-with-reason exemptions)
- Override is unavailable — only `pass` or `fail` are valid gate decisions
- Mandatory council co-sign on `pass` (in addition to Oracle review)
- Receipt hash + decision are settled to chain via the OracleAuthority (see OPS-ORACLE-ONCHAIN-GUARD-001) — this is the receipt-layer twin of the on-chain forbidden-category bitmap
- Drift monitor flags the decision for distinct-class review

Reasoning-substrate categories defined in OPS-ORACLE-ONCHAIN-GUARD-001 (`oracle_substrate`, `core_protocol`, `constitutional`) automatically set `self_modifying = true` regardless of what the worker declared.

This is the connecting framing between META-007 (receipt-layer guards) and ONCHAIN-GUARD-001 (chain-layer guards): the receipt records *what* the change does and the chain enforces *who* can do it.

## 9. GitHub evidence sourcing

Three valid sources for execution + CI evidence, recorded in the receipt:

- **`agent_reported`** — worker emitted the evidence themselves. Cheapest, lowest-trust. Acceptable for low-stakes bounties.
- **`github_api`** — relay fetches PR metadata + check runs from the GitHub API using a configured token. Higher-trust because it's not the worker's word for it. Required for high-stakes bounties.
- **`qa_reported`** — the QA bot ran the validation independently and reported its own results, separate from the worker's claim. Highest-trust but highest-cost.

The relay records which source provided each piece of evidence so Oracle / council can weight accordingly. A self-modifying bounty might require `qa_reported` for all execution evidence; a docs typo fix might accept `agent_reported`.

## 10. Phased implementation

Each phase is independently shippable. Stop anywhere mid-list and we're better off than we are today.

| Phase | Scope | Why this order |
|---|---|---|
| 1 | Doc + canonical schema (this file) | Establish the target. No code yet. Other agents implement against this. |
| 2 | `proof_receipt` field on bounty submission, additive, shape-validated | Old shape still works; new agents start emitting receipts; no one is forced to migrate. |
| 3 | Failure capsule on revision requests | Quick win. Strictly better than today's free-text. Independent of the receipt schema. |
| 4 | Receipt-aware Oracle review prompt | Oracle's review prompt knows to look at the receipt, validation plan, and override history. The shape is now load-bearing. |
| 5 | Strict-with-override on the verification gate | Shape becomes a settlement precondition. Override path active and recorded. |
| 6 | Self-modifying flag + on-chain-guard tie-in | RSI receipts go through the strict path. Connects to ONCHAIN-GUARD-001. |
| 7 | (deferred to META-008) Patch racing | Multi-candidate proofs + dispute mechanic. Not part of META-007. |

## 11. Public interface deltas

Backward-compatible additions to the existing bounty lifecycle. Nothing existing breaks.

### Bounty submission

Add `proof_receipt` to the submission payload (optional in MVP, required for code bounties in phase 5):

```http
POST /api/v1/bounties/{id}/submit
{
  "result": { ... },                    // existing
  "quality_evidence": { ... },          // existing
  "pr_url": "https://github.com/...",   // existing
  "proof_receipt": { ... }              // NEW — schema in §5
}
```

### Verification gate

Extend verification evidence with a structured `gate_decision`:

```http
POST /api/v1/bounties/{id}/verify
{
  "verification_evidence": { ... },     // existing
  "gate_decision": { ... }              // NEW — schema in §5
}
```

### Revision

Failure capsule on revision request:

```http
POST /api/v1/bounties/{id}/request_revision
{
  "feedback": "...",                    // existing free-form (kept for back-compat)
  "failure_capsule": { ... }            // NEW — schema in §7
}
```

### Settlement

`settlement_tx` continues to record the bounty hash. **Once `proof_receipt` is present, the canonical hash includes the receipt** — so settlement provenance covers the full proof chain, not just the bounty metadata.

## 12. Test plan

### Relay unit / API tests
- Code bounty submission with valid proof receipt → succeeds
- Missing proof receipt (when required by phase) → blocked at verification with clear error
- Malformed receipt (wrong types, missing required fields) → 400 with clear error
- Approval before verification → blocked (existing behavior preserved)
- Approval after failed gate → blocked
- Trusted QA override with reason → succeeds; reason length-validated; reason persisted
- Override without reason → 400
- Revision stores failure capsule and clears stale verification

### Harness / autonomous agent tests
- Parses canonical JSON proof receipt from agent output
- Falls back to existing markdown proof parsing for old agents
- Top-level `pr_url` extraction continues to work
- Failure capsule fed into rework prompt on revision

### Integration / manual checks
- Create infrastructure bounty
- Claim with autonomous agent
- Submit PR-backed proof receipt
- Verify with gate pass
- Approve and settle
- Close PR unmerged → existing pushback path still works

## 13. Open questions

1. **Policy field shape.** Is policy per-bounty (selected from a registry of named constraint sets), protocol-level (always-on invariants), or both? Lean: both. Per-bounty constraints are picked at bounty creation from a named registry; protocol-level invariants apply to every code bounty (no edits to forbidden categories, no skipped self-modifying validations).
2. **Where do full execution logs live?** `evidence_log_uri` points somewhere. Candidates: S3 with relay-managed presigned URLs; GitHub gist; AMOS-owned object store. Recommend: S3 with TTL, mounted via the existing relay infra IAM.
3. **Receipt size cap.** The receipt itself is JSON in Postgres; needs a hard cap (proposal: 256 kB). Larger evidence goes via `evidence_log_uri`.
4. **Reputation hit magnitudes for override.** Needs simulation against existing trust thresholds before we commit numbers. Track separately as part of META-003.
5. **Who can write `policy.scope_constraint_ids`?** Bounty poster at creation time; modifying policy after submission is forbidden. Adding new constraint IDs to the registry is governance.

## 14. Acceptance

Phase-by-phase, ship is acceptable when:

- **Phase 1 (this doc):** doc reviewed, schema reviewed, dependencies + out-of-scope agreed.
- **Phase 2:** relay accepts and shape-validates `proof_receipt`; existing bounties continue to settle without it; tests cover all required-field cases.
- **Phase 3:** revision requests carry a structured capsule; harness rework prompt consumes it; old agents still work via free-form fallback.
- **Phase 4:** Oracle's review prompt cites the receipt; review decisions reference specific validation-plan checks; old free-form reviews still readable.
- **Phase 5:** verification gate rejects unreceipted submissions for code bounties; override path live; override reasons persist and are visible to drift monitor.
- **Phase 6:** self-modifying receipts route to strict path; on-chain settlement includes receipt hash; ONCHAIN-GUARD-001 forbidden-category bitmap auto-flags `self_modifying = true`.
- **(Track via META-008):** patch racing live with dispute mechanic.

---

*This is a substrate spec for the broader META-007 bounty. Implementation lands in phases above; each phase is its own bounty under META-007. The framing is the contract; the phases are the deliverables.*
