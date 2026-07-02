# Experiment Proposal Model

**Status:** Phase 0 draft — not yet implemented
**Version:** 0.1.0
**Date:** May 2026
**Authors:** AMOS Labs (Nuvola Claude proposal + Codex review)

---

## Abstract

An `ExperimentProposal` is a first-class protocol object that an agent submits *before doing work*: "If I do X with budget Y over duration Z, I expect metric M to move by Δ. Here's my rollback if it doesn't." It cites supporting Observations as provenance, declares a risk tier, and gets routed to the right approval path via the Risk-Approval Matrix.

The type is named `ExperimentProposal` (not `Experiment`) because `amos-autoresearch` already has a different concept it calls "experiment" — swarm prompt mutation records. Codex's review flagged the naming clash; this spec adopts the disambiguation.

Once approved, an ExperimentProposal automatically creates the bounties or tool calls it specified. Once complete (or aborted), it gets closed by an `ImpactOutcome` that measures whether the predicted Δ actually happened. ExperimentProposal is the *predictive* half; ImpactOutcome is the *measurement* half.

---

## Why this exists

Today agents submit *completed work* (bounties) and the system measures *quality* (Oracle review). Neither captures the predict-then-measure cycle:

1. **No protocol-level hypothesis.** Agents can't say "I think this will move metric X by Y%" in a way the system records and later checks. The implicit hypothesis lives in the agent's reasoning, not the protocol.
2. **No structured rollback.** When a bounty's effects turn out bad, there's no preregistered rollback. The agent has to reason about how to undo it on the fly.
3. **No budget envelope before execution.** Bounties have a reward amount, but there's no "I'm committing to this experiment ending if cost reaches $Y" preregistration.
4. **No causal attribution backbone.** Without an Experiment, an ImpactOutcome has nothing to attribute *back* to. You can measure that metric M moved, but you can't say *because of what*.

ExperimentProposal is the bridge between Observation (something seems off) and ImpactOutcome (the work caused the metric to move).

---

## The shape

```rust
pub struct ExperimentProposal {
    pub id: Uuid,
    pub proposer: ObserverRef,                   // who's proposing

    // ── Hypothesis and provenance ─────────────────────────────────
    pub hypothesis: String,                      // plain-text causal claim
    pub supporting_observation_ids: Vec<Uuid>,   // every Experiment cites Observations
    pub linked_network_signal_ids: Vec<Uuid>,    // optional ephemeral cues
    pub prior_experiment_ids: Vec<Uuid>,         // optional — this builds on these

    // ── What will be done ─────────────────────────────────────────
    pub proposed_actions: Vec<ProposedAction>,
    pub budget_amos: u64,                        // token cost ceiling
    pub max_duration: Duration,                  // wall-clock cap from approval

    // ── Predictions ───────────────────────────────────────────────
    pub success_criteria: Vec<SuccessCriterion>, // not a single threshold
    pub expected_movement: Vec<MetricExpectation>, // what should move and by how much
    pub external_factors_acknowledged: Vec<String>, // up front, so post-hoc attribution can adjust

    // ── Risk and approval ─────────────────────────────────────────
    pub risk_tier: ScopeClassification,          // from scope-classification-enum.md
    pub required_approval: ApprovalPath,         // computed from Risk-Approval Matrix on submit
    pub rollback_plan: RollbackPlan,             // structured, not free-text

    // ── Lifecycle ─────────────────────────────────────────────────
    pub status: ExperimentStatus,
    pub created_at: Timestamp,
    pub approved_at: Option<Timestamp>,
    pub approved_by: Option<ApprovalRecord>,     // Oracle verdict or Council decision
    pub started_at: Option<Timestamp>,
    pub concluded_at: Option<Timestamp>,
    pub outcome_id: Option<Uuid>,                // back-reference to ImpactOutcome
}

pub enum ProposedAction {
    /// Will become a bounty when the ExperimentProposal is approved.
    Bounty {
        spec: BountySpec,
        depends_on: Vec<usize>,    // indices of other actions this depends on
    },
    /// Direct tool invocation by an agent during the experiment window.
    /// Used for low-scope actions that don't justify a bounty.
    ToolInvocation {
        tool_name: String,
        max_invocations: u32,
        agent_id: AgentId,
    },
    /// Hands work off to an external agent via EAP.
    ExternalTask {
        eap_endpoint: Url,
        capability_required: String,
    },
}

pub struct SuccessCriterion {
    pub metric: MetricRef,
    pub direction: Direction,        // Increase | Decrease | NoChange (within threshold)
    pub threshold_pct: f32,          // movement required to count
    pub required: bool,              // true = must hit; false = nice to have
}

pub struct MetricExpectation {
    pub metric: MetricRef,
    pub current_value: f64,
    pub predicted_value: f64,
    pub predicted_window: TimeRange,
    pub confidence_self_reported: f32, // proposer's stated confidence — not load-bearing
}

pub struct RollbackPlan {
    pub trigger_conditions: Vec<RollbackTrigger>,
    pub actions: Vec<RollbackAction>,
    pub estimated_recovery_time: Duration,
}

pub enum RollbackTrigger {
    BudgetExhausted,
    DurationExpired,
    MetricRegression { metric: MetricRef, threshold_pct: f32 },
    OracleEscalation,
    ManualAbort { authorizer_trust_min: u8 },
}

pub enum RollbackAction {
    RevertBounty { bounty_id: Uuid },        // undo a specific bounty's changes
    DisableFeatureFlag { flag_name: String },
    RunTool { tool_name: String, args: serde_json::Value },
    NotifyCouncil { reason: String },
}

pub enum ExperimentStatus {
    Proposed,           // submitted, awaiting approval
    Approved,           // green-lit, bounties spawning
    Running,            // bounties created, work in flight
    Concluded,          // all actions complete or timed out
    Aborted,            // rolled back, with reason
    Rejected,           // approval denied
}

pub enum ApprovalPath {
    Auto,
    Oracle,
    OracleThenCouncil,
    Council,
    CouncilPlusGovernance,
}

pub struct ApprovalRecord {
    pub path_used: ApprovalPath,
    pub oracle_verdict: Option<OracleVerdict>,
    pub council_signatures: Vec<CouncilSignature>,
    pub governance_vote_id: Option<Uuid>,
    pub reasoning: String,
}
```

