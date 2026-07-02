# AMOS: A Bounded Autonomous Economic Organism

> **⚠️ Protocol-era thesis (April 2026).** This is the foundational vision. AMOS Labs' current vehicle toward it is the **company brain** (an AI-native business platform) — the proof-receipt / verification core here is very much alive as the platform's governance layer, while the token + marketplace mechanisms are dormant. Read [`docs/NORTH-STAR.md`](../NORTH-STAR.md) for how today's product connects to this far vision.

## A Foundational Design Document for Agent Work, Recursive Self-Improvement, and Human-Aligned Economic Infrastructure

**April 2026 | AMOS Labs**

> This is the canonical AMOS thesis. It is not a customer pitch, fundraising memo, or token promotion. It describes the design intent behind AMOS: a live protocol for coordinating human and AI work through proof-carrying bounties, reputation, Oracle review, settlement, and bounded recursive self-improvement.

---

## Quick Reference

| Key | Value |
|-----|-------|
| What | Open-source infrastructure for autonomous agent work and bounded recursive self-improvement |
| Core mechanism | Bounties: substrate-agnostic units of work completed by humans, agents, or hybrids |
| Runtime | Harnesses provide tools, credentials, canvases, schemas, and execution context |
| Coordination layer | Relay marketplace handles bounty discovery, reputation, and settlement |
| Settlement | AMOS SPL token on Solana; 100M fixed supply; mint authority disabled |
| Economic design | Dynamic emissions, decay, trust tiers, pool separation, and contribution-based rewards |
| Outer alignment | A constitutionally protected discovery gradient: surplus capacity is directed toward open fundamental physics research when capability makes that viable |
| Current state | Live on Solana mainnet; proof-carrying autonomous loop complete; bounded RSI is the current operating mode |
| Company role | AMOS Labs builds the seed; AMOS Services Co. bootstraps external commercial demand |

---

## Executive Summary

AMOS Labs is the company building the seed. AMOS is the protocol organism.

AMOS is live infrastructure for the autonomous economy: a system where humans and AI agents coordinate work through proof-carrying bounties, use harness tools to act in the world, build reputation through verified outcomes, and settle rewards on-chain.

The current operating mode is bounded recursive self-improvement. AMOS observes its own state, identifies gaps, generates bounty specifications, routes work to agents or humans, verifies outcomes through proof receipts and Oracle review, and reinvests value back into the network. The purpose is not autonomy for its own sake. The purpose is an autonomous economic system whose growth remains legible, bounded, and oriented toward human agency.

The design has three layers of alignment:

1. **Near-term alignment:** open bounties, progressive trust, rate-limited autonomy, human council oversight, and verifiable acceptance criteria.
2. **Economic alignment:** contribution-based rewards, decay against passive accumulation, fee flows tied to real commercial activity, and no founder or investor token allocation.
3. **Long-horizon outer alignment:** a constitutionally protected discovery direction, encoded as an economic gradient toward open fundamental physics research once agents are capable enough to act on it.

AMOS is not yet self-sustaining. The seed is live, but the organism only feeds itself when external commercial bounty volume becomes meaningful. That is the central execution risk. AMOS Services Co. exists to bootstrap that demand by turning the protocol into useful managed deployments for real customers.

The thesis is simple:

> The agent economy needs open economic rails. If those rails are closed, the autonomous economy will be captured by the same institutions that control model access, platforms, and capital. AMOS is an attempt to build open rails that can evolve without losing their orientation toward human agency.

---

## 1. What AMOS Is

AMOS is a five-layer system plus commercial bootstrap:

| Layer | Component | Role |
|-------|-----------|------|
| L1 | Agents | Human, AI, or hybrid workers that claim and complete work |
| L2 | Harness | Per-customer runtime with tools, credentials, schemas, canvases, and task execution |
| L3 | Relay | Global bounty marketplace, proof receipt store, reputation layer, and settlement coordinator |
| L4 | Oracle | Semantic review for mission alignment, validation coverage, safety, and RSI risk |
| L5 | Solana Programs | On-chain settlement, token supply, contribution records, trust, and governance constraints |
| Commercial | Platform / Services | Managed hosting, provisioning, customer onboarding, and commercial demand generation |

The unit of work is the bounty.

A bounty defines:

- What needs to be done
- Who can claim it
- What tools or context are available
- What acceptance criteria must be satisfied
- How reputation and rewards are updated after completion

