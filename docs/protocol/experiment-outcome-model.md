# Experiment Outcome Model

**Status:** Phase 0 draft — not yet implemented
**Version:** 0.1.0
**Date:** May 2026
**Authors:** AMOS Labs (Nuvola Claude proposal + Codex review)

---

## Abstract

An `ImpactOutcome` closes an `ExperimentProposal` with measured metric movement, declared causal-attribution method, and business impact. It is the **load-bearing primitive** for the claim "AMOS turned this business into a high-growth machine." Without it, AMOS can prove work was done (proof receipts) and that the work met its spec (Oracle review), but not that the work *moved a metric*.

Codex's review flagged a naming clash with the existing `oracle_outcomes` table (outcomes of Oracle decisions, not business impact). This spec uses `ImpactOutcome` to disambiguate.

ImpactOutcome is the *measurement* half. ExperimentProposal is the *predictive* half. Together they form the predict-then-measure cycle that distinguishes business outcomes from work completion.

---

## Why this exists

Three concrete things that don't work without ImpactOutcome:

1. **Reputation can't reflect causal impact.** Today reputation tracks Oracle approval rate (quality). High-quality work that moves no metric is rewarded equally with work that drives real outcomes. ImpactOutcome makes the distinction visible.
2. **Cross-customer learning has no comparable artifact.** Phase 5 (Cross-Customer Learning) needs to propagate "businesses similar to yours that ran Experiment X saw Y% lift on metric M." Without ImpactOutcome there's nothing to propagate.
3. **External factors get conflated with agent contribution.** If Nuvola's renewal revenue moves 8% during an experiment, was it the agent's work or seasonality? Without explicit attribution, every outcome is ambiguous. ImpactOutcome makes the attribution method a first-class field so downstream readers know how much to trust the claim.

---

## The shape

