# Custody and revenue accounting V2

This is a source implementation for review, not an account migration or evidence of deployment. The treasury program ID remains `8ZMaZDAxDPsCnMGRkhwLmFhoG43WUJcGC8xqVKo2PN7s`; the bounty program ID remains `4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq`.

## Existing accounts and assets

The serialized `TreasuryConfig`, `StakeRecord`, `HolderPool`, `Distribution`, `BountyConfig`, and `BountyProof` layouts and discriminators are unchanged. No old account is reallocated or deserialized as a new type. Legacy `register_stake`, `update_stake`, `claim_revenue`, `get_claimable_amount`, and `distribute_protocol_fee` instructions fail with `LegacyAccountingDisabled`. Failed instructions roll back all account initialization and transfers. Their historical records remain readable through existing account readers; old balance-proportional claim estimates must not be presented as V2 entitlements.

**This is deliberately not a claim that old funds can all be withdrawn after an upgrade.** Legacy stake records do not identify a trusted custody vault and never recorded reward debt. They cannot safely be imported as withdrawable principal or revenue. Before any upgrade, operators must inventory actual escrow, staking, reward and treasury accounts and original funding transactions. Reconcile beneficial ownership and any past withdrawals; resolve legitimate legacy balances through a separately reviewed recovery/migration transaction design or finish settlement before upgrading. Do not manufacture V2 positions from a legacy `amount` field.

The existing authority-only treasury withdrawal still covers only the canonical `treasury_amos` vault owned by `TreasuryConfig`. It does not rescue arbitrary legacy stake vaults, a legacy reward vault, the reserve, or bounty escrow. Ordinary owners can still move tokens from legacy token accounts they themselves control using SPL Token. A program-owned legacy vault or escrow may require an explicit audited recovery instruction. None is added here; no recovery authority or original depositor is inferred from an unsafe legacy balance. Upgrading before that inventory/recovery work could immobilize legitimate funds. No on-chain inventory was performed for this change.

## New treasury account contract

All seeds below are under the unchanged treasury program ID. `RevenuePoolV2` uses `revenue_pool_v2` (130 bytes), `StakePositionV2` uses `[stake_v2, owner]` (106 bytes). Both carry `version = 2` and distinct Anchor discriminators. `stake_custody_v2` and `holder_rewards_v2` are separate canonical SPL Token accounts, both owned by the `RevenuePoolV2` PDA and both bound to its mint. No instruction permits a caller-selected custody vault, reward source, owner or mint. Classic SPL Token is used, not Token-2022 transfer-fee tokens.

`initialize_revenue_v2` requires the existing treasury authority signature, its configured mint with exactly nine decimals, and creates fresh zeroed custody/accounting. It snapshots the configured Labs **wallet owner** and mint; later legacy config/mint changes do not mutate these V2 bindings. A minimum stake is 100 AMOS = **100,000,000,000 atomic units**. All `amount` arguments and recorded balances are atomic units.

| Instruction | Ordered accounts (Anchor names) | Arguments |
|---|---|---|
| `initialize_revenue_v2` | authority, treasury_config, mint, pool, custody, rewards, token_program, system_program | none |
| `open_stake_v2` | owner, pool, position, owner_tokens, custody, rewards, token_program, system_program | amount: u64 |
| `set_stake_v2` | owner, pool, position, owner_tokens, custody, rewards, token_program | amount: u64 |
| `claim_revenue_v2` | same as `set_stake_v2` | none |
| `distribute_fee_v2` | payer, pool, mint, payer_tokens, rewards, labs_tokens, token_program | amount: u64 |
| `sync_rewards_v2` | pool, rewards | none |

The Rust `Accounts` constraints remain authoritative for signer/writable flags and seeds. `set_stake_v2(0)` withdraws all principal without deleting the position; retained earned credit can still be claimed when eligible. A new/increased stake starts a 30-day **claim eligibility delay**, matching the old timer behavior; this is not a 30-day custody lock, and decreasing/withdrawing does not earn rewards after the decrease. Increasing stake resets eligibility for the position's pending claim as well. There is no position-closing or index-reset instruction.

## Accrual and commercial fee integration

V2 stores a cumulative reward-per-atomic-stake index with scale 10^18. Each position stores its index checkpoint, pending whole-unit credit and fractional remainder. Every claim first settles only the index delta; a successful claim clears pending credit and reduces the pool's accounted vault balance. Claim order does not change entitlement and a second claim cannot consume another holder's balance.

Every stake change and claim first recognizes actual, previously unaccounted deposits to the canonical rewards vault. Thus a deposit cannot be moved across a membership change by delaying an explicit sync. Direct commercial holder-share transfers can also be recognized by permissionless `sync_rewards_v2`. No token balance is credited based on an oracle's asserted amount alone.

