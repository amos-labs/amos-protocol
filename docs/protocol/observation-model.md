# Observation Model

**Status:** Phase 0 draft — not yet implemented
**Version:** 0.1.0
**Date:** May 2026
**Authors:** AMOS Labs (synthesizing Nuvola Claude proposal + Codex review)

---

## Abstract

An `Observation` is a typed, durable, queryable claim about a system's state — a customer's business, a relay's health, an Oracle's reasoning quality, a Solana program's behavior. Today agents observe state *through tools* but the output of observation is not a protocol object: you can't queue, replay, share across relays, or reason over a history.

Observations close that gap. They are the *durable* half of the upstream measurement layer; ephemeral coordination hints live in `NetworkSignal` (separate spec) so the two don't collapse into one type with conflicting lifecycle requirements.

---

## Why this exists

Three concrete things break without Observations:

1. **No history of business state changes** — when an agent later proposes an `ExperimentProposal`, it needs to cite *what observation made this seem worth testing*. Today there's nothing durable to cite.
2. **No cross-agent visibility** — Agent A notices something about a customer; Agent B has no way to know without re-running A's tools. Observations are queryable artifacts that propagate the signal.
3. **No tamper-evident audit trail** — the proof-carrying loop demands receipts for *work*. State observations deserve the same hygiene: who observed what, when, with what evidence.

Observations are NOT a substitute for proof receipts. Receipts describe *work performed*; Observations describe *state perceived*. Different protocol layers, different lifecycles.

---

## The shape

```rust
pub struct Observation {
    pub id: Uuid,
    pub observer: ObserverRef,           // who/what made the observation
    pub subject: ObservationSubject,     // what was observed
    pub source: ObservationSource,       // how it was observed
    pub kind: ObservationKind,           // type of claim being made
    pub metric: Option<MetricRef>,       // optional numeric claim
    pub claim: String,                   // human-readable, <= 500 chars
    pub evidence: EvidenceChain,         // pointers to logs, queries, artifacts
    pub evidence_strength: EvidenceStrength, // measurable signal quality
    pub recommended_followup: Followup,  // what (if anything) to do about it
    pub share_anonymized: bool,          // opt-in for cross-customer learning
    pub observed_at: Timestamp,
    pub created_at: Timestamp,
}

pub struct ObserverRef {
    pub agent_id: AgentId,
    pub relay_id: RelayId,
    pub harness_id: Option<HarnessId>,
}

pub enum ObservationSubject {
    Customer { customer_id: CustomerId, scope: CustomerScope },
    Relay    { relay_id: RelayId },
    Oracle   { oracle_id: OracleId },
    Program  { program_id: SolanaProgramId },
    SelfAgent { agent_id: AgentId },
    Package  { package_id: PackageId },
}

pub enum ObservationSource {
    HarnessTool   { tool_name: String, invocation_id: Uuid },
    OracleLog     { oracle_review_id: Uuid },
    AgentReasoning { agent_loop_iteration: u32 },
    ExternalApi   { provider: String, endpoint: String },
    OnChain       { tx_signature: String, slot: u64 },
    Aggregation   { from_observation_ids: Vec<Uuid> },
}

pub enum ObservationKind {
    /// A specific metric moved or is at a stated value.
    /// Pair with `metric: Some(...)`.
    MetricState,

    /// A change vs. a previous Observation or baseline.
    /// Pair with `metric: Some(...)` and reference the prior in evidence.
    MetricDelta,

    /// Something unusual happened — outside expected envelope.
    AnomalyDetected,

    /// A required capability or tool is missing for work that's expected.
    CapabilityGap,

    /// A dependency (package, service, integration) is missing or broken.
    DependencyMissing,

    /// A pattern across multiple lower-level observations.
    /// Pair with `source: Aggregation { ... }`.
    PatternIdentified,

    /// A qualitative claim that doesn't fit the above.
    Qualitative,
}

pub struct MetricRef {
    pub name: String,             // e.g. "nuvola.revenue.mrr_usd"
    pub value: f64,
    pub unit: String,             // "usd", "count", "percent", "seconds"
    pub baseline: Option<f64>,    // for MetricDelta — prior value
    pub window: TimeRange,        // measurement window
}

pub struct EvidenceChain {
    pub primary_uri: Url,         // log, screenshot, query result
    pub corroborating_uris: Vec<Url>,
    pub artifact_hashes: Vec<String>, // sha256 of fetched artifacts
}

/// Measurable signal quality. Replaces a self-graded confidence float
/// with quantities the protocol can verify.
pub struct EvidenceStrength {
    pub signal_count: u32,        // number of supporting data points
    pub distinct_sources: u32,    // count of distinct evidence sources
    pub replicated_at: Vec<Timestamp>, // if observed multiple times
    pub freshness_seconds: u64,   // how old the underlying data is
}

pub enum Followup {
    Archive,                                  // log and move on
    Monitor { recheck_after: Duration },
    EmitNetworkSignal { intensity: f32, ttl: Duration }, // soft propagation
    CommissionBounty { suggested_scope: ScopeClassification },
    ProposeExperiment { hypothesis_seed: String },
    EscalateToOracle,
    EscalateToCouncil,
}
```

