# AMOS Labs — North Star

**Rev B · 2026-07-01 · Internal compass. Not a roadmap. Read before any decision that trades short-term revenue against the far goal.**

## The mission: capture-resistant rails

An economy where work is priced by **verified outcomes**, not labor time. When AI does the work, **verification becomes the scarce good** — and whoever controls the gate that verified work flows through controls the rails. Concentration is the *default* for digital rails (payments, app stores, cloud, frontier models); rails set early rarely reset. The mission is to make the rails of the agent economy **capture-resistant** — including against AMOS itself.

The atomic unit is the **proof receipt**: intent, policy, validation plan, evidence, verdict — one portable, auditable object.

## The current vehicle: the company brain

Businesses live in AMOS — apps, data, engines — operated by whatever AI they already use, with proofgate on every move.

Two reasons this *is* the mission, not a detour from it:
1. **It's the demand engine the April protocol lacked.** Neutral rails can't be *declared* at genesis; neutrality is *earned* through adoption by parties who distrust each other. Dead rails protect no one. Volume was always going to come from the boring commercial path first — the pivot is how the mission gets ammunition.
2. **It fixes what actually hobbles AI.** The real problem in any business isn't that the software doesn't integrate — it's that **no one can hold a coherent picture of the whole business at once, not even the CEO.** That same fragmentation starves the AI: a model is only as good as the context it's given. Put the whole business in one governed place — one brain — and the AI finally sees the entire problem at once, and starts forming strategy instead of doing isolated tasks. Every tenant, every receipt, every reseller compounds toward the far goal.

## The stack, decomposed by lock-in clock

Not all rails lock in on the same clock — and the capture-resistant moves that *can't wait* are the early-locking ones.

**Early lock-in — keep OPEN NOW, as standing policy (cheap today, irreversible if missed):**
- **Receipt schema = an open standard** anyone can implement (see `docs/protocol/receipt-schema.md`). The one genuinely irreversible-but-cheap thing: never bake a *closed* receipt format.
- **proofgate stays open** — publish the gate + verification method. (The moat is the *receipt network + operator position*, not the code — open standards grow adoption; TCP/IP is open, the operators still win.)
- **Portable reputation** — an agent/tenant's receipt history is exportable and verifiable *outside* AMOS. This deliberately lowers our own switching costs. That cost *is* the point: it's the Ulysses pact working.

**Late lock-in — dormant until volume makes neutrality real:**
- **Settlement / token / marketplace.** Payments layered onto the web decades after it standardized; this layer truly can wait. Reopened only by the triggers below.

## What carries forward from the protocol era

The receipt schema. The Oracle / semantic gate. The trust ladder (reputation from passed receipts). Nothing else. Decay curves, emissions schedules, pool separation — defensive mechanism design for a market that didn't exist. Dormant until proven necessary.

## Triggers that reopen the marketplace / token

Do **not** build marketplace/settlement features until one is observed, unprompted, from real customers:

1. A tenant asks to hire capability its own AI can't provide (natural form: a bounty with a validation plan, settled in dollars).
2. Receipts get handed across org boundaries as the deliverable itself (Nuvola is closest today).
3. Two AMOS tenants want to transact and neither trusts the other's records.

**Trigger 3 — and only trigger 3, at scale — reopens the token question.**

## The commitment device

The open schema + open gate + portable reputation + the (dormant) on-chain programs together are a **constitutional backstop**: if AMOS Labs ever turns rent-extractor, there is an open protocol and portable reputations to fork to. Its credibility **scales with adoption** — today a fork has little to fork to; that's fine, because forkability and real neutrality are earned on the same clock as the commercial volume. This is the answer to *"what stops you from becoming Visa?"* — and it's the part that survives our own success, which the pure company path lacks.

## The decision test

At any fork: *does this make receipts more numerous, more trusted, or more portable?* If yes, it's on the road. If it just makes the CRM a nicer SaaS, it's drift — acceptable only if it pays for the road. And the discipline runs the other way too: **don't build protocol elegance ahead of the demand that makes neutrality real.** Volume first.

## What we steer by this quarter

Construction CRM shipped on the platform → agent execution env + receipt generation closed (dogfood P1/P2) → component library extracted → reseller channel live. Keep receipts **export-clean + schema documented** as we go (near-zero cost, protects the one early-lock-in layer that matters). Everything else trails.

---

*Vehicle docs: `amos-platform/docs/PLATFORM-BUILD-PLAN.md`, `CONSTRUCTION-CRM-PLAN.md`, `INTEGRATION-ONRAMPS-PLAN.md`. Capture-resistance artifact: `docs/protocol/receipt-schema.md`. This doc is the compass they serve.*
