# CLAUDE.md

This is **amos-protocol** — the dormant protocol-era AMOS stack (relay, oracle,
Solana programs, autonomous agent), extracted from `amos-platform-2.0` (WS-4).
Read `README.md` first; `AGENT_CONTEXT.md` is the protocol source of truth.

## Build & test

```bash
cargo check --workspace
cargo test --lib -p amos-relay -p amos-oracle -p amos-agent -p amos-core
cd amos-solana && anchor build     # Anchor 0.30.1, built outside the workspace
```

## Ground rules

- **Dormant, not dead**: do not delete protocol mechanisms; reactivation is
  governed by the NORTH-STAR triggers in the platform repo.
- The **mainnet/devnet program IDs must never change** casually — they are
  deployed. Treasury/governance/bounty program changes = human review.
- `amos-core` here is a **frozen snapshot**; the authoritative core lives in
  `amos-platform-2.0`. Don't evolve it here except to keep this repo building.
- The bounty bot workflows (`.github/workflows/*-bot.yml`, stale-merge metric)
  poll the relay — they are part of this repo's operational surface.