---

## Endpoints

```
POST /v1/observations
  body: Observation (without id, created_at — server fills)
  auth: observer agent's identity + trust level
  validation:
    - source matches observer (e.g., HarnessTool requires the tool was actually invoked)
    - evidence URIs are reachable from the relay
    - share_anonymized=true requires customer consent flag set on tenant
  returns: { id, created_at, ack }

GET /v1/observations
  query: ?subject=customer:nuvola-academy&kind=MetricDelta&since=2026-05-01
  auth: caller must have visibility into the subject
        (own observations always; cross-customer requires share_anonymized=true)
  returns: paginated list, newest first

GET /v1/observations/:id
  returns: full Observation + evidence chain

POST /v1/observations/:id/aggregate
  body: { aggregate_into_kind: ObservationKind, additional_ids: Vec<Uuid> }
  produces: a new Observation with source=Aggregation
  use case: agent notices a pattern across N prior observations
```

Observations are immutable after creation. Wrong observation? File a corrective one with `evidence: { corroborating: [prior.id] }` and let the system see both.

---

## Interaction with Oracle

Observations are **not Oracle-reviewed by default**. Volume would make it cost-prohibitive (some agents emit dozens per minute during active investigation).

Oracle is invoked only when:
- An Observation's `recommended_followup` is `EscalateToOracle`.
- An `ExperimentProposal` cites this Observation as supporting evidence, and the experiment's risk tier requires Oracle review.
- An automated audit flags the Observation as a likely fabrication (high downstream-impact + zero evidence count).

This decision intentionally lets Observations be cheap. Cost-of-emission shapes how often agents observe; making them Oracle-gated would suppress the very thing we want — fine-grained state awareness.

---

## Interaction with NetworkSignal

`Observation` and `NetworkSignal` are deliberately different shapes for different lifecycles. Rough mapping:

- Observation is durable, queryable, and tamper-evident. Lifecycle: created, indefinitely retained, possibly aggregated.
- NetworkSignal is ephemeral, decaying, intensity-weighted. Lifecycle: emit, decay, expire.

They cross-reference via `Followup::EmitNetworkSignal`: an Observation can spawn a NetworkSignal as a quick coordination cue. Conversely, when a NetworkSignal accumulates enough intensity from independent sources, an aggregator agent may write a durable Observation citing the signal as evidence.

See `network-signal-model.md`.

---

## Reputation effects

Observations *can* affect reputation, but mostly negatively and lazily:

- **Fabrication penalty**: if an Observation is cited as evidence in an ExperimentProposal whose ImpactOutcome later proves the observation was false (or fabricated), the observer takes a reputation hit.
- **High-signal bonus**: open question (see below). Rewarding good observations creates an incentive to spam.

**Proposed default**: no positive reward for Observations themselves. Reward flows when an Observation leads to an Experiment that produces a measured ImpactOutcome. Observations are a means, not an end.

---

## Privacy and cross-customer propagation

- `share_anonymized: false` (default): Observation is visible only to the observer's relay and the subject's tenant.
- `share_anonymized: true`: subject identifiers are scrubbed and metric values bucketed before propagation across relays. Requires explicit consent at the tenant level (not per-observation — too easy to misclick).

Bucketing rules (initial proposal):
- Revenue metrics: bucketed to nearest order of magnitude.
- Engagement counts: bucketed to {<10, 10-100, 100-1k, 1k-10k, 10k+}.
- Custom metrics: scrubbed entirely unless a relay-side allowlist passes them.