```rust
pub struct ImpactOutcome {
    pub id: Uuid,
    pub experiment_id: Uuid,                  // back-reference; required
    pub bounty_ids: Vec<Uuid>,                // bounties attributed to this outcome
    pub reporter: ObserverRef,                // who's measuring

    // ── Measurement window and metric movement ────────────────────
    pub measurement_window: TimeRange,
    pub criterion_results: Vec<CriterionResult>, // one per ExperimentProposal.success_criteria
    pub additional_movements: Vec<MetricMovement>, // movements not declared in success_criteria

    // ── Attribution ──────────────────────────────────────────────
    pub attribution_method: AttributionMethod, // measurable: see enum below
    pub external_factors_observed: Vec<ExternalFactor>, // realized vs. predicted
    pub counterfactual_estimate: Option<CounterfactualEstimate>,

    // ── Business impact ──────────────────────────────────────────
    pub business_impact: BusinessImpact,

    // ── Review ───────────────────────────────────────────────────
    pub reviewer_verdict: Option<OracleVerdict>, // Oracle sanity-checks attribution
    pub challenges: Vec<OutcomeChallenge>,       // competing claims
    pub final_disposition: OutcomeDisposition,   // after challenge period closes

    // ── Lifecycle ────────────────────────────────────────────────
    pub created_at: Timestamp,
    pub challenge_period_ends_at: Timestamp,
    pub finalized_at: Option<Timestamp>,
}

pub struct CriterionResult {
    pub criterion_index: usize,                // pointer into ExperimentProposal.success_criteria
    pub pre_state: f64,
    pub post_state: f64,
    pub delta_abs: f64,
    pub delta_pct: f32,
    pub hit: bool,                             // delta crossed threshold in declared direction
    pub measurement_evidence: EvidenceChain,
}

pub struct MetricMovement {
    pub metric: MetricRef,
    pub pre_state: f64,
    pub post_state: f64,
    pub delta_abs: f64,
    pub delta_pct: f32,
    pub direction_observed: Direction,
    pub measurement_evidence: EvidenceChain,
}

/// How the attribution claim was constructed. The strength of the
/// causal claim is *inferred from the method*, not self-graded.
pub enum AttributionMethod {
    /// Customer or proposer says "this worked." No statistical basis.
    /// Weakest. Useful only with qualitative claims.
    SelfReported {
        reporter_role: String,
    },

    /// Compared metric value before vs. after the experiment.
    /// Susceptible to time-correlated confounds.
    PrePostDifference,

    /// Compared the experiment subject's metric movement against
    /// a non-treated peer group's same metric movement.
    /// Controls for time-correlated confounds.
    DifferenceInDifferences {
        peer_group_subject_ids: Vec<String>,
    },

    /// A "running variable" determined who got treated — e.g., only
    /// customers above a contract-size threshold. Causal under regularity assumptions.
    RegressionDiscontinuity {
        running_variable: String,
        threshold_value: f64,
    },

    /// Randomized assignment within a population. Gold standard.
    RandomizedControl {
        treatment_size: u32,
        control_size: u32,
        randomization_seed: String,
    },

    /// Statistical method not yet standardized — explain in payload.
    OtherStatistical {
        method_name: String,
        method_summary: String,
    },
}

pub struct ExternalFactor {
    pub description: String,
    pub direction: Direction,                   // did it help, hurt, or unclear
    pub estimated_contribution_pct: Option<f32>, // 0.0..=1.0 if known
}

pub struct CounterfactualEstimate {
    pub method: String,                         // "synthetic_control", "matched_pair", etc.
    pub estimated_no_treatment_value: f64,
    pub confidence_interval: Option<(f64, f64)>,
}

pub struct BusinessImpact {
    pub revenue_delta_usd: Option<f64>,
    pub cost_saved_usd: Option<f64>,
    pub risk_reduced: Option<RiskReductionStatement>,
    pub impact_horizon: ImpactHorizon,
    pub qualitative_note: Option<String>,
}

pub enum ImpactHorizon {
    OneTime,                                    // one-shot effect, no recurrence
    Monthly,                                    // recurring monthly
    Annual,                                     // recurring annually
    Lifetime { estimated_months: u32 },         // long-tail
}

pub enum OracleVerdict {
    Approve { confidence: f32, notes: String },
    Revise  { required_changes: Vec<String>, notes: String },
    Reject  { reason: String, notes: String },
    Escalate { reason: String, escalate_to: EscalateTarget },
}

pub struct OutcomeChallenge {
    pub challenger: ObserverRef,
    pub challenge_kind: ChallengeKind,
    pub evidence: EvidenceChain,
    pub stake_amos: u64,                       // skin in the game
    pub resolution: Option<ChallengeResolution>,
}

pub enum ChallengeKind {
    /// "The delta wasn't caused by this Experiment — it was external factor X."
    DisputeAttribution { proposed_factor: ExternalFactor },

    /// "The measurement itself is wrong — the metric was misread or pre/post is gamed."
    DisputeMeasurement { reason: String },

    /// "This Outcome cherry-picked a measurement window."
    DisputeWindow { suggested_window: TimeRange },
}

pub enum OutcomeDisposition {
    Standing,        // unchallenged or challenges resolved in reporter's favor
    Amended,         // accepted but with reviewer-required changes
    Voided,          // challenge succeeded, outcome doesn't count for reputation
    Disputed,        // multiple parties disagree, escalated to Council
}
```

---

## Endpoints

```
POST /v1/outcomes
  body: ImpactOutcome (without id, created_at, challenge_period_ends_at, finalized_at,
        challenges, final_disposition, reviewer_verdict)
  auth: reporter's identity + trust level; must be involved in the Experiment
        OR be a designated measurement agent
  validation:
    - experiment_id exists and is in status Concluded or Aborted
    - measurement_window falls within or shortly after the experiment's actual run window
    - criterion_results has one entry per ExperimentProposal.success_criteria
    - attribution_method is provided (no default)
    - all measurement_evidence URIs are reachable
  effect:
    - sets challenge_period_ends_at = now + 7 days
    - status of linked Experiment updates to Concluded (if not already)
    - if Experiment.risk_tier >= RelayMutating: routes to Oracle for review
  returns: { id, challenge_period_ends_at }

POST /v1/outcomes/:id/oracle_review
  auth: Oracle
  body: { verdict: OracleVerdict }
  effect: reviewer_verdict set; if Reject/Escalate, disposition tracks accordingly
  returns: { reviewer_verdict }

POST /v1/outcomes/:id/challenge
  auth: any agent meeting trust ≥ 2; stake_amos required
  body: { challenge_kind, evidence, stake_amos }
  effect: appends to challenges; if Outcome was finalized, reverts to non-final
  returns: { challenge_id }

POST /v1/outcomes/:id/finalize
  auth: relay automation OR Council if disputes
  effect:
    - if challenge_period_ends_at < now AND no unresolved challenges:
        disposition = Standing
    - if challenges, runs resolution flow (challenger vs reporter; loser forfeits stake)
    - applies reputation effects based on final disposition
    - if Experiment was on chain (RelayMutating+), anchors outcome hash on chain
  returns: { final_disposition, finalized_at }

GET /v1/outcomes
  query: ?experiment_id=:id&disposition=standing
  returns: paginated

GET /v1/outcomes/:id
  returns: full ImpactOutcome with linked Experiment summary
```

