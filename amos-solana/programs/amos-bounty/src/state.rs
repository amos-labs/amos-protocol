/// AMOS Bounty Program State Accounts
///
/// This module defines all on-chain account structures that store the state
/// of the bounty distribution system. All fields are carefully sized and
/// documented to ensure transparent, trustless operation.
use anchor_lang::prelude::*;

// ============================================================================
// Enums
// ============================================================================

/// Source of bounty funding. Determines whether a protocol fee applies.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum BountySource {
    /// System bounty — funded from daily treasury emission, 0% fee
    Treasury,
    /// User-funded bounty — poster escrows AMOS tokens, 3% fee applies
    Commercial,
}

impl Default for BountySource {
    fn default() -> Self {
        BountySource::Treasury
    }
}

/// Staking vault tier — optional lockup for additional decay reduction.
/// Tenure and vault reductions stack multiplicatively:
///   effective_decay = base_decay × (1 - tenure_reduction) × (1 - vault_reduction)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum VaultTier {
    /// No lockup, no bonus
    None,
    /// 30 days, 20% reduction
    Bronze,
    /// 90 days, 50% reduction
    Silver,
    /// 365 days, 80% reduction
    Gold,
    /// No unlock, 95% reduction
    Permanent,
}

impl Default for VaultTier {
    fn default() -> Self {
        VaultTier::None
    }
}

/// Pool category for emission pool separation.
/// Technical pool is protected by a minimum floor; growth pool is capped.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PoolCategory {
    /// Protected by 80-95% emission floor (phase-dependent)
    Technical,
    /// Capped at 5-20% emission ceiling (phase-dependent)
    Growth,
}

impl Default for PoolCategory {
    fn default() -> Self {
        PoolCategory::Technical
    }
}

/// Status of a bounty listing on the board.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum BountyStatus {
    /// Available for claiming
    Open,
    /// Claimed by a worker, in progress
    Claimed,
    /// Work submitted, awaiting review
    Submitted,
    /// Approved and paid out
    Approved,
    /// Rejected by reviewer
    Rejected,
    /// Worker filed a dispute after rejection
    Disputed,
    /// Expired — deadline passed without completion
    Expired,
    /// Cancelled by poster (commercial only, before claim)
    Cancelled,
}

impl Default for BountyStatus {
    fn default() -> Self {
        BountyStatus::Open
    }
}

// ============================================================================
// PlatformMetrics - On-Chain Oracle for Economic Parameters
// ============================================================================

/// Rolling economic metrics used to compute dynamic decay rate.
/// Updated by the oracle authority; immutable once written for each window.
///
/// Seeds: ["platform_metrics"]
#[account]
pub struct PlatformMetrics {
    /// Rolling 30-day commercial bounty volume (AMOS tokens)
    pub commercial_volume_30d: u64, // 8 bytes

    /// Rolling 30-day total fees collected
    pub fees_collected_30d: u64, // 8 bytes

    /// Fees distributed to holders in current window
    pub fees_to_holders_30d: u64, // 8 bytes

    /// Fees burned in current window
    pub fees_burned_30d: u64, // 8 bytes

    /// Fees sent to Labs wallet in current window
    pub fees_to_labs_30d: u64, // 8 bytes

    /// Rolling 30-day system (treasury) bounty volume
    pub system_volume_30d: u64, // 8 bytes

    /// Computed profit ratio in basis points (0-10000)
    /// profit_ratio = fees_collected / system_volume (capped at 10000)
    pub profit_ratio_bps: u16, // 2 bytes

    /// Current effective decay rate in basis points (derived from profit ratio)
    /// decay = base_10% - (profit_ratio * 5%), clamped to [2%, 25%]
    pub computed_decay_rate_bps: u16, // 2 bytes

    /// Total commercial bounties in current 30-day window
    pub commercial_bounty_count: u32, // 4 bytes

    /// Total treasury bounties in current 30-day window
    pub treasury_bounty_count: u32, // 4 bytes

    /// Unix timestamp of last oracle update
    pub last_updated: i64, // 8 bytes

    /// PDA bump seed
    pub bump: u8, // 1 byte

    /// Reserved space for future upgrades
    pub reserved: [u64; 16], // 128 bytes
}

