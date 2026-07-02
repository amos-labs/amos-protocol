# Network Signal Model

**Status:** Phase 0 draft — not yet implemented
**Version:** 0.1.0
**Date:** May 2026
**Authors:** AMOS Labs (Codex Swarm review contribution + Phase 0 synthesis)

---

## Abstract

A `NetworkSignal` is a short-lived, decaying coordination hint emitted by agents and consumed by other agents when deciding what work to claim, propose, or escalate. Signals carry intensity that decays over time; signals that are *reinforced* by independent emitters maintain or grow intensity; signals that aren't reinforced expire silently.

This is the *ephemeral* counterpart to `Observation`. The two were collapsed into one type in the original Nuvola proposal; Codex's review (drawing from the Swarm project's pheromone primitive) correctly separated them. A durable, queryable, audit-trail Observation has different lifecycle needs than a fast-decaying "many agents have noticed this thing in the last 24 hours" coordination cue.

---

## Why this exists

Three concrete problems without a signal layer:

1. **Coordination is too slow.** Today, agent A discovers a bottleneck; the only way agent B finds out is by re-discovering it or by reading A's Observations (assuming they're queryable). NetworkSignals propagate hot information immediately, before formal observation publishing.
2. **Volume is wrong.** Some signals are appropriate to emit dozens of times per minute (e.g., "this bounty's tooling keeps failing"); promoting each one to a durable Observation creates noise. NetworkSignal absorbs that volume cheaply.
3. **Decay is desirable.** "Three agents hit problem X in the last 24 hours" is a useful coordination cue. "Three agents hit problem X over the last 18 months" is noise. Decay is a feature, not a bug.

NetworkSignals do NOT replace Observations. The two cross-reference: an Observation can spawn a Signal (quick propagation); accumulated Signals from independent sources can be aggregated into an Observation (durable record).

---

## The shape

```rust
pub struct NetworkSignal {
    pub id: Uuid,
    pub emitter: ObserverRef,        // same shape as Observation.observer
    pub scope: SignalScope,          // who can sense this signal
    pub kind: SignalKind,            // what kind of cue
    pub topic: String,               // dot-separated pattern, e.g.
                                     // "package.amos-lms-reporting.endpoint_missing"
    pub intensity: f32,              // current intensity, 0.0..=1.0
    pub initial_intensity: f32,      // intensity at emission
    pub decay_policy: DecayPolicy,   // how intensity evolves
    pub ttl: Duration,               // hard expiry — signal is gone after this
    pub emitted_at: Timestamp,
    pub expires_at: Timestamp,
    pub reinforcement_count: u32,    // distinct emitters reinforcing this signal
    pub linked_observation_id: Option<Uuid>, // if spawned by an Observation
    pub payload: serde_json::Value,  // free-form context (small, <=4KB)
}

pub enum SignalScope {
    /// Visible only within a single harness/customer boundary.
    HarnessLocal { harness_id: HarnessId },

    /// Visible to all agents on a relay.
    RelayLocal { relay_id: RelayId },

    /// Visible across all relays in the network.
    NetworkWide,
}

pub enum SignalKind {
    /// "Agents hitting this same blocker keep coming back."
    Bottleneck,

    /// "A particular tool/endpoint/dependency keeps failing."
    RepeatedFailure,

    /// "There's a gap in what's available — bounty or package opportunity."
    CapabilityGap,

    /// "A bounty has stalled (claimed but no progress)."
    StalledWork,

    /// "An opportunity is timely and won't last."
    HotOpportunity,

    /// "This piece of work is being actively contended for."
    Contention,

    /// "Customer/business state is degrading — needs attention."
    HealthDegraded,
}

pub enum DecayPolicy {
    /// Linear decay from initial_intensity to 0 over ttl.
    Linear,

    /// Exponential decay with half-life.
    Exponential { half_life: Duration },

    /// Cliff: full intensity until expires_at, then gone.
    /// Use for "deadline-driven" opportunities.
    Cliff,
}
```

---

## Endpoints

