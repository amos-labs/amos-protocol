/// AMOS Treasury Program
///
/// Versioned custody and fee distribution for the AMOS ecosystem.
/// Legacy stake/claim/distribution entrypoints fail closed. Use the V2 instruction
/// family described in MIGRATION_V2.md; deployment and account migration are separate.
/// All transactions denominated in AMOS tokens. No USDC track.
///
/// ## Fee Distribution (from Commercial Bounties)
/// - 50% to staked token holders (proportional to stake)
/// - 40% permanently burned (deflationary)
/// - 10% to AMOS Labs operating wallet
///
/// ## Trust Guarantees
/// - All percentages hardcoded in constants.rs
/// - No approval needed for claims (fully permissionless)
/// - Proportional distribution based on stake weight
/// - 30-day claim eligibility delay after new/increased stake
/// - All arithmetic uses checked operations
/// - Complete transparency via immutable distribution records
///
/// ## Staking Requirements
/// - Minimum stake: 100 AMOS tokens
/// - Claim eligibility delay: 30 days (principal is not time-locked)
/// - Can increase/decrease stake (maintaining minimum)
use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;
use state::*;

declare_id!("8ZMaZDAxDPsCnMGRkhwLmFhoG43WUJcGC8xqVKo2PN7s");

#[program]
pub mod amos_treasury {
    use super::*;

    // ========================================================================
    // Admin Instructions
    // ========================================================================

    /// Initialize the AMOS Treasury config and holder pool.
    /// Must be followed by `initialize_vaults` to complete setup.
    /// Fee splits are hardcoded constants.
    pub fn initialize(ctx: Context<Initialize>, labs_wallet: Pubkey) -> Result<()> {
        instructions::admin::initialize(ctx, labs_wallet)
    }

    /// Create treasury AMOS vault. Must be called after `initialize`.
    pub fn initialize_vaults(ctx: Context<InitializeVaults>) -> Result<()> {
        instructions::admin::initialize_vaults(ctx)
    }

    /// Create reserve vault. Must be called after `initialize_vaults`.
    pub fn initialize_reserve(ctx: Context<InitializeReserve>) -> Result<()> {
        instructions::admin::initialize_reserve(ctx)
    }

    /// Update the Labs wallet address. Authority-only.
    pub fn update_labs_wallet(
        ctx: Context<UpdateLabsWallet>,
        new_labs_wallet: Pubkey,
    ) -> Result<()> {
        instructions::admin::update_labs_wallet(ctx, new_labs_wallet)
    }

    /// Migrate the token mint and vault references. Authority-only.
    /// Atomically updates amos_mint, treasury_amos_vault, and reserve_vault.
    pub fn update_mint(ctx: Context<UpdateMint>) -> Result<()> {
        instructions::admin::update_mint(ctx)
    }

    /// Transfer tokens from treasury vault to a destination account.
    /// Authority-only. Used to rebalance tokens between program-controlled
    /// accounts (e.g., moving emission pool to bounty program treasury).
    pub fn authority_withdraw(ctx: Context<AuthorityWithdraw>, amount: u64) -> Result<()> {
        instructions::admin::authority_withdraw(ctx, amount)
    }

    // ========================================================================
    // Revenue Instructions
    // ========================================================================

    /// Distribute an AMOS protocol fee (from commercial bounties).
    /// Split: 50% holders, 40% burned, 10% Labs.
    /// Labs gets remainder to handle rounding dust.
    pub fn distribute_protocol_fee(
        ctx: Context<DistributeProtocolFee>,
        amount: u64,
        payment_reference: String,
    ) -> Result<()> {
        instructions::revenue::distribute_protocol_fee(ctx, amount, payment_reference)
    }

    // ========================================================================
    // Claim Instructions
    // ========================================================================

    /// Register AMOS stake for fee revenue sharing (min 100 AMOS, 30-day hold).
    pub fn register_stake(ctx: Context<RegisterStake>, amount: u64) -> Result<()> {
        instructions::claims::register_stake(ctx, amount)
    }

    /// Update existing stake amount (must maintain min 100 AMOS).
    pub fn update_stake(ctx: Context<UpdateStake>, new_amount: u64) -> Result<()> {
        instructions::claims::update_stake(ctx, new_amount)
    }

    /// Claim proportional share of AMOS fee revenue. Permissionless after 30 days.
    pub fn claim_revenue(ctx: Context<ClaimRevenue>) -> Result<()> {
        instructions::claims::claim_revenue(ctx)
    }

    /// Query claimable AMOS revenue amount (view function).
    pub fn get_claimable_amount(ctx: Context<GetClaimableAmount>) -> Result<ClaimableAmount> {
        instructions::claims::get_claimable_amount(ctx)
    }

    // ========================================================================
    // Transparency Instructions (View Functions)
    // ========================================================================

    /// Get current treasury statistics (view function).
    pub fn get_treasury_state(ctx: Context<GetTreasuryState>) -> Result<TreasuryStats> {
        instructions::transparency::get_treasury_state(ctx)
    }

    /// Get specific distribution by index (view function).
    pub fn get_distribution(ctx: Context<GetDistribution>, index: u64) -> Result<Distribution> {
        instructions::transparency::get_distribution(ctx, index)
    }

    /// New, separately funded V2 pool. Does not import legacy balances.
    pub fn initialize_revenue_v2(ctx: Context<InitializeRevenueV2>) -> Result<()> {
        instructions::v2::initialize_revenue_v2(ctx)
    }
    pub fn open_stake_v2(ctx: Context<OpenStakeV2>, amount: u64) -> Result<()> {
        instructions::v2::open_stake_v2(ctx, amount)
    }
    pub fn set_stake_v2(ctx: Context<ManageStakeV2>, amount: u64) -> Result<()> {
        instructions::v2::set_stake_v2(ctx, amount)
    }
    pub fn claim_revenue_v2(ctx: Context<ManageStakeV2>) -> Result<()> {
        instructions::v2::claim_revenue_v2(ctx)
    }
    pub fn distribute_fee_v2(ctx: Context<DistributeFeeV2>, amount: u64) -> Result<()> {
        instructions::v2::distribute_fee_v2(ctx, amount)
    }
    pub fn sync_rewards_v2(ctx: Context<SyncRewardsV2>) -> Result<()> {
        instructions::v2::sync_rewards_v2(ctx)
    }
}