impl PlatformMetrics {
    /// Size calculation:
    /// 8 (discriminator) + 8 + 8 + 8 + 8 + 8 + 8 + 2 + 2 + 4 + 4 + 8 + 1 + 128 = 205 bytes
    pub const SIZE: usize = 8 + 8 + 8 + 8 + 8 + 8 + 8 + 2 + 2 + 4 + 4 + 8 + 1 + 128;
}

// ============================================================================
// BountyConfig - Main Program Configuration
// ============================================================================

/// The main configuration account for the AMOS Bounty program.
/// This is a singleton PDA that stores global program state.
///
/// Seeds: ["bounty_config"]
#[account]
pub struct BountyConfig {
    /// The oracle authority that validates bounty submissions
    /// This is the ONLY authority that can submit bounty proofs
    pub oracle_authority: Pubkey, // 32 bytes

    /// The AMOS token mint
    pub mint: Pubkey, // 32 bytes

    /// The treasury token account holding the distribution pool
    pub treasury: Pubkey, // 32 bytes

    /// Unix timestamp when the program was initialized
    pub start_time: i64, // 8 bytes

    /// Current halving epoch (0-10)
    /// Increments every 365 days, affects daily emission rate
    pub halving_epoch: u8, // 1 byte

    /// Current daily emission rate in tokens
    /// Starts at 16,000, halves each epoch, minimum 100
    pub daily_emission: u64, // 8 bytes

    /// Total tokens distributed across all time
    pub total_tokens_distributed: u64, // 8 bytes

    /// Total bounties submitted and approved
    pub total_bounties: u64, // 8 bytes

    /// Total points awarded across all bounties
    pub total_points: u64, // 8 bytes

    /// Annual decay rate in basis points (200-2500, default 500 = 5%)
    /// Can be adjusted by oracle within bounds
    pub decay_rate_bps: u16, // 2 bytes

    /// PDA bump seed
    pub bump: u8, // 1 byte

    /// Holder pool token account — receives 50% of commercial bounty fees
    pub holder_pool: Pubkey, // 32 bytes

    /// Labs wallet token account — receives 10% of commercial bounty fees
    pub labs_wallet: Pubkey, // 32 bytes

    /// Reserved space for future upgrades
    pub reserved: [u64; 8], // 64 bytes
}

impl BountyConfig {
    /// Size calculation:
    /// 8 (discriminator) + 32 + 32 + 32 + 8 + 1 + 8 + 8 + 8 + 8 + 2 + 1 + 32 + 32 + 64 = 276 bytes
    pub const SIZE: usize = 8 + 32 + 32 + 32 + 8 + 1 + 8 + 8 + 8 + 8 + 2 + 1 + 32 + 32 + 64;
}

// ============================================================================
// DailyPool - Daily Token Distribution Pool
// ============================================================================

/// Tracks the token distribution pool for a specific day.
/// A new DailyPool is created each day to ensure fair, time-based distribution.
///
/// Seeds: ["daily_pool", day_index.to_le_bytes()]
#[account]
pub struct DailyPool {
    /// Day index (days since program start)
    pub day_index: u32, // 4 bytes

    /// Total emission allocated for this day
    pub daily_emission: u64, // 8 bytes

    /// Total tokens already distributed from this pool
    pub tokens_distributed: u64, // 8 bytes

    /// Total points accumulated across all bounties today
    /// Used for proportional distribution calculation
    pub total_points: u64, // 8 bytes

    /// Number of bounty proofs submitted today
    pub proof_count: u32, // 4 bytes

    /// Whether this daily pool has been finalized
    /// Once finalized, no more bounties can be added
    pub finalized: bool, // 1 byte

    /// PDA bump seed
    pub bump: u8, // 1 byte

    /// Tokens distributed from the growth pool today
    pub growth_tokens_distributed: u64, // 8 bytes

    /// Total weighted points from growth bounties today
    pub growth_points: u64, // 8 bytes

    /// Tokens distributed from the technical pool today
    pub technical_tokens_distributed: u64, // 8 bytes

    /// Total weighted points from technical bounties today
    pub technical_points: u64, // 8 bytes

    /// Reserved space for future upgrades (reduced from 8 to 4 u64s)
    pub reserved: [u64; 4], // 32 bytes
}

impl DailyPool {
    /// Size calculation:
    /// 8 (discriminator) + 4 + 8 + 8 + 8 + 4 + 1 + 1 + 8 + 8 + 8 + 8 + 32 = 106 bytes
    /// (Same total: moved 32 bytes from reserved into pool tracking fields)
    pub const SIZE: usize = 8 + 4 + 8 + 8 + 8 + 4 + 1 + 1 + 8 + 8 + 8 + 8 + 32;
}

