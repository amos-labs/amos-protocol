# Phase 0: Protocol Primitives — Coherent Spec Set

**Status:** Draft for review
**Date:** May 2026
**Authors:** AMOS Labs, synthesizing:
- `instructions/PROTOCOL_PRIMITIVES_PROPOSAL.md` (Nuvola Claude, 2026-05-25)
- `instructions/SWARM_NUVOLA_PROTOCOL_REVIEW.md` (Codex, 2026-05-25)
- AMOS-side review and pushback on both

---

## Context

Two prior reviews converged on the same conclusion: AMOS has a complete *execution* layer (proof receipts, Oracle review, settlement, reputation) but is missing the *measurement* layer that lets agents observe state, propose hypotheses, execute proof-carrying work, measure outcomes, and turn the learning into reusable protocol intelligence.

This Phase 0 deliverable is the **coherent set of protocol specs** that defines the missing layer. No Rust is written yet. The goal is to land the *shape* and *interfaces* — field names will iterate during implementation.

Phase 0 deliberately precedes any code. Both reviewers agreed this is the highest-leverage 2-3 days of work in the entire roadmap because it surfaces cross-cutting concerns (what does an Outcome reference? does an Observation flow through Oracle? where does NetworkSignal live?) before they become code conflicts.

---

## The six primitives, at a glance

| # | Primitive | Lifecycle | Cardinality | Spec |
|---|---|---|---|---|
| 1 | **ScopeClassification** | Closed enum; immutable | 5 values | [scope-classification-enum.md](scope-classification-enum.md) |
| 2 | **Risk-Approval Matrix** | YAML config + auto-generated table | One per protocol version | [risk-approval-matrix.md](risk-approval-matrix.md) |
| 3 | **Observation** | Durable, immutable; aggregatable | High volume (many per agent per day) | [observation-model.md](observation-model.md) |
| 4 | **NetworkSignal** | Ephemeral, decaying; reinforced or expires | Very high volume (Redis-backed) | [network-signal-model.md](network-signal-model.md) |
| 5 | **ExperimentProposal** | Predictive; status machine; rollback-aware | Moderate (a few per agent per week) | [experiment-proposal-model.md](experiment-proposal-model.md) |
| 6 | **ImpactOutcome** | Measurement; challengeable; on-chain anchored on finalize | One per concluded/aborted Experiment | [experiment-outcome-model.md](experiment-outcome-model.md) |

Cross-Customer Learning (Phase 5) is intentionally not in this Phase 0 set — spec-only for later.

---

## How the primitives fit together

```
                       ┌─────────────────────┐
                       │  ScopeClassification │  ← closed enum, foundational
                       └──────────┬──────────┘
                                  │ keys
                       ┌──────────▼──────────┐
                       │ Risk-Approval Matrix │  ← YAML, single source of truth
                       └──────────┬──────────┘
                                  │ consulted by
                                  │
   ┌───────────────────┐          │
   │   NetworkSignal   │  emits   │
   │   (ephemeral)     │◄─────────┼────┐
   └─────────┬─────────┘          │    │
             │ may aggregate into │    │
             ▼                    │    │
   ┌───────────────────┐          │    │
   │   Observation     │          │    │
   │   (durable)       │──────────┤    │
   └─────────┬─────────┘          │    │
             │ supports           │    │
             ▼                    │    │
   ┌───────────────────┐  routed  │    │
   │ ExperimentProposal │─────────►    │
   │ (predictive)       │ via matrix │
   └─────────┬─────────┘               │
             │ spawns bounties         │
             ▼                         │
   ┌───────────────────┐               │
   │   Bounty + Proof  │               │
   │   Receipt + Oracle │  (existing)  │
   └─────────┬─────────┘               │
             │ concludes               │
             ▼                         │
   ┌───────────────────┐               │
   │  ImpactOutcome    │  challenge-   │
   │  (measurement)    │  protected, ──┤
   └─────────┬─────────┘  on-chain     │
             │           anchored      │
             │                         │
             ▼                         │
   ┌───────────────────┐               │
   │   Reputation      │  feeds matrix │
   │   (existing)      │  trust gates ─┘
   └───────────────────┘
```

The loop is intentionally circular: Outcomes update reputation; reputation gates trust level; trust level controls what scope an agent can propose. The system becomes self-shaping over time.

---

## What changed from the original proposals (and why)

### From the Nuvola proposal

| Original | Phase 0 | Why |
|---|---|---|
| `scope_classification` reused from existing free-form string | Closed enum, named `ScopeClassification`, separate from existing free-form `work_kind` | The current field is free-form and conflates *what action* with *what kind of work*. Both reviews flagged this. |
| Observation includes ephemeral signals | Split: `Observation` (durable) and `NetworkSignal` (ephemeral) | Codex's Swarm review caught this. Lifecycle requirements are too different to share a shape. |
| `Observation.confidence: f32` | `Observation.evidence_strength: EvidenceStrength` (measurable signals) | LLM-reported confidence is famously miscalibrated. Replace with countable quantities. |
| `Experiment` | `ExperimentProposal` | Naming clash with `amos-autoresearch`'s prompt-mutation experiments. |
| `Experiment.success_threshold: f32` | `ExperimentProposal.success_criteria: Vec<SuccessCriterion>` | Single threshold is too binary for correlated metrics. |
| `Outcome` | `ImpactOutcome` | Naming clash with existing `oracle_outcomes` table. |
| `Outcome.causal_confidence: f32` | `ImpactOutcome.attribution_method: AttributionMethod` enum | Causal strength is inferable from the *method* used. No self-graded float. |
| `BusinessImpact` without time-boxing | `BusinessImpact.impact_horizon: ImpactHorizon` required | A `$5k` revenue delta is very different if `OneTime` vs `Lifetime { months: 36 }`. |
| `rollback_plan: String` (free text) | `RollbackPlan { triggers, actions, recovery_time }` (structured) | Free-text rollback means "no rollback" in practice. |
| Risk-Approval Matrix in Phase 4 | Risk-Approval Matrix in Phase 0 | Both reviews agreed it should accelerate — Phase 2 and 3 depend on it. |
| No on-chain anchoring spec for Outcomes | Outcomes anchor on-chain at finalize | Without it, a relay operator can fabricate Outcomes. Solana program change required. |

