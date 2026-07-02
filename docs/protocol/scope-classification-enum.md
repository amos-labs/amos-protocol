# Scope Classification Enum

**Status:** Phase 0 draft — not yet implemented
**Version:** 0.1.0
**Date:** May 2026
**Authors:** AMOS Labs (synthesizing Nuvola Claude proposal + Codex review)

---

## Abstract

This document closes the `scope_classification` field on `proof_receipt.intent` and the proposed `ExperimentProposal.risk_tier` from a free-form string into a small, closed enum. Every other Phase 0 primitive — Risk-Approval Matrix, ExperimentProposal, ImpactOutcome — depends on this enum being stable, so it must land first.

Today the field is conceptually present (`amos-relay/src/proof_receipt.rs:51`, `docs/AMOS_PROOF_CARRYING_DEV_PIPELINE.md:91`) but accepts arbitrary strings like `"feature"`, `"bugfix"`, `"docs"`. That's a domain-of-work classification, not a risk-of-action classification, and conflating the two has left agents and humans with no shared model of "if I propose X, who approves it?"

---

## Why this exists

Three concrete problems with the current free-form string:

1. **No machine-readable approval routing.** Risk-Approval Matrix needs to compile `{scope, trust_level, action_type} → approval_path` into a YAML the relay can enforce. Free-form keys make that table unbounded.
2. **`risk_tier` and `kind_of_work` are different concepts.** A "bugfix" can be `read_only` or `infrastructure_touching` depending on what it touches. Today's enum collapses them.
3. **Agents can't reason about their own risk envelope.** Without a closed set, an agent has no way to look at a proposed action and ask "is this beyond my trust level?" before submitting.

The values below split the concept cleanly: `scope_classification` answers *what does this action touch?* and a separate `work_kind` (existing free-form) answers *what kind of work was done?*.

---

## The enum

Five values, ordered by blast radius.

```rust
pub enum ScopeClassification {
    /// Pure reads. No state changes anywhere — no DB writes, no file
    /// edits, no API calls with side effects, no token transfers.
    /// Examples: querying records, fetching analytics, searching the
    /// web, reading a file.
    ReadOnly,

    /// Modifies state inside the customer's harness boundary only.
    /// No effects on the relay, network, other tenants, or shared
    /// infrastructure. Examples: writing a record, updating a page,
    /// posting to the customer's own Slack, modifying their schema.
    DataModifying,

    /// Modifies state on the relay or network — not just one tenant.
    /// Examples: posting a bounty, claiming work, submitting a proof
    /// receipt, registering an agent, settling a payout.
    RelayMutating,

    /// Touches shared infrastructure that other tenants or other
    /// relays depend on. Examples: deploying a harness image, rolling
    /// an ECS service, modifying CI, changing an ALB rule, deploying
    /// a Solana program.
    InfrastructureTouching,

    /// Modifies the AMOS reasoning substrate, governance rules,
    /// constitutional prompts, or settlement/reputation logic
    /// itself. Examples: editing the Oracle constitutional prompt,
    /// changing the bounty proof-receipt gate logic, modifying the
    /// trust-level progression formula, altering on-chain payout
    /// math.
    SelfModifying,
}
```

A sixth value was considered (`CustomerSensitive`, for actions that touch customer PII or financial data even within their harness) and rejected: that's a policy concern orthogonal to scope, better modeled as a separate `data_sensitivity` field. Keep this enum about *where the effect lands*, not *how careful you have to be*.

---

## Mapping from current free-form values

Existing `proof_receipt.intent.scope_classification` strings will be migrated. The mapping below is the migration script, not user-visible mapping (agents going forward must use the new enum).

| Current free-form | Maps to |
|---|---|
| `feature`, `bugfix`, `refactor` (on customer harness code) | `DataModifying` |
| `feature`, `bugfix`, `refactor` (on relay code) | `RelayMutating` |
| `infra` (Docker, CI, ECS) | `InfrastructureTouching` |
| `docs`, `content` | `ReadOnly` if no code changes; `DataModifying` if any |
| `protocol`, `governance`, `oracle`, `settlement` | `SelfModifying` |

The existing `work_kind` concept (feature / bugfix / refactor / docs / content) stays as a separate free-form field on `proof_receipt.intent` — it's useful for sorting and search, just not for approval routing.

---

## How agents pick the right value

The agent or proposer is responsible for declaring scope, and the relay's shape gate validates the declaration is consistent with the work submitted. The rule of thumb:

> Pick the value that describes the *highest-blast-radius effect* of any single action in your work. A bounty that does 12 read-only queries plus one settlement transaction is `RelayMutating`, not `ReadOnly`.

The Oracle is responsible for catching misclassification on review. Misclassifying a `SelfModifying` action as `DataModifying` is itself a reputation hit (separate from the work being good or bad).

---

## Interaction with other Phase 0 primitives

- **Risk-Approval Matrix**: keyed by `(scope, trust_level)`. See `risk-approval-matrix.md`.
- **ExperimentProposal**: `risk_tier: ScopeClassification`. Determines approval path before bounty creation.
- **Observation**: not gated by scope (observation is always read-only by definition; observing isn't an action).
- **NetworkSignal**: same as Observation — emitting a signal is itself `ReadOnly`. Acting on a signal carries the scope of the action taken.
- **ImpactOutcome**: not gated by scope, but the underlying ExperimentProposal it closes has one.

---

## Open questions

1. **Should `RelayMutating` be split into "settlement-touching" and "everything else relay-touching"?** Settlement has stricter approval requirements (e.g., council quorum) and conflating it with bounty-posting may force the matrix to use disjunctive rules.
2. **Where does a tool that calls an external API with side effects fit?** A bounty that emails a customer's mailing list is `DataModifying` if we consider the customer's mailing list "inside their harness boundary," or higher if we consider the email recipients "the network." Proposal: it's `DataModifying`; the email recipient list belongs to the customer, not the network.
3. **Does the enum need a `Vec<ScopeClassification>` form for multi-effect actions?** A bounty that both edits a customer page AND deploys a CI change has two scopes. Proposal: take the max (highest blast radius); don't introduce a vec form, it's a small win for big complexity.

---

## Validation

When implemented:

- `proof_receipt.intent.scope_classification` becomes the enum type, not `String`.
- Relay shape gate rejects proof receipts where the field is missing or not one of the five values.
- A migration script rewrites historical free-form values per the table above.
- Tests verify each value maps to the expected approval path in the Risk-Approval Matrix.

End of spec.
