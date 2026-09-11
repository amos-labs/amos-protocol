# Solana Settlement

> Source architecture, not proof of an installed upgrade. See [Economic Contract v1](ECONOMIC_CONTRACT.md) and [reconciliation prerequisites](RECONCILIATION_2026-09-10.md).

Solana programs provide the on-chain settlement and constraint layer for AMOS.

## What Goes On-Chain

- bounty listing and settlement records
- contribution points and quality signals
- operator trust records
- daily pool accounting
- token, treasury, and governance constraints

## What Stays Off-Chain

Full proof receipts, logs, PR metadata, and Oracle reasoning can be too large or too contextual for direct on-chain storage. Relay stores canonical receipt payloads and may hash them into settlement evidence.

## Settlement Flow

1. Authenticated, permissioned Relay review records approval provenance.
2. Relay reads verified config/pool state and computes a bounded payout cap; unknown state defers payment.
3. Relay atomically prepares required accounts and submits `submit_bounty_proof`.
4. The program independently enforces the shared time-release and virtual-points cap, reviewer destination and category/trust limits.
5. Settlement transaction hash is recorded back on the Relay bounty.

Retries reconcile a confirmed proof only after checking program ownership,
account discriminator, full layout and bounty ID. Sending SOL to the predictable
address does not establish settlement: an empty system-owned account remains
uninitialized. Missing state allows a normal attempt; malformed state or RPC
failure defers settlement instead of recording payment.

## Related Material

- [Bounty Lifecycle](bounty-lifecycle.md)
- [Proof-Carrying Autonomous Loop](proof-carrying-loop.md)
- [On-Chain Claims Roadmap Legacy](../archive/on-chain-claims-roadmap.md)