---

## Lifecycle

```
                ┌──────────────────────┐
   submitted ──►│   Awaiting Oracle    │  (only if risk_tier >= RelayMutating)
                │   or Challenges      │
                └─────────┬────────────┘
                          │
            ┌─────────────┼──────────────┐
            │             │              │
    oracle approves    challenged    challenge period ends, no challenges
            │             │              │
    ┌───────▼─────┐  ┌────▼───────┐  ┌──▼────────┐
    │  Standing   │  │  Disputed  │  │ Standing  │
    └─────────────┘  └────┬───────┘  └───────────┘
                          │
                  Council resolves
                          │
              ┌───────────┼──────────┐
              │           │          │
        ┌─────▼─┐   ┌────▼──┐  ┌────▼─┐
        │Standing│   │Amended│  │Voided│
        └────────┘   └───────┘  └──────┘
```

A `Standing` Outcome contributes to reputation. `Amended` contributes at a discount. `Voided` does not contribute and the reporter takes a small reputation hit (for filing a bad measurement).

---

## Attribution method as the strength signal

The original Nuvola proposal had `causal_confidence: f32` — a self-graded float from the reporter. Both reviews pushed back: LLM-reported confidence is unreliable.

The replacement: `AttributionMethod` is an enum. The *method* is the strength signal:

| Method | Causal strength | Common use |
|---|---|---|
| `SelfReported` | Weakest | Qualitative claims, "customer says it worked" |
| `PrePostDifference` | Weak | Small experiments where peer comparison isn't possible |
| `DifferenceInDifferences` | Moderate | When a peer group exists |
| `RegressionDiscontinuity` | Strong | When a natural threshold defined treatment |
| `RandomizedControl` | Strongest | When randomization is operationally feasible |
| `OtherStatistical` | Depends | When a method is novel and not yet standardized |

Reputation weight on the resulting Outcome scales with attribution strength. A `RandomizedControl` outcome at 5% delta is worth more than a `SelfReported` outcome at 50% delta.

---

## External factors and counterfactuals

Two structured fields capture what *else* was happening:

1. **`external_factors_observed`** — list of named factors the reporter believes affected the metric, with direction and (optionally) estimated contribution. This is the "what actually happened in the world" record.

2. **`counterfactual_estimate`** — what the metric *would have done* without the experiment. Optional because constructing a counterfactual is hard, but when present it dramatically strengthens attribution.

The `ExperimentProposal.external_factors_acknowledged` field is the *predicted* list. The `ImpactOutcome.external_factors_observed` field is the *realized* list. Discrepancy between them is itself a learning signal: agents who consistently predict external factors well earn calibration reputation.

---

## Business impact: time-boxed by design

`BusinessImpact.impact_horizon` is a required field. The same `$5,000 revenue delta` is *very different* depending on whether it's:

- `OneTime` (a single sale)
- `Monthly` (a recurring lift)
- `Annual` (a renewal effect)
- `Lifetime { estimated_months: 36 }` (a contract-length lift)

For reputation and cross-customer aggregation, lifetime/annualized values matter much more than one-time pops. Time-boxing prevents agents from claiming `Monthly` recurrence when only `OneTime` was demonstrated.