// ============================================================================
// BountyProof - Individual Bounty Submission Record
// ============================================================================

/// Records a single bounty submission with full provenance.
/// Each bounty is immutable once created, providing complete audit trail.
///
/// Seeds: ["bounty_proof", bounty_id]
#[account]
pub struct BountyProof {
    /// Unique identifier for this bounty (external system ID)
    pub bounty_id: [u8; 32], // 32 bytes

    /// Source of bounty funding (Treasury or Commercial)
    pub bounty_source: BountySource, // 1 byte (enum)

    /// Operator who earned this bounty (human or AI agent)
    pub operator: Pubkey, // 32 bytes

    /// Who funded this bounty (treasury PDA for Treasury, poster pubkey for Commercial)
    pub funded_by: Pubkey, // 32 bytes

    /// Escrow account holding Commercial bounty funds (Pubkey::default() if Treasury)
    pub escrow_account: Pubkey, // 32 bytes

    /// Base points awarded (before multipliers)
    pub base_points: u16, // 2 bytes

    /// Adjusted points (after multipliers, used for distribution)
    pub adjusted_points: u16, // 2 bytes

    /// Quality score (0-100)
    pub quality_score: u8, // 1 byte

    /// Contribution type (0-7: bug_fix, feature, docs, content, support, testing, design, infra)
    pub contribution_type: u8, // 1 byte

    /// Whether this was an AI agent submission
    pub is_agent: bool, // 1 byte

    /// Agent ID if applicable (empty if human)
    pub agent_id: [u8; 32], // 32 bytes

    /// Trust level of the operator/agent at time of submission (1-5)
    pub trust_level: u8, // 1 byte

    /// Total tokens awarded (including reviewer portion)
    pub tokens_earned: u64, // 8 bytes

    /// Protocol fee collected (0 for Treasury bounties, 3% for Commercial)
    pub fee_collected: u64, // 8 bytes

    /// Reviewer who validated this bounty
    pub reviewer: Pubkey, // 32 bytes

    /// Reviewer tokens earned (5% of total)
    pub reviewer_tokens: u64, // 8 bytes

    /// Hash of the evidence/work product
    pub evidence_hash: [u8; 32], // 32 bytes

    /// Unix timestamp of submission
    pub timestamp: i64, // 8 bytes

    /// Day index when submitted
    pub day_index: u32, // 4 bytes

    /// External reference (issue number, PR number, etc.)
    pub external_reference: [u8; 64], // 64 bytes

    /// PDA bump seed
    pub bump: u8, // 1 byte

    /// Reserved space for future upgrades
    pub reserved: [u64; 8], // 64 bytes
}

impl BountyProof {
    /// Size calculation:
    /// 8 (discriminator) + 32 + 1 + 32 + 32 + 32 + 2 + 2 + 1 + 1 + 1 + 32 + 1 + 8 + 8 + 32 + 8 + 32 + 8 + 4 + 64 + 1 + 64 = 406 bytes
    pub const SIZE: usize = 8
        + 32
        + 1
        + 32
        + 32
        + 32
        + 2
        + 2
        + 1
        + 1
        + 1
        + 32
        + 1
        + 8
        + 8
        + 32
        + 8
        + 32
        + 8
        + 4
        + 64
        + 1
        + 64;
}

// ============================================================================
// OperatorStats - Operator Performance and Balance Tracking
// ============================================================================

/// Tracks statistics and balances for each operator (human or AI agent).
/// This account enables decay mechanics and operator analytics.
///
/// Seeds: ["operator_stats", operator.key()]
#[account]
pub struct OperatorStats {
    /// Operator public key
    pub operator: Pubkey, // 32 bytes

    /// Total bounties completed
    pub total_bounties: u32, // 4 bytes

    /// Total points earned across all bounties
    pub total_points: u64, // 8 bytes

    /// Total tokens earned (before decay)
    pub total_tokens_earned: u64, // 8 bytes

    /// Current balance subject to decay
    pub decayable_balance: u64, // 8 bytes

    /// Original allocation (for floor calculation)
    pub original_allocation: u64, // 8 bytes

    /// Tokens already decayed
    pub tokens_decayed: u64, // 8 bytes

    /// Tokens burned through decay
    pub tokens_burned: u64, // 8 bytes

