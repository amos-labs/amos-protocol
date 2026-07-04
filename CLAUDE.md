# CLAUDE.md

This is **amos-protocol** — the AMOS protocol: the long-game economic layer
(relay, oracle, Solana programs, autonomous agents), extracted from
`amos-platform-2.0` (WS-4). Actively developed as a research/side track
alongside the commercial platform; not the current commercial priority.
Read `README.md` first; `AGENT_CONTEXT.md` is the protocol source of truth.

## Build & test

```bash
cargo check --workspace
cargo test --lib -p amos-relay -p amos-oracle -p amos-agent -p amos-core
cd amos-solana && anchor build     # Anchor 0.30.1, built outside the workspace
```

## Ground rules

- **Active side track**: do not delete protocol mechanisms; commercial-scale
  activation is governed by the NORTH-STAR triggers in the platform repo.
- The **mainnet/devnet program IDs must never change** casually — they are
  deployed. Treasury/governance/bounty program changes = human review.
- `amos-core` here is a **frozen snapshot**; the authoritative core lives in
  `amos-platform-2.0`. Don't evolve it here except to keep this repo building.
- The bounty bot workflows (`.github/workflows/*-bot.yml`, stale-merge metric)
  poll the relay — they are part of this repo's operational surface.
