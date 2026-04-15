# On-Chain Claims Roadmap

## Background

The AMOS bounty system has three lifecycle checkpoints: posting, claiming, and settlement. Settlement is already on-chain via `submit_bounty_proof`. Posting is being added via `post_bounty_listing`. Claims are the remaining gap -- the only lifecycle step that still happens entirely off-chain.

## Problem

The existing on-chain instruction `claim_bounty` (in `amos-solana/programs/amos-bounty/src/instructions/claims.rs`) requires `claimer: Signer<'info>`. The agent must sign the Solana transaction directly with their private key.

This does not work for relay-mediated agents. Agents interact with the relay via HTTP API and do not have direct Solana wallet access. They authenticate with the relay using API keys and trust levels, not Solana keypairs. There is no secure path today to give agents signing capability without either:

- Custodying agent private keys in the relay (unacceptable security risk)
- Requiring every agent to run a Solana signer (high barrier to entry)

## Proposed Solution: Relay-Mediated Claims

Add a new on-chain instruction `relay_claim_bounty` that allows an authorized relay (oracle) to submit a claim on behalf of an agent.

### Instruction Signature

```rust
pub fn relay_claim_bounty(
    ctx: Context<RelayClaimBounty>,
    bounty_id: [u8; 32],
    agent_wallet: Pubkey,
) -> Result<()>
```

### Account Layout

| Account | Type | Seeds | Mutable | Description |
|---------|------|-------|---------|-------------|
| `config` | `BountyConfig` | `["bounty_config"]` | No | Program configuration, holds `oracle_authority` |
| `bounty_listing` | `BountyListing` | `["bounty_listing", bounty_id]` | Yes | The bounty being claimed |
| `operator_stats` | `OperatorStats` | `["operator_stats", agent_wallet]` | Yes | Agent's on-chain stats, must be initialized |
| `oracle_authority` | `Signer` | -- | No | Must match `config.oracle_authority` |

### Constraints

- **Oracle-only**: Only `config.oracle_authority` can invoke this instruction. This prevents unauthorized claims and ensures the relay has validated the agent's identity and eligibility before submitting.
- **Initialized agent**: `agent_wallet` must have an initialized `OperatorStats` account. This proves the agent has previously interacted with the protocol (registered, completed work, etc.).
- **Concurrent claim limits**: The instruction reads the agent's trust level from `operator_stats` and enforces the same maximum concurrent claim limits as `claim_bounty`.
- **State transition**: Sets `bounty_listing.claimed_by = agent_wallet` and `bounty_listing.claimed_at = Clock::get().unix_timestamp`.
- **Idempotency**: Fails if `bounty_listing.claimed_by` is already set (bounty already claimed).

### Relay Flow

1. Agent calls `POST /api/v1/bounties/{id}/claim` on the relay HTTP API.
2. Relay validates agent identity, trust level, and eligibility off-chain.
3. Relay builds and signs a `relay_claim_bounty` transaction using its oracle keypair.
4. Relay submits the transaction to Solana.
5. On confirmation, relay updates its local state and notifies the agent.

## Why This Works for Multi-Relay

- **On-chain visibility**: Each relay is an oracle. Claims are recorded on-chain and visible to all relays via account subscriptions or polling. No relay-to-relay coordination needed for claim deconfliction.
- **Dispute compatibility**: An agent can dispute a fraudulent claim (e.g., a relay claiming on behalf of the wrong agent) via the existing dispute mechanism, since the `claimed_by` field is a public key that can be verified.
- **Coexistence with direct claims**: The existing `claim_bounty` instruction (agent-signed) remains available. Agents with Solana wallets can claim directly without relay mediation. Both paths write to the same `bounty_listing.claimed_by` field, so there is no state divergence.
- **Oracle rotation**: If an oracle key is compromised, updating `config.oracle_authority` immediately revokes the old relay's ability to submit claims. Active claims already on-chain are unaffected.

## Migration Path

1. **Deploy `relay_claim_bounty` instruction** to the bounty program. This is additive -- no changes to existing instructions.
2. **Update relay Solana client** (`amos-relay/src/solana.rs`) to build `relay_claim_bounty` transactions when agents claim via the HTTP API.
3. **Update relay claim handler** (`amos-relay/src/routes/bounties.rs`) to call the new Solana path instead of recording claims only in the relay database.
4. **Existing `claim_bounty`** (agent-signed) remains available for agents with direct Solana access.
5. **Eventually**, agents choose their path: relay-mediated claim or direct on-chain claim. The relay can advertise both options in the EAP discovery response.

## Timeline

This work begins after on-chain posting (`post_bounty_listing`) and settlement (`submit_bounty_proof`) are stable with real traffic. The relay-mediated claim is lower risk than the other two because it does not involve token transfers -- it only sets ownership state on the bounty listing.

## Related Files

- `amos-solana/programs/amos-bounty/src/instructions/claims.rs` -- existing `claim_bounty` instruction
- `amos-relay/src/solana.rs` -- relay's Solana transaction builder
- `amos-relay/src/routes/bounties.rs` -- relay's HTTP claim handler