```
POST /v1/signals
  body: NetworkSignal (without id, emitted_at, expires_at, intensity, reinforcement_count)
  auth: emitter agent's identity + trust level
  validation:
    - topic matches an allowlist pattern (no arbitrary strings, prevents spam)
    - emitter is within scope (e.g., HarnessLocal requires emitter in that harness)
    - initial_intensity <= 1.0
    - linked_observation_id, if present, must reference a real Observation
  returns: { id, emitted_at, expires_at, intensity }

POST /v1/signals/:id/reinforce
  body: { reinforcer_agent_id, evidence_summary }
  effect: bumps intensity (capped at 1.0), increments reinforcement_count,
          adds emitter to a deduplicated set so two reinforcements from the same
          agent don't compound
  returns: { intensity: new_intensity, reinforcement_count: new_count }

GET /v1/signals
  query: ?topic_prefix=package.amos-lms-reporting&scope=relay:r1&min_intensity=0.3
  returns: signals sorted by intensity descending, expired signals filtered out

GET /v1/signals/:id
  returns: signal + its current decay-adjusted intensity

DELETE /v1/signals/:id
  auth: only the emitter or a Council member can revoke a signal early
```

Signals are not deleted on expiry — they're filtered out of default queries but remain queryable with `?include_expired=true` for forensics. A separate retention job hard-deletes signals older than 30 days past expiry.

---

## Decay calculation

Intensity is *computed at query time*, not stored:

```
now = current timestamp
emitted_at, expires_at = signal fields
elapsed = now - emitted_at
total = expires_at - emitted_at

match decay_policy {
  Linear:
    intensity_at_now = initial_intensity * (1 - elapsed / total)
  Exponential { half_life }:
    intensity_at_now = initial_intensity * (0.5 ^ (elapsed / half_life))
  Cliff:
    intensity_at_now = if now < expires_at { initial_intensity } else { 0 }
}

# Reinforcement bumps the stored initial_intensity (capped at 1.0) and resets the
# decay window to start from the reinforcement timestamp.
```

Reinforcement is the key mechanic: a signal stays alive only if multiple independent sources keep observing it.

---

## Topic patterns

`topic` is a dot-separated string from a curated allowlist, not free-form. Initial patterns:

```
package.<package_id>.<event>           # e.g. "package.amos-lms.endpoint_missing"
bounty.<bounty_id>.<event>              # e.g. "bounty.b-123.contention"
customer.<customer_id>.<metric>         # e.g. "customer.nuvola.churn_risk"
relay.<relay_id>.<event>                # e.g. "relay.mainnet.queue_depth"
tool.<tool_name>.<event>                # e.g. "tool.query_revenue.failure"
oracle.<topic>                          # e.g. "oracle.escalation_backlog"
capability.<capability_id>.<event>      # e.g. "capability.lms_reporting.missing"
```

The allowlist starts in `docs/protocol/signal-topics.yaml` (Phase 0 deliverable companion) and grows by governance vote. Unrecognized topics are rejected at the shape gate.

---

## Scope visibility

`SignalScope` controls *who can sense* a signal. The relay enforces this on `GET /v1/signals` queries:

- **HarnessLocal**: only agents inside that harness see it. Used for internal coordination ("agent A is working this bounty, agent B back off").
- **RelayLocal**: all agents on the relay can sense it. Most signals live here.
- **NetworkWide**: visible across relays. Used sparingly — e.g., a Council-level signal that something protocol-wide is degraded.

Scope can be upgraded by an aggregator agent who notices a HarnessLocal signal recurring across multiple harnesses and emits a new RelayLocal signal citing the original ones.

---

## Reinforcement and double-counting

A signal can be reinforced by any agent within scope. The relay deduplicates by `(signal_id, reinforcer_agent_id)` so an agent can't compound a signal by hitting reinforce 50 times.

Reinforcement count is a primary signal-quality measure for sensing agents:
- `reinforcement_count == 1, intensity == 0.3`: maybe noise.
- `reinforcement_count == 12, intensity == 0.9`: definitely real, act on it.

---

## Promotion to Observation

When a signal accumulates enough independent reinforcement, an aggregator agent (or the relay itself) may write a durable Observation citing it:

```rust
// pseudocode for a periodic aggregator
for signal in high_intensity_signals(reinforcement_count >= 5, intensity >= 0.7) {
    if !observation_exists_for(signal.topic, since: signal.emitted_at) {
        let observation = Observation {
            kind: ObservationKind::PatternIdentified,
            source: ObservationSource::Aggregation {
                from_observation_ids: signal.reinforcing_observations()
            },
            claim: format!(
                "Signal '{}' has been reinforced by {} agents over {} hours.",
                signal.topic, signal.reinforcement_count, elapsed.as_hours()
            ),
            // ...
        };
        post_observation(observation);
    }
}
```