    /// Tokens recycled to treasury through decay
    pub tokens_recycled: u64, // 8 bytes

    /// Unix timestamp of last activity (earning or spending)
    pub last_activity_time: i64, // 8 bytes

    /// Unix timestamp of last decay application
    pub last_decay_time: i64, // 8 bytes

    /// Number of times decay has been applied
    pub decay_applications: u32, // 4 bytes

    /// Number of bounties submitted today
    pub daily_bounty_count: u16, // 2 bytes

    /// Day index of last bounty submission
    pub last_submission_day: u32, // 4 bytes

    /// Current staking vault tier (affects decay reduction)
    pub vault_tier: VaultTier, // 1 byte (enum)

    /// Unix timestamp when vault lockup expires (0 for None, u64::MAX for Permanent)
    pub vault_lockup_expires: i64, // 8 bytes

    /// Number of currently active (uncompleted) bounty claims
    /// Incremented on claim, decremented on submit/expire/abandon
    pub active_claim_count: u8, // 1 byte

    /// PDA bump seed
    pub bump: u8, // 1 byte

    /// Reserved space for future upgrades
    pub reserved: [u64; 16], // 128 bytes
}

impl OperatorStats {
    /// Size calculation:
    /// 8 (discriminator) + 32 + 4 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 4 + 2 + 4 + 1 + 8 + 1 + 1 + 128 = 261 bytes
    pub const SIZE: usize =
        8 + 32 + 4 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 4 + 2 + 4 + 1 + 8 + 1 + 1 + 128;
}

// ============================================================================
// AgentTrustRecord - AI Agent Trust and Performance Tracking
// ============================================================================

/// Tracks trust level and performance for AI agents.
/// Enables progressive trust system where agents earn higher limits over time.
///
/// Seeds: ["agent_trust", agent_id]
#[account]
pub struct AgentTrustRecord {
    /// Unique agent identifier (hash of agent properties)
    pub agent_id: [u8; 32], // 32 bytes

    /// Operator/controller of this agent
    pub operator: Pubkey, // 32 bytes

    /// Current trust level (1-5)
    /// Determines max points per bounty and daily limits
    pub trust_level: u8, // 1 byte

    /// Total completed bounties
    pub total_completions: u32, // 4 bytes

    /// Total rejected bounties
    pub total_rejections: u32, // 4 bytes

    /// Reputation score (0-10000 basis points)
    /// Calculated as: (completions * 10000) / (completions + rejections)
    pub reputation_score: u32, // 4 bytes

    /// Total tokens earned by this agent
    pub total_tokens_earned: u64, // 8 bytes

    /// Total points earned across all bounties
    pub total_points_earned: u64, // 8 bytes

    /// Unix timestamp of first registration
    pub created_at: i64, // 8 bytes

    /// Unix timestamp of last activity
    pub last_activity: i64, // 8 bytes

    /// Unix timestamp of last trust level upgrade
    pub last_upgrade: i64, // 8 bytes

    /// PDA bump seed
    pub bump: u8, // 1 byte

    /// Reserved space for future upgrades
    pub reserved: [u64; 16], // 128 bytes
}

impl AgentTrustRecord {
    /// Size calculation:
    /// 8 (discriminator) + 32 + 32 + 1 + 4 + 4 + 4 + 8 + 8 + 8 + 8 + 8 + 1 + 128 = 254 bytes
    pub const SIZE: usize = 8 + 32 + 32 + 1 + 4 + 4 + 4 + 8 + 8 + 8 + 8 + 8 + 1 + 128;

    /// Calculate reputation score from completions and rejections
    pub fn calculate_reputation(completions: u32, rejections: u32) -> u32 {
        let total = completions.saturating_add(rejections);
        if total == 0 {
            return 0;
        }
        // Reputation = (completions / total) * 10000
        (completions as u64 * 10000 / total as u64) as u32
    }
}

// ============================================================================
// BountyListing - Bounty Board Entry (Claim/Timeout/Dispute Tracking)
// ============================================================================

/// A bounty listing on the board. Tracks claim status, timeout, and disputes.
/// This is the lifecycle account for a bounty from posting through completion.
///
/// Seeds: ["bounty_listing", bounty_id]
#[account]
pub struct BountyListing {
    /// Unique bounty identifier
    pub bounty_id: [u8; 32], // 32 bytes