### From the Codex review

| Codex contribution | Adopted | Notes |
|---|---|---|
| Add `NetworkSignal` as a sixth primitive | Yes | Full spec in `network-signal-model.md`. |
| Use `signal` / `network_signal` terminology, not `pheromone` | Yes | Codex's call. |
| Rename `Experiment` → `ExperimentProposal` | Yes | Adopted. |
| Rename `Outcome` → `ImpactOutcome` | Yes | (Codex offered several variants; we picked `ImpactOutcome` for the load-bearing distinction from `oracle_outcomes`.) |
| Promote Risk-Approval Matrix to early phase | Yes | Now Phase 0. |
| Spec multi-tier memory taxonomy | Deferred | Useful but orthogonal to predict-then-measure loop. Separate spec when reputation rules are written. |
| Spec context-aware intake assistant | Deferred | Real value once Observation+Experiment are live and agents need help converting customer needs into well-formed proposals. |

---

## Implementation sequencing (revised)

| Phase | Deliverable | Where | Effort |
|---|---|---|---|
| **0** *(this set)* | 6 spec docs + open questions resolved | `docs/protocol/*.md` | 2-3 days writing + 2-3 days Rick + Council review |
| **1** | Observation primitive: Rust struct + endpoints + persistence + Nuvola publishing first ones | AMOS + Nuvola | 1-2 weeks |
| **1b** | NetworkSignal primitive: Redis-backed in relay + topic allowlist + reinforce flow | AMOS | 1 week (can parallelize with Phase 1) |
| **2** | Risk-Approval Matrix YAML + relay enforcement + Markdown autogen | AMOS | 1 week |
| **3** | ExperimentProposal: struct + endpoints + Oracle review hook + bounty-creation linkage | AMOS | 2 weeks |
| **4** | ImpactOutcome: struct + endpoints + reputation wiring + on-chain anchoring | AMOS + Solana | 2-3 weeks |
| **5** | Cross-Customer Learning: spec only | AMOS docs | 2-3 days |

Phase 1 and 1b can run in parallel because they touch different storage layers. Phase 2 (matrix) must land before Phase 3 (proposals) can be functional. Phase 4 (outcomes) requires Solana program changes for on-chain anchoring — sequence accordingly.

---

## Open questions for Rick (and Council, if convened)

The specs themselves contain detailed open questions in their own sections. The following are the cross-cutting decisions that affect multiple specs and need a single answer:

1. **Council membership.** Several specs reference "Council" as a reviewer. Who's on it today? What's the quorum policy? `risk-approval-matrix.md` assumes a 3-of-N quorum for `InfrastructureTouching` and 5-of-N for `SelfModifying`. Needs a governance doc separate from these specs.

2. **Token economics for stakes and rewards.** `ImpactOutcome` challenges require a stake. `ExperimentProposal` budgets are denominated in AMOS. What's the right baseline (5% of experiment budget? capped at 100k AMOS?). Probably a separate `docs/protocol/staking-formula.md`.

3. **Privacy / consent surfaces for cross-customer propagation.** `Observation.share_anonymized: bool` requires explicit tenant-level consent. What's the UI for that consent? Where does it live in the harness? Needs a frontend design spec.

4. **Where does Nuvola plug in first?** Phase 1 says "Nuvola publishes first Observations." Which Nuvola metric streams come first — agency renewals, course completion, helpdesk tickets, all of them? Affects the MCP tool surface Nuvola needs to build.

5. **On-chain anchoring for Outcomes — full settlement or hash batching?** The Outcome spec proposes anchoring on finalize. Open: do reputation-weighted payouts (settling AMOS tokens) read directly from on-chain state, or from relay-cached state with on-chain hash as tamper proof? Affects Solana program scope.

6. **Single Council, or per-relay Council?** With the multi-relay future on the roadmap, this matters: does every relay have its own Council, or is there one cross-relay Council? `risk-approval-matrix.md` assumes "the Council" singular today; multi-relay requires picking a model.

These are the questions that block confident move to Phase 1. The specs themselves are reviewable and corrigible without these answers locked.

---

## What I'd like from review

The audience for this review is:
- Rick (founder, final call on direction)
- The Nuvola Claude agent that wrote the original proposal (so they can fold corrections into their next pass)
- The Codex review author (so they know which contributions landed and which were deferred)
- Future Phase 1 implementers (so they have an unambiguous spec to build from)

Specifically:
1. Push back on field shapes — any field is renameable; any structure is reshape-able until Phase 1 starts.
2. Identify cross-spec inconsistencies — there are probably a few I missed.
3. Resolve the 6 open questions above (Rick has standing to answer most; some need Council input).
4. Flag any AMOS existing work I missed that overlaps.

---

## What's next if this passes review

- Rick + Council resolve the 6 cross-cutting open questions.
- Nuvola Claude + Codex agent fold corrections into a final review pass.
- Phase 1 implementation begins on the AMOS side (Observation + NetworkSignal in parallel).
- Nuvola begins building MCP tool surface to publish first Observations.
- Phase 0 spec set becomes the canonical reference; the original proposal + Codex review move to `docs/archive/` after merge.

End of Phase 0 README.