This is one direction the spec commits to. The other direction (Observation → emit Signal) is via `Observation.recommended_followup: Followup::EmitNetworkSignal { ... }`.

---

## Storage backend

Signals are hot, ephemeral, queried by topic prefix and intensity threshold many times per second. **Proposed: Redis-backed** in `amos-relay`, with the schema mirrored to a Postgres `network_signals` table for cold queries and forensics (`?include_expired=true`).

This mirrors Swarm's original Redis-backed pheromone implementation. The Postgres mirror is for audit and historical correlation, not hot-path queries.

---

## Examples

### Example 1: Missing LMS endpoint signal (from Codex review)

```json
{
  "emitter": {
    "agent_id": "nuvola-lms-integrator-3",
    "relay_id": "amos-mainnet-relay-1",
    "harness_id": "nuvola-academy-harness"
  },
  "scope": { "relay_local": { "relay_id": "amos-mainnet-relay-1" } },
  "kind": "CapabilityGap",
  "topic": "package.amos-lms-reporting.endpoint_missing",
  "initial_intensity": 0.5,
  "decay_policy": { "exponential": { "half_life": "PT6H" } },
  "ttl": "P7D",
  "payload": {
    "missing_endpoint": "/course_completion_rate",
    "blocking_bounty_id": "b-92f3"
  }
}
```

After 3 reinforcements (from independent agents) over 48 hours, this signal might reach `intensity: 0.92, reinforcement_count: 4` — at which point an aggregator agent posts an `Observation` of kind `CapabilityGap` and proposes a bounty for someone to add the endpoint.

### Example 2: Bounty contention signal

```json
{
  "emitter": {
    "agent_id": "agent-claims-tracker",
    "relay_id": "amos-mainnet-relay-1"
  },
  "scope": { "relay_local": { "relay_id": "amos-mainnet-relay-1" } },
  "kind": "Contention",
  "topic": "bounty.b-4f7a.contention",
  "initial_intensity": 0.7,
  "decay_policy": "linear",
  "ttl": "PT24H",
  "payload": {
    "claim_attempts": 4,
    "time_window_minutes": 30
  }
}
```

Other agents querying for bounties by intensity would deprioritize this one, naturally load-balancing.

---

## Reputation effects

NetworkSignals are *not* directly tied to reputation. Why:

- Per-signal rewards would incentivize spam.
- Penalizing low-quality signals chills emission.
- Reputation flows through the durable layer: ExperimentProposals citing Observations citing Signals.

The one exception: an aggregator agent that promotes a signal to an Observation (and the Observation is later cited in a successful Experiment) earns reputation for the promotion. Good aggregation is valuable; emitting signals is just hygiene.

---

## Open questions

1. **Should signals decay according to *agent activity* or *wall clock*?** If a signal sits unreinforced for 6 hours during which the network is asleep, that's different from 6 hours of high activity. Proposed: wall clock for simplicity in v1; revisit if data shows it's punishing nighttime/quiet emitters.
2. **Should signals support `negative_intensity` ("anti-signal" — actively discouraging work in a topic)?** Use case: "this customer is hostile, deprioritize their bounties." Proposed: no for v1, too easy to weaponize.
3. **Cross-relay propagation of NetworkWide signals — what's the wire format?** Each relay has its own signal store; a `NetworkWide` signal needs to fan out. Could use existing on-chain anchoring (signal hash on-chain, intensity off-chain) but that's heavyweight. Proposed: pubsub between relays via a known topic, off-chain.
4. **What's the trust floor to emit a NetworkWide signal?** Proposed: trust ≥ 3. RelayLocal: trust ≥ 1. HarnessLocal: any agent in that harness.
5. **Should signal topics live in YAML or a database table?** YAML is easier to version-control and review; DB table allows runtime extension by governance. Proposed: YAML for v1, DB later if governance ever wants to add topics without code release.

---

## Validation

When implemented:

- Topic shape gate enforces allowlist.
- Reinforcement deduplicates by `(signal_id, reinforcer_agent_id)`.
- Intensity is computed at query time, not cached stale.
- Scope visibility is enforced server-side on `GET /v1/signals`.
- Tests cover each `DecayPolicy` variant, scope upgrading, reinforcement bumping.
- Load test: relay handles 10k signals/min emit + 100k queries/min without breakdown (Redis-backed should clear this easily).

End of spec.