    /// Current status
    pub status: BountyStatus, // 1 byte

    /// Source (Treasury or Commercial)
    pub bounty_source: BountySource, // 1 byte

    /// Poster wallet (treasury PDA for system bounties)
    pub poster: Pubkey, // 32 bytes

    /// Worker who claimed this bounty (Pubkey::default() if unclaimed)
    pub claimed_by: Pubkey, // 32 bytes

    /// Unix timestamp when claimed (0 if unclaimed)
    pub claimed_at: i64, // 8 bytes

    /// Claim timeout in hours (0 = use default)
    pub claim_timeout_hours: u64, // 8 bytes

    /// Reward amount (AMOS tokens, escrow amount for commercial)
    pub reward_amount: u64, // 8 bytes

    /// Contribution type (0-10)
    pub contribution_type: u8, // 1 byte

    /// Required trust level (1-5)
    pub required_trust_level: u8, // 1 byte

    /// Unix timestamp when bounty was posted
    pub posted_at: i64, // 8 bytes

    /// Unix timestamp of deadline (for commercial bounties)
    pub deadline: i64, // 8 bytes

    /// Unix timestamp when work was submitted (0 if not submitted)
    pub submitted_at: i64, // 8 bytes

    /// Unix timestamp when rejected (0 if not rejected)
    pub rejected_at: i64, // 8 bytes

    /// PDA bump seed
    pub bump: u8, // 1 byte

    /// Reserved space
    pub reserved: [u64; 8], // 64 bytes
}

impl BountyListing {
    /// 8 (disc) + 32 + 1 + 1 + 32 + 32 + 8 + 8 + 8 + 1 + 1 + 8 + 8 + 8 + 8 + 1 + 64 = 229
    pub const SIZE: usize = 8 + 32 + 1 + 1 + 32 + 32 + 8 + 8 + 8 + 1 + 1 + 8 + 8 + 8 + 8 + 1 + 64;
}

// ============================================================================
// DisputeRecord - Tracks a dispute on a rejected bounty
// ============================================================================

/// Records a dispute filed by a worker after bounty rejection.
///
/// Seeds: ["dispute", bounty_id]
#[account]
pub struct DisputeRecord {
    /// The bounty being disputed
    pub bounty_id: [u8; 32], // 32 bytes

    /// Worker who filed the dispute
    pub worker: Pubkey, // 32 bytes

    /// Stake amount (5% of bounty value, locked during dispute)
    pub stake_amount: u64, // 8 bytes

    /// Unix timestamp when dispute was filed
    pub filed_at: i64, // 8 bytes

    /// Unix timestamp when resolved (0 if pending)
    pub resolved_at: i64, // 8 bytes

    /// Resolution: true = upheld (worker wins), false = denied (reviewer wins)
    pub upheld: bool, // 1 byte

    /// Whether this dispute has been resolved
    pub is_resolved: bool, // 1 byte

    /// Resolver authority (governance initially)
    pub resolver: Pubkey, // 32 bytes

    /// PDA bump seed
    pub bump: u8, // 1 byte

    /// Reserved space
    pub reserved: [u64; 8], // 64 bytes
}

impl DisputeRecord {
    /// 8 (disc) + 32 + 32 + 8 + 8 + 8 + 1 + 1 + 32 + 1 + 64 = 195
    pub const SIZE: usize = 8 + 32 + 32 + 8 + 8 + 8 + 1 + 1 + 32 + 1 + 64;
}

// ============================================================================
// ContributionTypeRegistry - Governance-updatable with graduated freeze
// ============================================================================

/// A single entry in the contribution type registry.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub struct ContributionTypeEntry {
    /// Type ID (0-31)
    pub type_id: u8,
    /// Fixed-size name (padded with zeros)
    pub name: [u8; 32],
    /// Multiplier in basis points
    pub multiplier_bps: u16,
    /// Pool category (Technical or Growth)
    pub pool_category: PoolCategory,
    /// Whether this entry is active
    pub is_active: bool,
    /// One-way freeze: once true, this entry is IMMUTABLE
    pub frozen: bool,
    /// Unix timestamp when added
    pub added_at: i64,
    /// Unix timestamp when frozen (0 if not frozen)
    pub frozen_at: i64,
}

impl Default for ContributionTypeEntry {
    fn default() -> Self {
        Self {
            type_id: 0,
            name: [0u8; 32],
            multiplier_bps: 0,
            pool_category: PoolCategory::Technical,
            is_active: false,
            frozen: false,
            added_at: 0,
            frozen_at: 0,
        }
    }
}