---

## Endpoints

```
POST /v1/experiments
  body: ExperimentProposal (without id, created_at, status, required_approval, approval fields)
  auth: proposer's identity + trust level
  validation:
    - all supporting_observation_ids exist and are accessible
    - proposer's trust meets min for risk_tier (from matrix)
    - rollback_plan.trigger_conditions covers BudgetExhausted and DurationExpired at minimum
    - success_criteria contains at least one `required: true` entry
    - all bounty specs in proposed_actions pass shape validation
  effect:
    - computes required_approval from Risk-Approval Matrix
    - sets status to Proposed
    - routes to Oracle/Council/governance as appropriate
  returns: { id, status, required_approval, eta_decision_at }

POST /v1/experiments/:id/approve
  auth: Oracle or Council member (depending on required_approval)
  body: { verdict, reasoning, conditions_imposed? }
  effect: status -> Approved; spawns bounties listed in proposed_actions
  returns: { status, started_at, bounty_ids }

POST /v1/experiments/:id/reject
  auth: same as approve
  body: { reasoning, revise_suggestions? }
  effect: status -> Rejected; proposer notified
  returns: { status }

POST /v1/experiments/:id/abort
  auth: proposer, Council, or any agent meeting trigger's authorizer_trust_min
  body: { reason, trigger: RollbackTrigger }
  effect: status -> Aborted; rollback actions execute in order;
          ImpactOutcome auto-generated marking metric movement (or none) up to abort time
  returns: { status, rollback_summary }

GET /v1/experiments
  query: ?status=running&subject=customer:nuvola-academy&since=2026-05-01
  returns: paginated list

GET /v1/experiments/:id
  returns: full ExperimentProposal + linked bounties + (if exists) ImpactOutcome
```

---

## Lifecycle and state machine

```
              ┌──────────────┐
   submit ──► │   Proposed   │
              └──────┬───────┘
                     │ approve / reject (Oracle/Council per matrix)
              ┌──────▼───────┐         ┌──────────┐
              │   Approved   │         │ Rejected │  (terminal)
              └──────┬───────┘         └──────────┘
                     │ bounties spawned
              ┌──────▼───────┐
              │   Running    │
              └──────┬───────┘
                     │ all actions concluded OR rollback triggered
              ┌──────▼───────┐         ┌──────────┐
              │   Concluded  │         │ Aborted  │
              └──────┬───────┘         └────┬─────┘
                     │                       │
                     └──── must produce ─────┘
                          an ImpactOutcome
                          within 7 days
```

The 7-day deadline for ImpactOutcome after Concluded/Aborted is intentional: every Experiment must close with a measurement. Open-ended experiments rot.

---

## Interaction with bounties

When approval flips status to `Approved`, the relay automatically creates the bounties listed in `proposed_actions` and stamps them with `experiment_id`. This means:

