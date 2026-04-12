/// AMOS Treasury State Accounts
///
/// Defines all on-chain account structures for the treasury system.
/// All transactions are denominated in AMOS tokens. No USDC track.
/// Fee distribution: 50% holders, 40% burned, 10% Labs.
use anchor_lang::prelude::*;

// ============================================================================
// Treasury Configuration Account
// ============================================================================

/// Main configuration account for the AMOS Treasury
///
/// This account stores the core configuration including authority,
/// Labs wallet, token mint, and running totals.
///
/// PDA: ["treasury_config"]
#[account]
pub struct TreasuryConfig {
    /// Program authority (can only be changed by current authority)
    pub authority: Pubkey,

    /// AMOS Labs operating wallet (receives 10% of protocol fees)
    pub labs_wallet: Pubkey,

    /// AMOS token mint address
    pub amos_mint: Pubkey,

    /// Treasury AMOS vault address (holds bounty emission pool)
    pub treasury_amos_vault: Pubkey,

    /// Reserve vault address (DAO-locked emergency reserve)
    pub reserve_vault: Pubkey,

    /// Total AMOS protocol fees collected (all-time)
    pub total_fees_collected: u64,

    /// Total AMOS fees distributed to holders (all-time)
    pub total_fees_to_holders: u64,

    /// Total AMOS fees burned (all-time)
    pub total_fees_burned: u64,

    /// Total AMOS fees sent to Labs wallet (all-time)
    pub total_fees_to_labs: u64,

    /// Total AMOS tokens burned (from decay + fees)
    pub total_amos_burned: u64,

    /// Number of fee distributions processed
    pub distribution_count: u64,

    /// Total number of registered stakes
    pub total_stakes: u64,

    /// Total AMOS staked across all users
    pub total_staked_amount: u64,

    /// Timestamp of treasury initialization
    pub initialized_at: i64,

    /// Timestamp of last distribution
    pub last_distribution_at: i64,

    /// PDA bump seed
    pub bump: u8,

    /// Reserved space for future upgrades
    pub reserved: [u64; 8],
}

impl TreasuryConfig {
    pub const LEN: usize = 8 + // discriminator
        32 + // authority
        32 + // labs_wallet
        32 + // amos_mint
        32 + // treasury_amos_vault
        32 + // reserve_vault
        8 + // total_fees_collected
        8 + // total_fees_to_holders
        8 + // total_fees_burned
        8 + // total_fees_to_labs
        8 + // total_amos_burned
        8 + // distribution_count
        8 + // total_stakes
        8 + // total_staked_amount
        8 + // initialized_at
        8 + // last_distribution_at
        1 + // bump
        64; // reserved
}

// ============================================================================
// Stake Record Account
// ============================================================================

/// Individual stake record for a user
///
/// Tracks a user's AMOS stake amount, timestamps, and claim history.
/// Users must stake for minimum 30 days before claiming fee revenue.
///
/// PDA: ["stake_record", user_pubkey]
#[account]
pub struct StakeRecord {
    /// Owner of this stake
    pub owner: Pubkey,

    /// Amount of AMOS tokens staked
    pub amount: u64,

    /// Timestamp when stake was registered
    pub staked_at: i64,

    /// Timestamp of last stake update
    pub updated_at: i64,

    /// Timestamp of last claim
    pub last_claim_at: i64,

    /// Total AMOS claimed from fee pool (all-time)
    pub total_amos_claimed: u64,

    /// Number of claims made
    pub claim_count: u64,

    /// PDA bump seed
    pub bump: u8,
}

impl StakeRecord {
    pub const LEN: usize = 8 + // discriminator
        32 + // owner
        8 + // amount
        8 + // staked_at
        8 + // updated_at
        8 + // last_claim_at
        8 + // total_amos_claimed
        8 + // claim_count
        1; // bump

    /// Check if minimum stake period has been met
    pub fn can_claim(&self, current_time: i64, min_stake_seconds: i64) -> bool {
        let stake_duration = current_time.saturating_sub(self.staked_at);
        stake_duration >= min_stake_seconds
    }

