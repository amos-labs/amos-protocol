# AMOS Protocol

> **The long-game economic layer.** This repo holds the AMOS protocol — the
> bounty **relay**, the **Oracle** semantic-review agent, the **Solana**
> on-chain programs (treasury, governance, bounty), and the autonomous
> **agent** — extracted from
> [`amos-platform-2.0`](https://github.com/amos-labs/amos-platform-2.0)
> (WS-4) so the two tracks can evolve independently. It is **actively
> developed as a research/side track** alongside the commercial platform;
> not the current commercial priority, very much alive. The mainnet/devnet
> **program IDs are unchanged**. Scaling triggers live in the platform
> repo's `docs/NORTH-STAR.md` — volume first, then rails.

## What lives here

| Crate | Purpose |
|-------|---------|
| `protocol-math` | Shared integer emission, payout-cap and decay kernel |
| `amos-relay` | Bounty marketplace, proof receipts, reputation, settlement coordination |
| `amos-oracle` | Semantic review of proof receipts (mission alignment, validation coverage, RSI risk) |
| `amos-solana` | Anchor on-chain programs — treasury, governance, bounty (built via Anchor, outside the cargo workspace) |
| `amos-agent` | The protocol-era default autonomous worker (superseded commercially by BYO-AI over MCP) |
| `amos-core` | Frozen snapshot of the shared core these crates depend on. **The live `amos-core` is authoritative in `amos-platform-2.0`** — this copy keeps the protocol track self-contained. |

[Economic Contract v1](docs/protocol/ECONOMIC_CONTRACT.md) is the canonical
reconciled economic specification. [AGENT_CONTEXT.md](AGENT_CONTEXT.md) is its
agent-facing entry point. `protocol-math` supplies the integer emission and payout
kernel shared by Relay and the bounty program. Historical papers and the frozen
core snapshot remain reference material, not conflicting runtime authorities.

[Reconciliation and activation prerequisites](docs/protocol/RECONCILIATION_2026-09-10.md)
record source fixes, tests and migration limits. Merging these sources does not
activate a chain upgrade or prove current deployment state.

## Lineage

The proof-receipt idea born here is very much alive: it became
[**Plumbline**](https://github.com/amos-labs/plumbline) (the proof-carrying
gate) and the AMOS platform's operation receipts. The verification core
graduated to the commercial track; the token + marketplace mechanisms keep
evolving here as the long-game track.

## Build

```bash
cargo check            # relay, oracle, agent, frozen core, protocol-math
cd amos-solana && anchor build   # on-chain programs (Anchor 0.31.1)
```