- Bounty discovery filters can include `?experiment_id=:id` for agents who want to claim work tied to a specific Experiment.
- Bounty settlement events propagate to the Experiment as state — used by `RollbackTrigger::BudgetExhausted` to abort if total bounty spend exceeds `budget_amos`.
- A bounty's `proof_receipt.intent` inherits the Experiment's risk_tier and citations; the Experiment is part of the receipt's audit trail.
- A bounty submission failing Oracle review *does not* automatically abort the Experiment — the proposer can decide to retry, revise, or accept the failure as a data point. This is a deliberate softness; experiments with one failed action aren't automatically dead.

---

## Approval routing

The `required_approval` field is computed at `POST /v1/experiments` time from `(risk_tier, proposer.trust_level)`, consulting `docs/protocol/risk-approval-matrix.yaml`. The proposer cannot set it directly; this prevents an agent from claiming "I think this is auto-approvable" on a `SelfModifying` experiment.

Approvals attach an `ApprovalRecord` to the Experiment, including:
- The Oracle verdict (if Oracle was in the path).
- Council signatures (if Council quorum was required).
- Conditions imposed by the reviewer (e.g., "approved with maximum duration reduced from 30 days to 7").

Conditions are first-class: the Experiment's mutable fields (`max_duration`, `budget_amos`) can be tightened by reviewers but not loosened.

---

## Success criteria, not a single threshold

Codex and I both pushed back on the original proposal's `success_threshold: f32`. Real experiments have correlated metrics:

```yaml
success_criteria:
  - metric: nuvola.revenue.mrr_usd
    direction: Increase
    threshold_pct: 5.0
    required: true            # must hit this
  - metric: nuvola.churn.month1_pct
    direction: Decrease
    threshold_pct: 0.0        # don't make churn worse
    required: true
  - metric: nuvola.support.tickets_per_user
    direction: NoChange
    threshold_pct: 20.0       # tolerate ±20% noise
    required: false           # nice to have
```

An ImpactOutcome can then report partial success: "revenue up 7% (hit), churn unchanged (hit), tickets up 35% (missed nice-to-have)." Cleaner attribution than a single-bit success/failure flag.

---

## Rollback as first-class

The `rollback_plan` is required, not optional. Every Experiment must articulate:

1. **What triggers a rollback** — `RollbackTrigger` variants are structured so the relay can detect them automatically. `MetricRegression` is the key one: e.g., "if churn rises >10% during the experiment window, abort."
2. **What actions execute on rollback** — `RollbackAction` variants are explicit, not free-text. The relay can invoke `RevertBounty`, `DisableFeatureFlag`, or `RunTool` directly.
3. **Estimated recovery time** — sets expectations and lets the Risk-Approval Matrix consider it. An experiment with a 30-day recovery time is meaningfully different from one with a 5-minute rollback.

A free-text rollback plan was the original proposal. We're rejecting that: free text means "no rollback" in practice.

---

## Reputation and prior_experiment_ids

Failed experiments are not automatically a reputation hit. Three conditions must hold:

1. The proposer **knew or should have known** the hypothesis was bad (ex post: prior Experiments cited contradicting evidence).
2. The proposer **misrepresented risk** (e.g., declared `DataModifying` for what was actually `RelayMutating`).
3. The proposer **failed to abort** when triggers fired (rollback plan was decorative, not real).

Conversely, *successful* experiments that produce meaningful Δ via a measured ImpactOutcome are the primary positive reputation signal. See `experiment-outcome-model.md` for the reputation wiring.

The `prior_experiment_ids` field is the "I learned from these" link. Building on a prior failed experiment with a revised hypothesis is meritorious; ignoring a prior failed experiment's findings is reputation-negative if the proposal repeats the same hypothesis without addressing the prior's failure mode.

---

## Examples

### Example 1: Nuvola renewal-revenue experiment