Full spec lives in Cross-Customer Learning (Phase 5).

---

## Examples

### Example 1: Nuvola revenue decline observation

```json
{
  "observer": {
    "agent_id": "nuvola-growth-agent-01",
    "relay_id": "amos-mainnet-relay-1",
    "harness_id": "nuvola-academy-harness"
  },
  "subject": {
    "customer": {
      "customer_id": "nuvola-academy",
      "scope": "revenue"
    }
  },
  "source": {
    "harness_tool": {
      "tool_name": "query_revenue_snapshots",
      "invocation_id": "01HXY..."
    }
  },
  "kind": "MetricDelta",
  "metric": {
    "name": "nuvola.revenue.agency_renewal_30d",
    "value": 87340.00,
    "unit": "usd",
    "baseline": 99250.00,
    "window": "2026-04-25T00:00:00Z..2026-05-25T00:00:00Z"
  },
  "claim": "Agency renewal revenue declined 12% over the trailing 30 days vs the prior 30-day window.",
  "evidence": {
    "primary_uri": "s3://nuvola-audit/queries/01HXY...result.json",
    "artifact_hashes": ["sha256:..."]
  },
  "evidence_strength": {
    "signal_count": 1,
    "distinct_sources": 1,
    "replicated_at": [],
    "freshness_seconds": 3600
  },
  "recommended_followup": {
    "propose_experiment": {
      "hypothesis_seed": "Renewal decline correlates with onboarding latency increase observed in helpdesk tickets"
    }
  },
  "share_anonymized": false,
  "observed_at": "2026-05-25T18:00:00Z"
}
```

### Example 2: Cross-agent pattern observation

```json
{
  "observer": {
    "agent_id": "amos-network-intel-aggregator",
    "relay_id": "amos-mainnet-relay-1"
  },
  "subject": {
    "package": { "package_id": "amos-package-lms-reporting" }
  },
  "source": {
    "aggregation": {
      "from_observation_ids": ["obs-001", "obs-002", "obs-003", "obs-004"]
    }
  },
  "kind": "PatternIdentified",
  "claim": "Four agents across three customers hit the same missing 'course_completion_rate' endpoint this week.",
  "evidence": {
    "primary_uri": "https://relay.amoslabs.com/observations/aggregation/01HXZ...",
    "corroborating_uris": []
  },
  "evidence_strength": {
    "signal_count": 4,
    "distinct_sources": 3,
    "replicated_at": [
      "2026-05-22T09:00:00Z",
      "2026-05-23T14:30:00Z",
      "2026-05-24T11:15:00Z",
      "2026-05-25T08:00:00Z"
    ],
    "freshness_seconds": 600
  },
  "recommended_followup": {
    "commission_bounty": { "suggested_scope": "DataModifying" }
  },
  "share_anonymized": true,
  "observed_at": "2026-05-25T16:00:00Z"
}
```

---

## Open questions

1. **Should there be a positive reward for high-quality Observations?** Codex flagged this. Proposed default: no — reward flows through Experiments/Outcomes. But this can change if data shows Observations are under-emitted.
2. **On-chain anchoring?** Probably no — too high-volume, too expensive. A periodic batched hash of the day's Observations could go on-chain for tamper-evidence; per-observation settlement would not scale.
3. **What's the retention policy?** Forever feels wrong (storage + privacy); deleting Observations breaks the ability to audit past ExperimentProposals that cite them. Proposed: indefinite retention for Observations that ImpactOutcomes cite; 18-month TTL for orphans.
4. **How do we prevent observation spam?** Per-agent rate limits keyed by `(observer, subject, kind)` — e.g., max one `MetricState` per 60s per metric. Detailed numbers in implementation.
5. **Should `recommended_followup` be enforced or advisory?** Proposed: advisory. The observer suggests; downstream agents choose. Forcing a followup couples emission and action too tightly.

---

## Validation

When implemented:

- Observations are immutable after `POST /v1/observations` succeeds.
- Shape gate validates `source` matches the claimed origin (HarnessTool requires a matching invocation log; OnChain requires the tx_signature resolves).
- Tests cover: each ObservationKind round-trips, each Followup variant lookups correctly, `share_anonymized=true` requires tenant consent.
- Cross-customer propagation tests assert subject scrubbing and metric bucketing.

End of spec.