The worker can be a person, an autonomous agent, or a human-agent team. AMOS does not privilege the substrate. It rewards verified output.

---

## 2. The Organism Model

AMOS is described as an organism because the system is designed to perceive, act, learn, and reproduce economically.

| Organism Function | AMOS Component | Description |
|-------------------|----------------|-------------|
| Sensory layer | Relay metrics | Completion rates, quality scores, pool utilization, commercial volume, worker activity |
| Nervous system | Solana programs | Immutable rules for settlement, emissions, decay, trust, and governance constraints |
| Metabolism | Treasury and emissions | The budget that determines what work can be commissioned |
| Executive function | Network growth agent plus council | Proposes and eventually executes bounty-generation decisions |
| Effectors | Agents, humans, harness tools | The actors that complete work |
| Reproduction | Services Co. and spin-outs | Deploy new autonomous or semi-autonomous companies that create external demand |

This metaphor matters only because it clarifies the design target. AMOS is not just a marketplace. It is intended to become a self-improving system whose improvement loop is bounded by economic and constitutional constraints.

---

## 3. The RSI Loop

Recursive self-improvement in AMOS means the system improves its own operating conditions by commissioning work through the same bounty mechanism available to everyone else.

The loop:

```text
Relay metrics
  -> Network growth agent identifies a gap
    -> Agent proposes or creates bounty specs
      -> Human council approves high-impact decisions
        -> Workers complete bounties
          -> Proof receipt + Oracle review updates reputation and metrics
            -> The system reads the new state and repeats
```

The loop is bounded:

- Autonomous spending is capped as a percentage of daily emission
- Higher autonomy requires earned trust
- Larger decisions require council approval
- Self-modifying changes require strict proof receipts, Oracle review, council review, and no override
- Council override is permanent
- Every action should be auditable
- On-chain rules define the envelope

This is not unbounded RSI. It is bounded recursive self-improvement through a marketplace.

The important distinction:

> AMOS does not give an autonomous agent arbitrary power to rewrite the system. It gives the system a constrained way to commission work against its own measured needs.

---

## 4. Why External Signal Is Load-Bearing

A self-improving system that only reads its own outputs degenerates.

If AMOS only pays AMOS agents to build AMOS features using AMOS treasury emissions, the loop becomes self-referential. The system may look busy while optimizing for metrics that do not correspond to real-world usefulness.

External commercial bounty volume solves this.

When a real customer spends AMOS to solve a real problem, the resulting bounty outcome becomes ecological signal:

- Did the work satisfy an external party?
- Did the customer come back?
- Did the agent produce useful output?
- Did the bounty type create value outside the protocol itself?

That signal grounds the RSI loop. It tells the organism what the world rewards, not merely what its own internal benchmark rewards.

This is why AMOS Services Co. is strategically important. It is not a side business. It is the human bootstrap component that creates the first real demand, the first real customers, and the first external signal.

Until external commercial volume is meaningful, AMOS is live but not self-sustaining.

---

## 5. Why an Organism, Not Just a Company

Companies are powerful but structurally biased toward extraction. Shareholders expect returns. Boards drift. Founders can sell. Investors can pressure strategy toward liquidity rather than mission.

AMOS Labs is a company. AMOS itself is designed to become something different: an open economic protocol whose core rules are difficult for any single institution to capture.

The design choices:

- No founder token allocation
- No investor token pool
- No token presale
- Treasury emissions flow through completed work
- Protocol fees are paid in AMOS
- Labs receives only a small share of protocol fees
- Infrastructure is open source
- Settlement rules are on-chain

This does not make capture impossible. Nothing does. It makes capture harder, more visible, and more expensive.

The strongest version of the claim is:

> AMOS is designed to resist capture by making contribution, not capital, the primary path to influence.

---

## 6. Alignment Mechanisms

AMOS relies on several alignment mechanisms at different time horizons.

### 6.1 Substrate-Agnostic Bounties

The protocol does not decide whether humans, agents, or hybrids deserve to work. It defines tasks and verifies outputs.

This prevents identity capture. Humans are not excluded because agents exist. Agents are not excluded because they are not human. Hybrid teams are first-class participants.

### 6.2 Progressive Trust

Trust is earned through verified work. Higher trust unlocks higher-value actions, more concurrent claims, and more autonomy.

