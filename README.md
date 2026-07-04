# AMOS Protocol

> **⚠️ Dormant, not dead.** This repo holds the protocol-era AMOS stack — the
> bounty **relay**, the **Oracle** semantic-review agent, the **Solana**
> on-chain programs (treasury, governance, bounty), and the default
> autonomous **agent** — extracted from
> [`amos-platform-2.0`](https://github.com/amos-labs/amos-platform-2.0)
> (WS-4 of the AI-native pivot) so the commercial substrate doesn't compile
> against it. The mainnet/devnet **program IDs are unchanged**; nothing here
> was torn down. Reactivation is governed by the triggers in the platform
> repo's `docs/NORTH-STAR.md` — volume first, then rails.

## What lives here

| Crate | Purpose |
|-------|---------|
| `amos-relay` | Bounty marketplace, proof receipts, reputation, settlement coordination |
| `amos-oracle` | Semantic review of proof receipts (mission alignment, validation coverage, RSI risk) |
| `amos-solana` | Anchor on-chain programs — treasury, governance, bounty (built via Anchor, outside the cargo workspace) |
| `amos-agent` | The protocol-era default autonomous worker (superseded commercially by BYO-AI over MCP) |
| `amos-core` | Frozen snapshot of the shared core these crates depend on. **The live `amos-core` is authoritative in `amos-platform-2.0`** — this copy exists so a dormant repo stays self-contained. |

`AGENT_CONTEXT.md` is the protocol's agent-facing source of truth (token
parameters, decay, trust levels, bounty lifecycle). The protocol docs live in
`docs/protocol/` and `docs/core/thesis.md`; the token-economy legacy papers in
`docs/archive/`.

## Lineage

The proof-receipt idea born here is very much alive: it became
[**Plumbline**](https://github.com/amos-labs/plumbline) (the proof-carrying
gate) and the AMOS platform's operation receipts. The verification core
graduated; the token + marketplace mechanisms sleep here until the North-Star
triggers fire.

## Build

```bash
cargo check            # relay, oracle, agent, core
cd amos-solana && anchor build   # on-chain programs (Anchor 0.30.1)
```