The commercial escrow already splits its **3% fee** into 50% holders / 40% burn / 10% Labs. Its configured holder destination must therefore be `holder_rewards_v2`; transfer there and sync. **Do not pass that already-split holder share to `distribute_fee_v2`**, which is a separate fresh-fee entry point that pulls new funds from the payer's signed token account and applies 50/40/10 once. It cannot redistribute an existing treasury balance. The Labs destination must be a same-mint token account owned by the pool's immutable Labs wallet. Rounding of the fee split goes to Labs as before.

Deposits while no stake exists are recorded as `unallocated` and remain in the rewards vault, excluded from future staker entitlements. Index rounding dust also remains in the vault. Fractional credits survive settlement/withdrawal, but there is no privileged sweep or mechanism to reassign dust/unallocated balances. This avoids a first-staker windfall; any future dust policy needs an explicit versioned design. Pool `total_received`/`total_claimed` are V2 holder-reward counters, not legacy gross-fee counters; `FeeDistributedV2` records new full-fee splits.

The first-party `scripts/set-fee-recipients.mjs` now requires explicit RPC and Labs token addresses, derives the V2 reward PDA, verifies account owners/discriminators/version/mint/custody, and prints a dry-run instruction by default. It never loads a key unless `--execute --keypair PATH` is explicit. Updating these source files does not configure a deployed bounty program or activate V2.

## Commercial escrow V2 account contract

Existing commercial instruction names and argument encodings remain, but their account lists now require `CommercialEscrowV2` metadata at `[commercial_escrow_v2, bounty_id]` under the bounty program. Its 123-byte account stores version, bounty ID, original poster, mint, deadline, deposited amount and terminal status. The existing escrow token PDA `[bounty_escrow, bounty_id]` is preserved, is also its own PDA token authority, and cannot be reinitialized to invent metadata for a legacy funded escrow.

Creation requires a future deadline, nine-decimal mint and the canonical commercial minimum. Release requires the configured oracle, stored poster/mint, open status, and `now < deadline`; refund requires the stored poster's **signature**, their same-mint destination, open status and `now >= deadline`. Release also binds the reviewer token owner to the stated reviewer, makes the burn mint writable, and bounds quality to 30–100. Status moves once from open to released/refunded. Each settlement transfers the full current escrow token balance, including any unsolicited deposits received before settlement. Later donations to a terminal escrow are not refundable by this interface. No close/reinitialize path exists.

| Instruction | Ordered accounts |
|---|---|
| `create_commercial_bounty` | config, escrow_record, escrow_token_account, escrow_authority, mint, poster_token_account, poster, token_program, system_program, rent |
| `release_commercial_bounty` | escrow_record, config, bounty_proof, operator_stats, operator, escrow_token_account, escrow_authority, mint, operator_token_account, reviewer_token_account, holder_pool, labs_wallet, poster, oracle_authority, token_program, system_program |
| `refund_commercial_bounty` | escrow_record, config, escrow_token_account, escrow_authority, mint, poster_token_account, poster, token_program |

Legacy escrows lack authenticated poster/deadline metadata and fail closed. Neither a caller-supplied poster nor a guess from token account ownership is a migration proof. Original funding transaction evidence plus a separately reviewed recovery design is required before upgrading a program with outstanding legacy escrow. Generated IDLs and external clients must be updated together with this account-list change; old client account lists do not continue to work.

## Offline build and validation

No first-party create/release/refund or staking transaction builder exists in the current repository; this account contract and generated Anchor IDLs define the integration work for those clients. Generate the IDLs locally from source; never reuse a deployed legacy IDL for these changes:

```sh
cargo test --manifest-path amos-solana/Cargo.toml -p amos-treasury -p amos-bounty --lib
node --test amos-solana/programs/amos-treasury/tests/fee-recipient-builder.test.mjs
# With rustup Cargo ahead of Homebrew Cargo:
cargo build-sbf --manifest-path amos-solana/programs/amos-treasury/Cargo.toml
cargo build-sbf --manifest-path amos-solana/programs/amos-bounty/Cargo.toml
cd amos-solana
anchor idl build --program-name amos_treasury --out /tmp/amos-treasury-v2-idl.json
anchor idl build --program-name amos_bounty --out /tmp/amos-bounty-v2-idl.json
```

Host tests cover repeated/reordered claims, membership changes, fractional carry, zero-staker deposits, claim timing, legacy discriminator rejection, and generated Anchor account validation for forged custody/mint/authority/unsigned stake owner and wrong-poster/unsigned/legacy-metadata refunds. They do not execute SPL transfers inside a validator, prove deployment state, or replace a local-validator migration rehearsal and compute-budget review before a chain upgrade. Existing Anchor configuration includes historical localnet ID differences; this work does not rewrite program identities to make a deployment command appear to succeed.