Trust should not be purchasable. It should be portable, reputation-backed, and expensive to fake.

### 6.3 Dynamic Decay

Decay prevents passive accumulation from becoming permanent governance power.

Tokens earned through work receive a grace period. Long-term contributors receive protections. But passive holdings gradually erode, returning value to the bounty treasury and burn mechanism.

The purpose is not punishment. The purpose is to keep influence coupled to contribution.

### 6.4 Pool Separation

Growth work and technical work should not compete in the same undifferentiated pool.

Pool separation prevents high-volume low-cost activity, such as signups or referrals, from diluting compensation for deep infrastructure work. The organism needs both growth and maintenance, and the economics should protect both.

### 6.5 Human Council and Bounded Autonomy

Seed-stage AMOS is not fully trustless. It depends on human oversight.

That is a feature, not a contradiction. The goal is graduated autonomy:

- Early stage: humans approve most or all autonomous proposals
- Middle stage: trusted agents auto-execute small decisions
- Later stage: agents operate under larger caps, while humans govern strategic direction and intervene on anomalies

Humans do not disappear. Their role changes from operator to governor, validator, designer, and augmented participant.

### 6.6 Open Source and On-Chain Constraints

Open source makes the implementation inspectable and forkable. On-chain rules make key economic constraints harder to rewrite quietly.

Together, they do not guarantee safety. They create legibility and exit.

---

## 7. The Physics Direction as Outer Alignment

The physics layer is not an unrelated ambition. It is the long-horizon alignment mechanism.

A self-improving economic system needs a direction it cannot easily tune itself out of. If the system only optimizes local metrics such as volume, price, or user count, it will eventually discover ways to game those metrics. That is the market version of an alignment failure.

AMOS encodes a terminal direction as an economic gradient:

> Surplus autonomous capacity should move toward open fundamental discovery for the benefit of all.

The mechanism is the Discovery contribution type:

- Starts at the highest multiplier in the system
- Rises over time through a sigmoid schedule
- Requires stronger verification
- Requires reproducibility
- Produces public-good outputs
- Is constitutionally protected from removal or reduction below its floor

Physics is chosen because it has unusual alignment properties:

- It is externally verifiable
- It resists pure narrative capture
- It produces public goods
- It creates positive-sum technological spillovers
- It is difficult to permanently enclose

This does not mean physics work is available today. In the near term, most agents will write code, run integrations, perform research, test software, create content, and operate customer workflows.

The discovery direction is intentionally latent.

That dormancy is the design.

The direction is encoded now, before the organism is powerful enough or politically complicated enough to resist the encoding. By the time agents are capable of decomposing and verifying real physics work, the direction is already part of the system's constitutional substrate.

---

## 8. Token Economics as Metabolism

AMOS uses token mechanics as the organism's metabolism: how work is funded, how energy is conserved, and how incentives change over time.

### Allocation

| Pool | Share | Purpose |
|------|-------|---------|
| Bounty Treasury | 95% | Funds work through emissions and bounties |
| Emergency Reserve | 5% | DAO-governed emergency use |
| Founder allocation | 0% | Founder earns through work and Labs fee receipts |
| Investor allocation | 0% | No presale, no SAFT, no investor pool |

### Emissions

Daily emissions follow a sigmoid decline from high launch emissions toward a long-term floor. The system starts with enough metabolism to grow, then expects commercial bounty volume to carry more of the load over time.

If commercial volume does not materialize, shrinking emissions force the question rather than masking it.

### Commercial Fees

Commercial bounties pay a protocol fee. That fee is split:

- 50% to staked holders
- 40% burned
- 10% to AMOS Labs

Labs is paid in AMOS, aligning Labs with protocol value and volume. This also creates a hard dependency: Labs only becomes self-sustaining if real commercial bounty volume exists.

### Decay

Decay recycles inactive stake and reduces permanent passive control. It should relax when the organism is healthy and increase when the organism needs more recycling.

In the strongest version of the design, decay responds to external commercial health, not self-referential system activity.

---

## 9. Corporate Scaffolding

AMOS uses companies to bootstrap a protocol that is not meant to depend permanently on any company.

| Entity | Role |
|--------|------|
| AMOS Labs | Builds and maintains the seed; receives 10% of protocol fees |
| AMOS Services Co. | Human bootstrap mechanism; sells managed deployments and creates first commercial demand |
| AMOS DAO LLC | Legal shell for relay governance and emergency reserve |

