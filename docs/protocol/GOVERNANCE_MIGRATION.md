# Custodied feature voting — V2 migration

Status: source replacement under review, not a deployed upgrade. This document specifies the migration and its limits. Program ID remains `245xpoWLEAAPmUQxMSBDqQw5qnGfqt5roi5enuFG9fZZ`. Existing GovernanceConfig, FeatureProposal, VoteRecord, parameters and other account layouts/discriminators are unchanged. Existing error numbers are retained; new errors are appended. No live transaction is part of this change.

## Why a new vote account is required

The old `vote_for_feature` observed a wallet's liquid token balance and incremented a tally. It did not transfer or lock tokens. The old withdrawal only changed the tally. Consequently old VoteRecord amounts are not deposits and cannot be converted into refundable balances or verified influence.

Both legacy vote/withdraw instruction ABIs now return `CustodiedVotingRequired`. Their transaction changes, including account initialization, roll back on error. There are no legacy escrowed funds to release through these instructions. Do not reinterpret old records as V2 records or credit old counts into new proposals.

## New proposals and explicit V2 custody

`submit_feature_proposal` keeps its existing instruction accounts and FeatureProposal layout, but newly created proposals receive `AMOSVOT2` in the first eight reserved bytes. That marker can only be written by program-owned initialization; no instruction marks an existing proposal as migrated. New feature votes require this marker.

`vote_for_feature_v2(proposal_id, vote_amount)` accepts the configured governance mint and the classic SPL Token program. Amounts are raw mint units, not whole-token display values. It requires an active, unexpired proposal and a signer-owned source token account for that exact mint. Self-votes remain forbidden.

Two new PDAs are derived under the unchanged governance program:

- VoteRecordV2: `["vote_record_v2", proposal_id_le_u64, voter_pubkey]`.
- Token vault: `["vote_vault_v2", proposal_id_le_u64, voter_pubkey]`.

The vote-record PDA owns the token vault. Anchor initializes both accounts exactly once, paid by the voter. The record binds version, voter, proposal account and ID, mint, amount, timestamps and both bumps. A checked SPL transfer moves the amount into the vault before it contributes to the proposal tally. Failed transfers do not add votes. Tokens in one vault are unavailable for a simultaneous vote through another wallet or proposal.

## Withdrawal and terminal history

`withdraw_vote_v2(proposal_id)` requires the original voter signature, the same record, the canonical vault, the configured mint, and a destination token account owned by that voter. It does not permit arbitrary recipients or treasury vault substitution.

While Active, withdrawal is available after the seven-day lock or once the 90-day proposal lifetime ends. It removes the recorded weight before that custody becomes reusable. Expired proposals cannot accept more votes or advance from Active to InDevelopment. InDevelopment, AwaitingGates and RewardsDistribution retain custody until the oracle reaches Finalized or Cancelled.

Finalized and Cancelled permit immediate refunds, even if seven days have not elapsed. Their recorded tally and terminal timestamp are preserved. Neither terminal status can transition to any other status, including another terminal identity. The oracle remains the authority for status transitions; there is no voter-controlled finalize, cancellation, or reopening instruction. Other reward/gate handlers already require their specific nonterminal states.

Refund returns the full bound vault balance to the voter, including unsolicited extra deposits. Such deposits never increase votes. This prevents dust from blocking token-vault closure. The transfer and close occur in one atomic transaction. A vault below the recorded amount fails closed. The V2 vote record remains allocated with `withdrawn_at` set: it is a replay tombstone and cannot be reinitialized to vote again under that voter/proposal pair. Token-vault rent returns to the voter; vote-record rent remains with the tombstone.

## Existing proposals and clients

Inventory pre-upgrade proposals and inform affected users before any deployment. Existing Active proposals must be cancelled and resubmitted with fresh proposal IDs; voters then opt into actual V2 custody. No automatic migration, token collection, minted credit or imported vote count is authorized by this source change.

Legacy tallies remain readable history, but `calculate_priority` assigns them zero community-vote weight. Legacy Active proposals cannot advance into development. Proposals already in development retain their existing oracle/gate/reward lifecycle; this does not retroactively certify their old votes. They have no V2 deposits. Budget-gate steward voting is a separate registered-steward mechanism and is not converted into token voting here.

Clients must use the new vote/withdraw instructions and account lists. Publish the rebuilt IDL only after the approved program revision is built and migration reviewed. Existing program addresses and historical records stay stable. The localnet placeholder in Anchor.toml is not deployment evidence; this change does not edit it or any wallet configuration.

## Verification and remaining qualification

Host regressions execute the production transfer helper through SPL Token's native processor, showing that escrowed tokens cannot be moved to a second wallet, and that only the correctly signed PDA can return custody. Actual Anchor withdrawal account validation rejects wrong recipients, mint, vault and missing signatures. Additional tests cover lock/expiry boundaries, all terminal transitions, legacy tally exclusion, discriminator separation and serialized V2 size.

The host CPI stub supplies runtime signer privileges; it does not prove SBF execution, transaction rollback, account-creation rent or token-vault closure in a validator. Required release evidence therefore also includes an actual Anchor/SBF build and a local-validator lifecycle test before production deployment: create → escrow vote → attempted double use → locked withdrawal failure → permitted withdrawal/refund → replay failure; repeat with cancellation, finalization, wrong accounts and unsolicited vault dust. No source test or compilation should be described as a deployed custody audit.
