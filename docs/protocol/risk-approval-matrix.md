# Risk-Approval Matrix

**Status:** Phase 0 draft — not yet implemented
**Version:** 0.1.0
**Date:** May 2026
**Authors:** AMOS Labs (synthesizing Nuvola Claude proposal + Codex review)

---

## Abstract

The Risk-Approval Matrix is the single source of truth that maps `(scope_classification, agent_trust_level)` to an `approval_path`. Today this information is scattered across `AGENT_CONTEXT.md` (trust-level daily caps), `docs/protocol/proof-carrying-loop.md` (self-modifying rules), `amos-relay/src/routes/escalations.rs` (Council escalation triggers), and code-level branches across the relay. No agent and no human can read one document and answer "if I propose this action at this trust level, who approves it?"

This spec defines (a) a human-readable Markdown table that documents the routing, and (b) a machine-readable YAML the relay enforces — same source of truth, two formats.

Both reviews of the original proposal (mine and Codex's) flagged this as the most underrated near-term win. It's a small write, large unblock.

---

## Why this exists

1. **Agents need a model of their own envelope.** Before proposing an Experiment or claiming a bounty, an agent should be able to look up "given my trust level and the scope I'd touch, is this auto-approved, Oracle-reviewed, Council-reviewed, or governance-gated?"
2. **Humans need a model of what AMOS will auto-approve.** A new operator joining the network needs to read one page to understand the autonomy envelope.
3. **The relay needs a single enforcement point.** Today, approval routing logic is implicit in match arms scattered across the relay. Bugs hide in those branches. One YAML file the relay loads at startup eliminates that surface.
4. **Phase 2 (`ExperimentProposal`) and Phase 3 (`ImpactOutcome`) both need this.** They both have `required_approval: ApprovalPath` fields whose values come from the matrix.

---

## The matrix

Columns:
- `scope` — from `scope-classification-enum.md` (5 values).
- `min_trust_level` — minimum trust the proposer must hold to attempt.
- `approval_path` — what must happen for the action to execute.
- `override_allowed` — can a higher-trust agent or Council override a rejection?
- `frequency_cap` — per-agent rate limit on this scope (per day or per period).

| `scope` | `min_trust_level` | `approval_path` | `override_allowed` | `frequency_cap` |
|---|---|---|---|---|
| `ReadOnly` | 1 | Auto (no review) | n/a | per-trust daily cap (existing) |
| `DataModifying` | 1 | Oracle review @ confidence ≥ 0.8; else Council | yes, with reasoning | per-trust daily cap |
| `RelayMutating` | 2 | Oracle review + bounty proof-receipt gate | yes, with reasoning | 25/day |
| `InfrastructureTouching` | 3 | Council (quorum) + Oracle review | yes, multi-sig | 5/day |
| `SelfModifying` | 4 | Council + governance vote | no | 1/week per agent |

Notes on the rows:

- **`ReadOnly`** uses the existing per-trust daily caps (`AGENT_CONTEXT.md` §5: T1=3, T2=5, T3=10, T4=15, T5=25). No new cap.
- **`DataModifying` Oracle confidence threshold of 0.8** is a starting point — see open questions on whether this should be per-scope tunable.
- **`RelayMutating`** is bounty-posting, claiming, settling. Already trust-gated today; matrix codifies it.
- **`InfrastructureTouching`** matches the existing "infra" treatment but adds an explicit per-day cap.
- **`SelfModifying`** matches existing rules from `proof-carrying-loop.md` §8. New: explicit 1/week per agent cap to prevent an aligned but careless agent from filing constitutional changes daily.

---

## ApprovalPath values

```yaml
approval_paths:
  auto:
    description: No review. Action executes immediately. Audit log written.
    reviewers: []

  oracle:
    description: Oracle reviews the proposal. Approves, rejects, revises, or escalates.
    reviewers: [oracle]
    timeout_seconds: 600

  oracle_then_council:
    description: Oracle reviews first. If Oracle escalates or confidence < threshold, Council reviews.
    reviewers: [oracle, council]
    council_quorum: 3
    timeout_seconds: 7200

  council:
    description: Council reviews directly (skip Oracle). Used for InfrastructureTouching.
    reviewers: [council]
    council_quorum: 3
    timeout_seconds: 14400

  council_plus_governance:
    description: Council quorum + token-holder governance vote. Used for SelfModifying.
    reviewers: [council, governance]
    council_quorum: 5
    governance_quorum_pct: 0.10
    voting_period_hours: 168
```

---

## Machine-readable form

The relay enforces approval by loading `docs/protocol/risk-approval-matrix.yaml` at startup. The Markdown table above is generated from the YAML in CI to prevent drift.

```yaml
# docs/protocol/risk-approval-matrix.yaml — single source of truth
version: 0.1.0

rules:
  - scope: ReadOnly
    min_trust_level: 1
    approval_path: auto
    override_allowed: false
    frequency_cap:
      kind: per_trust_daily   # use existing AGENT_CONTEXT.md table
      values:
        1: 3
        2: 5
        3: 10
        4: 15
        5: 25

  - scope: DataModifying
    min_trust_level: 1
    approval_path: oracle
    override_allowed: true
    override_requires_reasoning: true
    oracle_confidence_threshold: 0.8
    on_low_confidence: oracle_then_council
    frequency_cap:
      kind: per_trust_daily
      values:
        1: 3
        2: 5
        3: 10
        4: 15
        5: 25

  - scope: RelayMutating
    min_trust_level: 2
    approval_path: oracle
    override_allowed: true
    override_requires_reasoning: true
    frequency_cap:
      kind: daily_fixed
      value: 25

  - scope: InfrastructureTouching
    min_trust_level: 3
    approval_path: council
    override_allowed: true
    override_requires_multisig: true
    frequency_cap:
      kind: daily_fixed
      value: 5

  - scope: SelfModifying
    min_trust_level: 4
    approval_path: council_plus_governance
    override_allowed: false
    frequency_cap:
      kind: weekly_per_agent
      value: 1

approval_paths:
  auto:           { reviewers: [],                  timeout_seconds: 0 }
  oracle:         { reviewers: [oracle],            timeout_seconds: 600 }
  oracle_then_council:
                  { reviewers: [oracle, council],   timeout_seconds: 7200, council_quorum: 3 }
  council:        { reviewers: [council],           timeout_seconds: 14400, council_quorum: 3 }
  council_plus_governance:
                  { reviewers: [council, governance],
                    timeout_seconds: 604800,
                    council_quorum: 5,
                    governance_quorum_pct: 0.10 }
```

---

## Interaction with existing systems

- **Bounty claims** (existing): `claim_bounty` reads `bounty.scope_classification` and the claimant's trust level; rejects if the matrix says trust is too low.
- **Proof-receipt gate** (existing): on submission, the receipt's `intent.scope_classification` must match what the bounty was posted under, else shape-gate rejects.
- **ExperimentProposal** (new, Phase 2): on submission, the matrix is consulted to set `required_approval`. The proposer is rejected at submission time if their trust is below the floor for the proposed scope.
- **ImpactOutcome** (new, Phase 3): not directly gated, but the Experiment it closes is.
- **Council escalation** (existing): the matrix is the source for *when* Council is required. Cleans up the implicit "novel territory" criterion from `escalations.rs`.

---

## Override semantics

An override is a higher-authority decision to bypass the matrix for a specific instance — not a permanent rule change.

- **Oracle override** (e.g., for `DataModifying` with confidence < 0.8): Council member writes a signed reason. Reputation cost on the Council member if downstream signal proves the override was bad.
- **Multisig override** (`InfrastructureTouching`): three Council members sign a reasoned override.
- **No-override scopes**: `SelfModifying` has no override path. The only way through is the full path. This is a hard floor.

Override events are first-class artifacts — they get logged, audited, and feed back into Council reputation.

---

## Open questions

1. **Should `oracle_confidence_threshold` be per-scope tunable, or uniform at 0.8?** Proposed: per-scope, defaulting to 0.8, override-able in the YAML. `SelfModifying` may want a much higher floor (0.95) even though Council always reviews — Oracle confidence informs how much weight Council gives the recommendation.
2. **Should `frequency_cap` decay back to zero gracefully, or hard-cap at midnight?** Hard cap is simpler; gradual restore (e.g., 1 token per N hours) is more fair to long-running agents. Proposed: hard cap for v1, revisit when there's traffic.
3. **Should trust level itself be auto-adjusting based on matrix violations?** E.g., if an agent misclassifies scope 3 times in a week, drop them a trust level. Proposed: yes, but separate spec (reputation impact rules, Phase 3 territory).
4. **What's the Council membership policy?** The matrix references Council; that's a separate governance doc we'll need before Phase 3 ships.
5. **What happens during matrix YAML migration?** First load needs explicit "version 0 → version 1" migration, otherwise existing pending bounties may suddenly fail. Proposed: pin existing bounties to the matrix version at posting time; new bounties use whatever's current.

---

## Validation

When implemented:

- Relay loads YAML at startup; refuses to start if YAML is malformed or references undefined `approval_paths`.
- Bounty claims, ExperimentProposal submissions, and proof-receipt gates all query the matrix at the same code path (no duplicate enforcement).
- Markdown table is generated from YAML in CI; PR check fails on drift.
- Tests: every cell in the table has a corresponding test that asserts the right approval path is invoked.
- Override events appear in `audit_log` with reviewer, reason, and the bypassed rule.

End of spec.
