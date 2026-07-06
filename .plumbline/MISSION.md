# Mission — amos-protocol

This repository is the **AMOS protocol layer** — the long-game economic rails for
autonomous work, extracted from `amos-platform-2.0` so the protocol track can evolve
independently of the commercial platform. It holds four cargo crates plus the on-chain
programs:

- **`amos-relay`** — the bounty marketplace: bounty lifecycle, proof-receipt intake,
  reputation, and settlement coordination with the chain.
- **`amos-oracle`** — semantic review of proof receipts (mission alignment, validation
  coverage, safety, RSI risk), driven by the constitutional prompt in
  `amos-oracle/prompts/`.
- **`amos-agent`** — the protocol-era default autonomous worker.
- **`amos-core`** — a frozen snapshot of the shared core these crates depend on. The
  *live* `amos-core` is authoritative in `amos-platform-2.0`; this copy keeps the
  protocol track self-contained.
- **`amos-solana`** — the Anchor on-chain programs (treasury, governance, bounty),
  built via `anchor build` outside the cargo workspace. **Program IDs are frozen** on
  mainnet and devnet.

This is a research / long-game track, not the current commercial priority — but it is
very much alive, and the primitives here (the proof receipt, the Oracle) are the lineage
of Plumbline and the platform's operation receipts.

## What a change here must honor

1. **The token economy is a set of invariants, not preferences.** Decay, trust levels,
   protocol fees, and reputation math (`amos-core/src/token/**`,
   `amos-relay/src/reputation.rs`, `amos-relay/src/protocol_fees.rs`) define the economic
   physics. A change to these is an economic-policy change; it must state its intent
   against `AGENT_CONTEXT.md`, the protocol's agent-facing source of truth.
2. **On-chain code is settlement of real value.** The Anchor programs
   (`amos-solana/programs/**`) and the relay's settlement path
   (`amos-relay/src/solana.rs`, `settlement_retry.rs`) move funds and cannot silently
   change program IDs or settlement semantics. Treat these as protected.
3. **The proof-carrying loop is the trust boundary.** Proof-receipt intake
   (`amos-relay/src/proof_receipt.rs`), the bounty lifecycle
   (`amos-relay/src/routes/bounties.rs`), the GitHub webhook receiver
   (`amos-relay/src/routes/webhooks.rs`), and the Oracle's review/prompt
   (`amos-oracle/src/review.rs`, `prompt.rs`, `amos-oracle/prompts/**`) are what make an
   unverified agent's work trustworthy. Weakening them weakens every downstream claim.
4. **The gate governs the gate.** The CI workflows, this `.plumbline/` directory, and the
   Oracle prompt are the enforcement machinery. Changes to them are `self_modifying` and
   escalate to a human by design — an agent must not quietly rewrite the rules it is
   judged by.
5. **`amos-core` here is a mirror, not a fork.** It is deliberately a frozen snapshot;
   divergence from the authoritative copy in `amos-platform-2.0` should be intentional and
   noted, not accidental drift.

## The loop for contributors (human or agent)

`plumb propose` (issue + contract) → work → `plumb receipt --write` → fill the judgment
fields honestly → push → CI (`Lint & Unit Tests`) + the `Plumbline Gate` must pass →
human review where escalated → merge.

Automate the bookkeeping; never the judgment.