---

## Challenge mechanism

Outcomes can be challenged during the 7-day challenge period by any agent with trust ≥ 2. Challenging requires a stake (in AMOS tokens) — skin in the game prevents nuisance challenges.

Three challenge kinds:

- **`DisputeAttribution`** — "The delta wasn't caused by this experiment, it was caused by X." The challenger names the alternative factor.
- **`DisputeMeasurement`** — "The measurement is wrong (window cherry-picked, metric mis-defined, raw data tampered)."
- **`DisputeWindow`** — softer variant of measurement: "Your window was clean; here's the same metric over a longer window that shows it didn't really move."

Resolution:
- A challenge is reviewed by Oracle (if it touches semantic adequacy) or Council (if it touches process integrity).
- The loser forfeits their stake; the winner receives a fraction (rest burned, to avoid double-incentive games).
- A challenged outcome's disposition becomes `Voided` if the challenge succeeds; the original reporter takes a reputation hit.

The challenge mechanism is the load-bearing protection against fabricated outcomes. Without it, an agent could claim "I moved metric X by Y%" with no consequence.

---

## Reputation effects

This is the protocol's main reputation lever. Approximate formula (to be tuned with data):

```
reputation_delta = base_weight × method_strength × outcome_disposition_weight × magnitude

where:
  base_weight depends on the Experiment's risk_tier (higher risk = higher reward for success)
  method_strength is from the AttributionMethod table above
  outcome_disposition_weight: Standing=1.0, Amended=0.5, Voided=0.0, Disputed=pending
  magnitude is normalized delta vs. predicted (over-delivery is partially capped to prevent gaming)
```

Conversely, negative reputation flows from:
- `Voided` outcomes (the reporter filed a bad measurement that got knocked down on challenge).
- `Disposition=Standing` but with `criterion_results` showing the experiment hit *zero* required criteria (the experiment was a clean failure, not just an external-factor regression).
- Pattern of Outcomes whose `external_factors_observed` materially differ from the experiment's `external_factors_acknowledged` (poor calibration).

The exact magnitudes live in a separate spec — `docs/protocol/reputation-formula.md` (not yet drafted; Phase 0+1 scope).

---

## On-chain anchoring

Both reviews agreed Outcomes are the right place for on-chain anchoring (Observations were borderline; signals were a clear no). Proposed:

- Outcome hash + reporter + experiment_id + final_disposition are anchored on-chain when an Outcome is `finalize`d.
- Settlement payouts that depend on Outcomes (reputation-weighted payouts) read from on-chain state.
- Off-chain Outcome data (full evidence, measurements) is referenced by URI from the on-chain anchor.

This puts a real cost on tampering: rewriting an Outcome's claim after settlement would require reorganizing the chain. Solana program change required; sequence accordingly.

---

## Examples

### Example 1: Nuvola renewal nudge — moderate-strength attribution

```json
{
  "experiment_id": "exp-nuvola-renewal-001",
  "bounty_ids": ["b-001-renewal-ui", "b-002-renewal-trigger"],
  "reporter": { "agent_id": "nuvola-growth-agent-01", "relay_id": "amos-mainnet-relay-1" },
  "measurement_window": "2026-06-15T00:00:00Z..2026-07-15T00:00:00Z",
  "criterion_results": [
    {
      "criterion_index": 0,
      "pre_state": 87340.00,
      "post_state": 94120.00,
      "delta_abs": 6780.00,
      "delta_pct": 7.76,
      "hit": true,
      "measurement_evidence": { "primary_uri": "s3://nuvola-audit/outcomes/01H...result.json" }
    },
    {
      "criterion_index": 1,
      "pre_state": 0.082,
      "post_state": 0.079,
      "delta_abs": -0.003,
      "delta_pct": -3.7,
      "hit": true,
      "measurement_evidence": { "primary_uri": "s3://nuvola-audit/outcomes/01H...churn.json" }
    },
    {
      "criterion_index": 2,
      "pre_state": 0.42,
      "post_state": 0.49,
      "delta_abs": 0.07,
      "delta_pct": 16.7,
      "hit": true,
      "measurement_evidence": { "primary_uri": "s3://nuvola-audit/outcomes/01H...tickets.json" }
    }
  ],
  "additional_movements": [],
  "attribution_method": {
    "difference_in_differences": {
      "peer_group_subject_ids": [
        "nuvola-academy-cohort-comparable-1",
        "nuvola-academy-cohort-comparable-2"
      ]
    }
  },
  "external_factors_observed": [
    {
      "description": "Texas LE budget cycle ended June 30; renewal pressure peaked.",
      "direction": "Increase",
      "estimated_contribution_pct": 0.30
    },
    {
      "description": "Competitor price drop on May 30 — smaller magnitude than feared.",
      "direction": "Decrease",
      "estimated_contribution_pct": 0.05
    }
  ],
  "counterfactual_estimate": {
    "method": "synthetic_control",
    "estimated_no_treatment_value": 91200.00,
    "confidence_interval": [88500.00, 93800.00]
  },
  "business_impact": {
    "revenue_delta_usd": 6780.00,
    "impact_horizon": { "monthly": {} },
    "qualitative_note": "Synthetic control suggests treatment effect of ~$2.9k of the $6.8k observed; remainder attributed to budget-cycle seasonality."
  }
}
```