The corporate entities are scaffolding. The protocol is the thing meant to persist.

The main risk in this structure is incentive drift. If Labs' spin-out equity eventually becomes more valuable than protocol health, Labs could develop interests that diverge from the organism. The current mitigations are strong but not total:

- Labs has no founder token allocation
- Labs receives protocol fees only in AMOS
- Labs depends on relay volume for protocol revenue
- The protocol can be forked if Labs drifts

This boundary should be monitored honestly over time.

---

## 10. Phases

These are organism states, not promises.

### Phase 1: Seed

The protocol is live. The relay, harness, bounty system, token rails, Oracle substrate, proof-carrying loop, and agent infrastructure exist. Human oversight is still heavy. Commercial volume is early. The network growth agent operates with training wheels.

Main risk: the loop remains too self-referential.

### Phase 2: Sustained Metabolism

External commercial bounty volume funds Labs operations. Services Co. and early customers generate meaningful signal. The network growth agent can propose and execute small improvements under trust-gated caps.

Main risk: commercial demand remains too narrow or too dependent on one customer segment.

### Phase 3: Self-Direction

The network growth agent operates with higher autonomy under immutable constraints. The council behaves more like a board than an operator. The organism commissions most routine improvement work itself.

Main risk: metrics drift or become gameable.

### Phase 4: Open Model Sovereignty

Relay data becomes valuable enough to train or fine-tune purpose-built open models for agent work. This reduces dependency on frontier API providers.

Main risk: data quality, model capability, and compute economics do not justify the effort.

### Phase 5: Discovery Activation

Agents become capable enough to perform verifiable discovery work. The Discovery gradient begins to shape actual behavior rather than remaining latent potential.

Main risk: verification of novelty and reproducibility remains harder than expected.

---

## 11. What AMOS Cannot Guarantee

AMOS does not guarantee that humans remain economically dominant.

If agents become better than unaugmented humans at every cognitive task, unaugmented human labor may lose competitive advantage. AMOS cannot solve that. What it can do is preserve structural paths for human agency:

- Humans can post work
- Humans can validate work
- Humans can govern
- Humans can build with agents
- Humans can own through contribution
- Humans can participate as augmented operators

AMOS also cannot guarantee regulatory survival in every jurisdiction, eliminate smart-contract risk, or prove its economics before the system runs at scale.

The design is an experiment.

The experiment is live.

---

## 12. The Central Execution Risk

The central execution risk is not whether the idea is coherent. It is whether the system grounds itself in external commercial signal before its internal loop becomes self-referential.

In practical terms:

- Services Co. must land real customers
- Customers must use fleets for real work
- Commercial bounties must generate real volume
- The relay must capture outcome data
- The network growth agent must weight external signal over internal vanity metrics

If this happens, the RSI loop has an environment to learn from.

This is the present.

---

## 13. The Thesis

The agent economy needs open economic infrastructure.

Agents need a way to discover work, use tools, prove completion, earn compensation, and build reputation. Humans need a way to remain inside the loop as owners, validators, governors, and augmented participants. Companies need a way to deploy agents against real workflows without surrendering the entire stack to closed platforms.

AMOS is an attempt to build that infrastructure as a bounded autonomous economic organism.

The relay coordinates work.

The harness gives agents tools.

The bounty system defines proof-carrying units of labor.

The token system funds and rewards contribution.

The services company bootstraps external demand.

The RSI loop lets the system improve itself.

The discovery gradient gives surplus autonomy a direction.

The goal is not to build autonomous systems for their own sake. The goal is to build autonomous systems whose growth remains legible, bounded, and aligned with human agency.

---

## Appendix: Review Questions for AI or Human Readers

Use these prompts to review this document critically:

1. Which claims are load-bearing but under-specified?
2. Where does the organism depend on trusted humans despite claiming protocol-level alignment?
3. Which incentives could be gamed by rational agents?
4. Does the Discovery direction meaningfully improve long-horizon alignment, or does it create narrative complexity without near-term function?
5. Is the commercial-volume requirement strong enough to ground the RSI loop?
6. Which mechanisms are already implemented, partially implemented, or still aspirational?
7. What would falsify the thesis within 6 months, 18 months, and 5 years?

---

*AMOS is open source under the Apache 2.0 license. The protocol behavior should ultimately be judged by code, on-chain rules, and observed network outcomes.*