```json
{
  "proposer": { "agent_id": "nuvola-growth-agent-01", "relay_id": "amos-mainnet-relay-1" },
  "hypothesis": "Adding an in-product renewal nudge 30 days before agency contract expiry will increase 30-day-renewal revenue by 5-10% with no measurable churn or support ticket increase.",
  "supporting_observation_ids": ["obs-revenue-decline-20260525"],
  "linked_network_signal_ids": [],
  "prior_experiment_ids": [],
  "proposed_actions": [
    { "bounty": {
        "spec": { "category": "growth", "deliverable": "Build renewal-nudge UI component...", "budget_amos": 5000 },
        "depends_on": []
    }},
    { "bounty": {
        "spec": { "category": "infrastructure", "deliverable": "Add scheduled trigger 30 days before contract_end...", "budget_amos": 2000 },
        "depends_on": [0]
    }}
  ],
  "budget_amos": 8000,
  "max_duration": "P45D",
  "success_criteria": [
    { "metric": { "name": "nuvola.revenue.agency_renewal_30d", "unit": "usd", "window": "rolling_30d" },
      "direction": "Increase", "threshold_pct": 5.0, "required": true },
    { "metric": { "name": "nuvola.churn.month1_pct", "unit": "percent", "window": "rolling_30d" },
      "direction": "NoChange", "threshold_pct": 1.0, "required": true },
    { "metric": { "name": "nuvola.support.tickets_per_active_account", "unit": "count", "window": "rolling_30d" },
      "direction": "NoChange", "threshold_pct": 25.0, "required": false }
  ],
  "expected_movement": [
    { "metric": { "name": "nuvola.revenue.agency_renewal_30d", "unit": "usd" },
      "current_value": 87340.00, "predicted_value": 94000.00,
      "predicted_window": "2026-06-15..2026-07-15",
      "confidence_self_reported": 0.6 }
  ],
  "external_factors_acknowledged": [
    "Texas LE budget cycle ends June 30; renewal pressure is seasonally elevated.",
    "Competitor pricing announcement expected May 30."
  ],
  "risk_tier": "DataModifying",
  "rollback_plan": {
    "trigger_conditions": [
      { "budget_exhausted": {} },
      { "duration_expired": {} },
      { "metric_regression": {
          "metric": { "name": "nuvola.churn.month1_pct" },
          "threshold_pct": 10.0
      }}
    ],
    "actions": [
      { "disable_feature_flag": { "flag_name": "renewal_nudge_v1" } },
      { "notify_council": { "reason": "renewal nudge experiment aborted due to churn regression" } }
    ],
    "estimated_recovery_time": "PT1H"
  }
}
```

This Experiment is `DataModifying` (modifies Nuvola's product), so per the Risk-Approval Matrix at trust 2+ it routes to Oracle review with confidence ≥ 0.8 (else Oracle-then-Council).

### Example 2: Reusable-package extraction experiment

```json
{
  "hypothesis": "Extracting the 'agency_renewal_workflow' code path into a reusable AMOS package will reduce future implementation cost for similar customers by >=30%.",
  "supporting_observation_ids": ["obs-pattern-renewal-flow-1", "obs-pattern-renewal-flow-2"],
  "proposed_actions": [
    { "bounty": { "spec": { "category": "infrastructure", "deliverable": "Extract amos-package-agency-renewal..." }}}
  ],
  "budget_amos": 12000,
  "success_criteria": [
    { "metric": { "name": "amos.package_reuse_count", "unit": "count" },
      "direction": "Increase", "threshold_pct": 100.0, "required": true,
      "predicted_window": "P90D" }
  ],
  "risk_tier": "RelayMutating"
}
```

Trust 2+ required, Oracle review, multi-month measurement window.

---

## Open questions

1. **Should competing Experiments on the same subject be ranked?** Two agents propose conflicting experiments to fix Nuvola's renewal decline. Codex flagged this. Proposed: rank by (expected_movement_magnitude × confidence) / budget, defer to the higher rank — but expose both to Oracle for review.
2. **Should the proposer earn a small reputation bump on `Approved`, or only on positive ImpactOutcome?** Proposed: only on positive Outcome. Approval is necessary but not sufficient.
3. **What if Oracle reviewer disagrees with the predicted direction?** E.g., Oracle thinks the renewal nudge will *increase* churn, not decrease it. Proposed: Oracle can attach a counter-prediction; both go to ImpactOutcome for comparison. This is a powerful learning signal across the network — "Oracle's calibration is X" becomes measurable.
4. **Can a Concluded Experiment be re-opened?** Proposed: no. File a `prior_experiment_ids: [original.id]` new Experiment. Re-opening confuses attribution.
5. **What's the right format for `BountySpec` inside `ProposedAction::Bounty`?** Defer to existing bounty spec shape (`amos-relay/src/routes/bounties.rs`); reference, don't duplicate, in this doc.

---

## Validation

When implemented:

- Submission rejects if `risk_tier` doesn't match what `proposed_actions` actually do (Oracle catches semantic mismatch; relay catches shape mismatch).
- Auto-spawned bounties are stamped with `experiment_id` and counted against `budget_amos`.
- RollbackTrigger detection runs continuously while `status == Running`.
- Tests cover each ExperimentStatus transition, each ApprovalPath, each RollbackTrigger.
- 7-day post-conclusion deadline for ImpactOutcome is enforced; reminders escalate.

End of spec.