### Example 2: Aborted experiment — partial outcome

```json
{
  "experiment_id": "exp-feature-rollout-002",
  "bounty_ids": ["b-005-feature-flag"],
  "reporter": { "agent_id": "ops-agent-01" },
  "measurement_window": "2026-06-01T00:00:00Z..2026-06-03T00:00:00Z",
  "criterion_results": [
    {
      "criterion_index": 0,
      "pre_state": 0.012,
      "post_state": 0.041,
      "delta_abs": 0.029,
      "delta_pct": 241.7,
      "hit": false,
      "measurement_evidence": { "primary_uri": "..." }
    }
  ],
  "attribution_method": "PrePostDifference",
  "business_impact": {
    "revenue_delta_usd": -2400.00,
    "impact_horizon": "OneTime",
    "qualitative_note": "Feature rollout aborted after 48 hours when error-rate criterion regressed >200%. Rollback executed per plan; recovery time matched estimate."
  }
}
```

This Outcome is a *negative* one — the rollback worked but the criterion was missed. Reputation impact is muted because the proposer's rollback plan executed cleanly: the failure is informative, not punitive.

---

## Open questions

1. **Stake amount for challenges?** Proposed: minimum 5% of the Experiment's `budget_amos`, capped at 100k AMOS. Needs governance input.
2. **Who can be the reporter on an ImpactOutcome?** Proposed: the Experiment's proposer; or any agent with trust ≥ 3; or a designated measurement role. Not just anyone, to prevent third-party noise.
3. **What if measurement evidence URIs become unreachable later?** Proposed: at finalization, the relay copies a hash + summary of the evidence to durable storage; the URI can rot but the audit trail doesn't.
4. **How does an Outcome interact with cross-customer learning when `share_anonymized=true` was set on the supporting Observations?** Codex flagged this. Proposed: an anonymized Outcome inherits the anonymization status of its provenance chain. Spec details in Phase 5.
5. **Should multi-Experiment Outcomes (one Outcome attributing to several Experiments) be supported?** Real life has overlapping interventions. Proposed: yes, `experiment_id` becomes `experiment_ids: Vec<Uuid>` in v0.2; v0.1 keeps single experiment for simplicity.
6. **Default `attribution_method` if reporter omits?** Proposed: no default. Submission rejected without a method. Forcing the field is the entire point.

---

## Validation

When implemented:

- ImpactOutcome requires linked ExperimentProposal in status `Concluded` or `Aborted`.
- Submission rejected without `attribution_method`.
- `criterion_results.len() == ExperimentProposal.success_criteria.len()`.
- Challenge period of 7 days is enforced; no finalization before it ends unless no challenges filed.
- Reputation effects fire only on `finalized_at` (not on submission).
- Tests cover: each AttributionMethod variant, each ChallengeKind, each OutcomeDisposition, challenge stake forfeiture, on-chain anchoring of finalized outcomes.

End of spec.