    /// Get stake duration in days
    pub fn stake_duration_days(&self, current_time: i64) -> u64 {
        let duration_seconds = current_time.saturating_sub(self.staked_at);
        (duration_seconds / 86400) as u64
    }
}

// ============================================================================
// Distribution Record Account
// ============================================================================

/// Record of a fee distribution event
///
/// Immutable record of each distribution for transparency.
/// All distributions are AMOS-only with 50/40/10 split.
///
/// PDA: ["distribution", distribution_index]
#[account]
pub struct Distribution {
    /// Sequential index of this distribution
    pub index: u64,

    /// Timestamp of distribution
    pub timestamp: i64,

    /// Total fee amount (before split)
    pub total_amount: u64,

    /// Amount to holders pool (50%)
    pub amount_to_holders: u64,

    /// Amount burned (40%)
    pub amount_burned: u64,

    /// Amount to Labs wallet (10%)
    pub amount_to_labs: u64,

    /// Payment reference (bounty ID, etc.)
    pub payment_reference: String,

    /// PDA bump seed
    pub bump: u8,
}

impl Distribution {
    pub const MAX_PAYMENT_REF_LEN: usize = 64;

    pub const LEN: usize = 8 + // discriminator
        8 + // index
        8 + // timestamp
        8 + // total_amount
        8 + // amount_to_holders
        8 + // amount_burned
        8 + // amount_to_labs
        (4 + Self::MAX_PAYMENT_REF_LEN) + // payment_reference string
        1; // bump
}

// ============================================================================
// Holder Pool Account
// ============================================================================

/// Holder pool state tracking
///
/// Tracks the AMOS pool available for staker claims.
/// Protocol fee revenue (50% share) accumulates here.
///
/// PDA: ["holder_pool"]
#[account]
pub struct HolderPool {
    /// Current AMOS balance available for claims
    pub amos_balance: u64,

    /// Total AMOS deposited (all-time)
    pub total_amos_deposited: u64,

    /// Total AMOS claimed by all holders (all-time)
    pub total_amos_claimed: u64,

    /// Number of claims processed
    pub claim_count: u64,

    /// Timestamp of last deposit
    pub last_deposit_at: i64,

    /// Timestamp of last claim
    pub last_claim_at: i64,

    /// PDA bump seed
    pub bump: u8,
}

impl HolderPool {
    pub const LEN: usize = 8 + // discriminator
        8 + // amos_balance
        8 + // total_amos_deposited
        8 + // total_amos_claimed
        8 + // claim_count
        8 + // last_deposit_at
        8 + // last_claim_at
        1; // bump
}

// ============================================================================
// Treasury Statistics (View/Query struct, not an account)
// ============================================================================

/// Treasury statistics returned by get_treasury_state
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct TreasuryStats {
    /// Total AMOS protocol fees collected (all-time)
    pub total_fees_collected: u64,

    /// Total AMOS fees to holders
    pub total_fees_to_holders: u64,

    /// Total AMOS fees burned
    pub total_fees_burned: u64,

    /// Total AMOS fees to Labs
    pub total_fees_to_labs: u64,

    /// Total AMOS burned (fees + decay)
    pub total_amos_burned: u64,

    /// Number of distributions
    pub distribution_count: u64,

    /// Total registered stakes
    pub total_stakes: u64,

    /// Total amount staked
    pub total_staked_amount: u64,

    /// Current holder pool balance
    pub holder_pool_amos: u64,

    /// Treasury initialization timestamp
    pub initialized_at: i64,

    /// Last distribution timestamp
    pub last_distribution_at: i64,
}

// ============================================================================
// Claimable Amount (View/Query struct, not an account)
// ============================================================================

/// Claimable revenue amounts for a specific stake
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ClaimableAmount {
    /// Claimable AMOS amount from fee pool
    pub amos_amount: u64,

    /// User's stake amount
    pub stake_amount: u64,

    /// Total staked across all users
    pub total_staked: u64,

    /// User's share percentage (basis points)
    pub share_bps: u16,

    /// Can claim (minimum period met)
    pub can_claim: bool,

    /// Days staked
    pub days_staked: u64,

    /// Days remaining until eligible
    pub days_remaining: u64,
}