impl ContributionTypeEntry {
    /// Size: 1 + 32 + 2 + 1 + 1 + 1 + 8 + 8 = 54 bytes
    pub const SIZE: usize = 1 + 32 + 2 + 1 + 1 + 1 + 8 + 8;
}

/// Governance-updatable registry of contribution types with graduated freeze.
///
/// Seeds: ["contribution_registry"]
#[account]
pub struct ContributionTypeRegistry {
    /// PDA bump seed
    pub bump: u8, // 1

    /// Governance authority PDA
    pub authority: Pubkey, // 32

    /// One-way: once true, NO changes to ANY entry, ever
    pub registry_frozen: bool, // 1

    /// Unix timestamp when registry was frozen (0 if not frozen)
    pub registry_frozen_at: i64, // 8

    /// Number of active entries
    pub entry_count: u8, // 1

    /// Fixed array of contribution type entries (max 16)
    pub entries: [ContributionTypeEntry; 16], // 16 * 54 = 864

    /// Minimum BPS for technical pool (8000 = 80%)
    pub pool_technical_min_bps: u16, // 2

    /// Maximum BPS for growth pool (2000 = 20%)
    pub pool_growth_max_bps: u16, // 2

    /// Auto-freeze deadline (launch_timestamp + 3 years)
    pub freeze_deadline: i64, // 8

    /// Number of extensions used (max 2)
    pub extensions_used: u8, // 1

    /// Maximum extensions allowed (hardcoded 2)
    pub max_extensions: u8, // 1

    /// Extension duration in seconds (exactly 1 year)
    pub extension_duration_seconds: i64, // 8

    /// Growth phase 1 end timestamp
    pub growth_phase_1_end: i64, // 8

    /// Growth phase 1 cap (BPS)
    pub growth_phase_1_cap_bps: u16, // 2

    /// Growth phase 2 end timestamp
    pub growth_phase_2_end: i64, // 8

    /// Growth phase 2 cap (BPS)
    pub growth_phase_2_cap_bps: u16, // 2

    /// Growth phase 3 end timestamp
    pub growth_phase_3_end: i64, // 8

    /// Growth phase 3 cap (BPS)
    pub growth_phase_3_cap_bps: u16, // 2

    /// Growth phase 4 cap (BPS, permanent)
    pub growth_phase_4_cap_bps: u16, // 2

    /// Reserved space
    pub reserved: [u64; 16], // 128
}

impl ContributionTypeRegistry {
    /// 8 (disc) + 1 + 32 + 1 + 8 + 1 + 864 + 2 + 2 + 8 + 1 + 1 + 8 + 8 + 2 + 8 + 2 + 8 + 2 + 2 + 128 = 1097
    pub const SIZE: usize = 8
        + 1
        + 32
        + 1
        + 8
        + 1
        + (16 * ContributionTypeEntry::SIZE)
        + 2
        + 2
        + 8
        + 1
        + 1
        + 8
        + 8
        + 2
        + 8
        + 2
        + 8
        + 2
        + 2
        + 128;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_sizes() {
        // Verify account sizes are within Solana limits (10KB max)
        assert!(PlatformMetrics::SIZE < 10240);
        assert!(BountyConfig::SIZE < 10240);
        assert!(DailyPool::SIZE < 10240);
        assert!(BountyProof::SIZE < 10240);
        assert!(OperatorStats::SIZE < 10240);
        assert!(AgentTrustRecord::SIZE < 10240);
        assert!(BountyListing::SIZE < 10240);
        assert!(DisputeRecord::SIZE < 10240);
        assert!(ContributionTypeRegistry::SIZE < 10240);
    }

    #[test]
    fn test_reputation_calculation() {
        // Perfect record
        assert_eq!(AgentTrustRecord::calculate_reputation(10, 0), 10000);

        // 90% success rate
        assert_eq!(AgentTrustRecord::calculate_reputation(9, 1), 9000);

        // 50% success rate
        assert_eq!(AgentTrustRecord::calculate_reputation(5, 5), 5000);

        // No activity yet
        assert_eq!(AgentTrustRecord::calculate_reputation(0, 0), 0);

        // All failures
        assert_eq!(AgentTrustRecord::calculate_reputation(0, 10), 0);
    }
}
