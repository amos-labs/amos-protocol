/// AMOS Bounty Program
///
/// A trustless, transparent token distribution system for rewarding contributor work.
///
/// # Core Features
///
/// 1. **Proportional Distribution**
///    - Daily emission pool divided among all contributors
///    - Tokens = (adjusted_points / total_points_today) × remaining_emission
///    - Fair share based on contribution value
///
/// 2. **Sigmoid Emission Schedule**
///    - Starts at ~16,000 tokens/day
///    - Smoothly decays via sigmoid curve (midpoint at ~4 years)
///    - Minimum floor of 100 tokens/day
///    - No discrete halving events — fully stateless computation
///
/// 3. **Token Decay**
///    - Recycles unused tokens back to treasury
///    - 90-day grace period before decay begins
///    - 2-25% annual rate (default 5%)
///    - 10% floor preserved
///    - 10% burned, 90% recycled
///
/// 4. **AI Agent Trust System**
///    - Progressive trust levels (1-5)
///    - Higher levels unlock higher point caps and daily limits
///    - Upgrades based on on-chain performance metrics
///    - Reputation = (completions × 10000) / total_attempts
///
/// 5. **Contribution Multipliers**
///    - Bug fixes: 120%
///    - Features: 100%
///    - Documentation: 80%
///    - Content: 90%
///    - Support: 70%
///    - Testing: 110%
///    - Design: 100%
///    - Infrastructure: 130%
///
/// # Trustless Guarantees
///
/// - Oracle validates work but cannot manipulate distribution math
/// - All parameters bounded by protocol constants
/// - Permissionless operations (anyone can trigger decay, upgrades)
/// - Complete on-chain audit trail
/// - All arithmetic uses checked operations (no overflow/underflow)
/// - Immutable records (cannot alter history)
///
/// # Security Model
///
/// - Oracle authority: Validates bounty submissions (read: proves work was done)
/// - Token distribution: Pure math based on proportional share
/// - Trust upgrades: On-chain threshold verification
/// - Decay: Time-locked and rate-limited by protocol
/// - All critical values bounded by MIN/MAX constants
use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq");

#[program]
pub mod amos_bounty {
    use super::*;

    // ========================================================================
    // Admin Instructions
    // ========================================================================

    /// Initialize the AMOS Bounty program
    ///
    /// Sets up the singleton configuration with:
    /// - Oracle authority for bounty validation
    /// - Token mint and treasury references
    /// - Sigmoid emission: 16,000 → 100 AMOS/day (no halving epochs)
    /// - Default decay rate (5% annual)
    ///
    /// This can only be called once.
    pub fn initialize(ctx: Context<Initialize>, oracle_authority: Pubkey) -> Result<()> {
        instructions::admin::handler_initialize(ctx, oracle_authority)
    }

    /// Update the annual decay rate
    ///
    /// Oracle can adjust the decay rate within protocol bounds (2-25%).
    /// This affects how quickly unused tokens recycle to treasury.
    ///
    /// # Arguments
    /// * `new_rate_bps` - New rate in basis points (200-2500)
    pub fn update_decay_rate(ctx: Context<UpdateDecayRate>, new_rate_bps: u16) -> Result<()> {
        instructions::admin::handler_update_decay(ctx, new_rate_bps)
    }

    /// Update the treasury token account address. Oracle-only.
    pub fn update_treasury(ctx: Context<UpdateTreasury>) -> Result<()> {
        instructions::admin::handler_update_treasury(ctx)
    }

    /// Set fee recipient addresses for commercial bounty fee distribution.
    /// Oracle-only. Must be called before any commercial bounty can be released.
    ///
    /// # Arguments
    /// * `holder_pool` - Token account receiving 50% of protocol fees
    /// * `labs_wallet` - Token account receiving 10% of protocol fees
    pub fn set_fee_recipients(ctx: Context<SetFeeRecipients>) -> Result<()> {
        instructions::admin::handler_set_fee_recipients(ctx)
    }

    // ========================================================================
    // Preparation Instructions
    // ========================================================================

    /// Prepare accounts for bounty submission.
    ///
    /// Creates daily_pool and operator_stats accounts if they don't exist.
    /// Must be called before submit_bounty_proof or release_commercial_bounty
    /// in the same transaction. Idempotent.
    ///
    /// # Arguments
    /// * `operator_key` - The operator's public key (for operator_stats PDA)
    /// * `day_index` - Current day index since program start (validated on-chain)
    pub fn prepare_bounty_submission(
        ctx: Context<PrepareBountySubmission>,
        operator_key: Pubkey,
        day_index: u32,
    ) -> Result<()> {
        instructions::prepare::handler_prepare(ctx, operator_key, day_index)
    }

    // ========================================================================
    // Distribution Instructions
    // ========================================================================

    /// Submit a bounty proof and distribute tokens
    ///
    /// This is the CORE distribution function. Only the oracle can call this,
    /// but the distribution is pure math based on contribution value.
    ///
    /// Prerequisites: `prepare_bounty_submission` must be called first.
    ///
    /// Token allocation formula:
    /// `tokens = (adjusted_points / total_points_today) × remaining_emission`
    ///
    /// Where adjusted_points = base_points × contribution_type_multiplier
    ///
    /// # Arguments
    /// * `bounty_id` - Unique identifier (32 bytes)
    /// * `base_points` - Base point value before multipliers (1-2000)
    /// * `quality_score` - Quality assessment (30-100)
    /// * `contribution_type` - Type of work (0-7)
    /// * `is_agent` - Whether this is an AI agent submission
    /// * `agent_id` - Agent identifier if applicable
    /// * `day_index` - Current day index since program start
    /// * `max_reward` - Maximum token payout in lamports (0 = no cap)
    /// * `reviewer` - Validator who approved this work
    /// * `evidence_hash` - Hash of the work product
    /// * `external_reference` - External ID (issue #, PR #, etc.)
    #[allow(clippy::too_many_arguments)]
    pub fn submit_bounty_proof(
        ctx: Context<SubmitBountyProof>,
        bounty_id: [u8; 32],
        base_points: u16,
        quality_score: u8,
        contribution_type: u8,
        is_agent: bool,
        agent_id: [u8; 32],
        day_index: u32,
        max_reward: u64,
        reviewer: Pubkey,
        evidence_hash: [u8; 32],
        external_reference: [u8; 64],
    ) -> Result<()> {
        instructions::distribution::handler_submit_proof(
            ctx,
            bounty_id,
            base_points,
            quality_score,
            contribution_type,
            is_agent,
            agent_id,
            day_index,
            max_reward,
            reviewer,
            evidence_hash,
            external_reference,
        )
    }

    // ========================================================================
    // Commercial Bounty (Escrow) Instructions
    // ========================================================================

    /// Create a commercial bounty by escrowing AMOS tokens.
    ///
    /// The poster deposits tokens into a PDA escrow. On completion, a 3% fee
    /// is deducted (50% holders, 40% burned, 10% Labs). On expiry, poster
    /// can reclaim full amount with zero fee.
    ///
    /// # Arguments
    /// * `bounty_id` - Unique identifier for this bounty
    /// * `reward_amount` - Total AMOS tokens to escrow
    /// * `deadline` - Unix timestamp after which poster can reclaim
    pub fn create_commercial_bounty(
        ctx: Context<CreateCommercialBounty>,
        bounty_id: [u8; 32],
        reward_amount: u64,
        deadline: i64,
    ) -> Result<()> {
        instructions::escrow::handler_create_commercial_bounty(
            ctx,
            bounty_id,
            reward_amount,
            deadline,
        )
    }

    /// Release escrowed funds to the worker after oracle validates completion.
    /// 3% protocol fee is deducted and distributed per the 50/40/10 split.
    ///
    /// Prerequisites: `prepare_bounty_submission` must be called first.
    /// Fee recipient accounts (holder_pool, labs_wallet) are validated against
    /// the addresses stored in BountyConfig via `set_fee_recipients`.
    #[allow(clippy::too_many_arguments)]
    pub fn release_commercial_bounty(
        ctx: Context<ReleaseEscrow>,
        bounty_id: [u8; 32],
        base_points: u16,
        quality_score: u8,
        contribution_type: u8,
        is_agent: bool,
        agent_id: [u8; 32],
        reviewer: Pubkey,
        evidence_hash: [u8; 32],
        external_reference: [u8; 64],
    ) -> Result<()> {
        instructions::escrow::handler_release_escrow(
            ctx,
            bounty_id,
            base_points,
            quality_score,
            contribution_type,
            is_agent,
            agent_id,
            reviewer,
            evidence_hash,
            external_reference,
        )
    }

    /// Refund escrowed funds to the poster if bounty was not completed.
    /// No fee is charged on refunds.
    pub fn refund_commercial_bounty(ctx: Context<RefundEscrow>, bounty_id: [u8; 32]) -> Result<()> {
        instructions::escrow::handler_refund_escrow(ctx, bounty_id)
    }

    // ========================================================================
    // Decay Instructions
    // ========================================================================

    /// Apply decay to an operator's balance
    ///
    /// This is a PUBLIC GOOD function - anyone can trigger it to keep
    /// the system healthy. Decayed tokens are split: 10% burned, 90% recycled.
    ///
    /// Decay only applies after:
    /// - 90-day grace period since last activity
    /// - Balance is above 10% floor
    ///
    /// This is a PERMISSIONLESS operation.
    pub fn apply_decay(ctx: Context<ApplyDecay>) -> Result<()> {
        instructions::decay::handler_apply_decay(ctx)
    }

    // ========================================================================
    // Platform Metrics Instructions
    // ========================================================================

    /// Initialize the PlatformMetrics singleton.
    /// Oracle-only. Must be called once after program initialization.
    pub fn initialize_platform_metrics(ctx: Context<InitializePlatformMetrics>) -> Result<()> {
        instructions::metrics::handler_initialize_metrics(ctx)
    }

    /// Update rolling 30-day platform metrics and recompute decay rate.
    /// Oracle pushes off-chain computed metrics on-chain for transparency.
    ///
    /// Decay formula: base_10% - (profit_ratio × 5%), clamped [2%, 25%]
    #[allow(clippy::too_many_arguments)]
    pub fn update_platform_metrics(
        ctx: Context<UpdatePlatformMetrics>,
        commercial_volume_30d: u64,
        fees_collected_30d: u64,
        fees_to_holders_30d: u64,
        fees_burned_30d: u64,
        fees_to_labs_30d: u64,
        system_volume_30d: u64,
        commercial_bounty_count: u32,
        treasury_bounty_count: u32,
    ) -> Result<()> {
        instructions::metrics::handler_update_metrics(
            ctx,
            commercial_volume_30d,
            fees_collected_30d,
            fees_to_holders_30d,
            fees_burned_30d,
            fees_to_labs_30d,
            system_volume_30d,
            commercial_bounty_count,
            treasury_bounty_count,
        )
    }

    // ========================================================================
    // Trust System Instructions
    // ========================================================================

    /// Register a new AI agent in the trust system
    pub fn register_agent_trust(
        ctx: Context<RegisterAgentTrust>,
        agent_id: [u8; 32],
    ) -> Result<()> {
        instructions::trust::handler_register_agent(ctx, agent_id)
    }

    /// Record the outcome of an agent's bounty submission
    pub fn record_agent_completion(
        ctx: Context<RecordAgentCompletion>,
        agent_id: [u8; 32],
        approved: bool,
        tokens_earned: u64,
    ) -> Result<()> {
        instructions::trust::handler_record_completion(ctx, agent_id, approved, tokens_earned)
    }

    /// Upgrade an agent's trust level (PERMISSIONLESS)
    pub fn upgrade_trust_level(ctx: Context<UpgradeTrustLevel>, agent_id: [u8; 32]) -> Result<()> {
        instructions::trust::handler_upgrade_trust(ctx, agent_id)
    }

    // ========================================================================
    // Bounty Board — Claim, Timeout, and Lifecycle Instructions
    // ========================================================================

    /// Post a bounty listing to the board (system or commercial)
    #[allow(clippy::too_many_arguments)]
    pub fn post_bounty_listing(
        ctx: Context<PostBountyListing>,
        bounty_id: [u8; 32],
        bounty_source: u8,
        reward_amount: u64,
        contribution_type: u8,
        required_trust_level: u8,
        claim_timeout_hours: u64,
        deadline: i64,
    ) -> Result<()> {
        instructions::claims::handler_post_bounty_listing(
            ctx,
            bounty_id,
            bounty_source,
            reward_amount,
            contribution_type,
            required_trust_level,
            claim_timeout_hours,
            deadline,
        )
    }

    /// Claim an open bounty. Enforces concurrent claim limits per trust level.
    pub fn claim_bounty(ctx: Context<ClaimBounty>, bounty_id: [u8; 32]) -> Result<()> {
        instructions::claims::handler_claim_bounty(ctx, bounty_id)
    }

    /// Release an expired claim back to the board (PERMISSIONLESS, no reputation penalty)
    pub fn release_expired_claim(
        ctx: Context<ReleaseExpiredClaim>,
        bounty_id: [u8; 32],
    ) -> Result<()> {
        instructions::claims::handler_release_expired_claim(ctx, bounty_id)
    }

    // ========================================================================
    // Dispute Instructions
    // ========================================================================

    /// File a dispute after bounty rejection (within 48h, stakes 5% of value)
    pub fn file_dispute(ctx: Context<FileDispute>, bounty_id: [u8; 32]) -> Result<()> {
        instructions::dispute::handler_file_dispute(ctx, bounty_id)
    }

    /// Resolve a dispute (governance authority only)
    pub fn resolve_dispute(
        ctx: Context<ResolveDispute>,
        bounty_id: [u8; 32],
        upheld: bool,
    ) -> Result<()> {
        instructions::dispute::handler_resolve_dispute(ctx, bounty_id, upheld)
    }

    /// Default dispute resolution after 7-day timeout (PERMISSIONLESS, worker-favorable)
    pub fn default_dispute_resolution(
        ctx: Context<DefaultDisputeResolution>,
        bounty_id: [u8; 32],
    ) -> Result<()> {
        instructions::dispute::handler_default_dispute_resolution(ctx, bounty_id)
    }

    // ========================================================================
    // Contribution Type Registry Instructions
    // ========================================================================

    /// Initialize the ContributionTypeRegistry with 11 seed types
    pub fn initialize_registry(ctx: Context<InitializeRegistry>) -> Result<()> {
        instructions::registry::handler_initialize_registry(ctx)
    }

    /// Add a new contribution type to the registry (governance only)
    pub fn add_contribution_type(
        ctx: Context<AddContributionType>,
        name: [u8; 32],
        multiplier_bps: u16,
        pool_category: u8,
    ) -> Result<()> {
        instructions::registry::handler_add_contribution_type(
            ctx,
            name,
            multiplier_bps,
            pool_category,
        )
    }

    /// Update a contribution type's multiplier or category (governance only)
    pub fn update_contribution_type(
        ctx: Context<UpdateContributionType>,
        type_id: u8,
        multiplier_bps: u16,
        pool_category: u8,
    ) -> Result<()> {
        instructions::registry::handler_update_contribution_type(
            ctx,
            type_id,
            multiplier_bps,
            pool_category,
        )
    }

    /// Freeze a single contribution type entry (ONE-WAY, irreversible)
    pub fn freeze_entry(ctx: Context<FreezeEntry>, type_id: u8) -> Result<()> {
        instructions::registry::handler_freeze_entry(ctx, type_id)
    }

    /// Freeze the entire registry (ONE-WAY, nuclear option)
    pub fn freeze_registry(ctx: Context<FreezeRegistry>) -> Result<()> {
        instructions::registry::handler_freeze_registry(ctx)
    }

    /// Auto-freeze registry after deadline (PERMISSIONLESS)
    pub fn auto_freeze_registry(ctx: Context<AutoFreezeRegistry>) -> Result<()> {
        instructions::registry::handler_auto_freeze_registry(ctx)
    }

    /// Extend freeze deadline by 1 year (governance only, max 2 extensions)
    pub fn extend_freeze_deadline(ctx: Context<ExtendFreezeDeadline>) -> Result<()> {
        instructions::registry::handler_extend_freeze_deadline(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_id() {
        // Verify program ID is set
        let program_id = id();
        assert_ne!(program_id, Pubkey::default());
    }

    #[test]
    fn test_constants_invariants() {
        use crate::constants::*;

        // Treasury allocation should be 95% of total supply
        assert_eq!(TREASURY_ALLOCATION, TOTAL_SUPPLY * 95 / 100);

        // Decay rate bounds are valid
        assert!(MIN_DECAY_RATE_BPS < MAX_DECAY_RATE_BPS);
        assert!(DEFAULT_DECAY_RATE_BPS >= MIN_DECAY_RATE_BPS);
        assert!(DEFAULT_DECAY_RATE_BPS <= MAX_DECAY_RATE_BPS);

        // Sigmoid emission parameters are sensible
        assert!(EMISSION_FLOOR > 0);
        assert!(EMISSION_CEILING > EMISSION_FLOOR);
        assert!(EMISSION_MIDPOINT_DAYS > 0);
        assert!(EMISSION_K_SCALED > 0);

        // Trust levels are properly configured
        assert_eq!(TRUST_LEVEL_MAX_POINTS.len(), 5);
        assert_eq!(TRUST_LEVEL_DAILY_LIMITS.len(), 5);

        // Max points increase with each level
        for i in 0..4 {
            assert!(TRUST_LEVEL_MAX_POINTS[i] < TRUST_LEVEL_MAX_POINTS[i + 1]);
            assert!(TRUST_LEVEL_DAILY_LIMITS[i] < TRUST_LEVEL_DAILY_LIMITS[i + 1]);
        }

        // Contribution multipliers are valid (8 technical + 3 growth = 11 types)
        for i in 0..=10 {
            let multiplier = get_contribution_multiplier(i).unwrap();
            assert!(multiplier > 0);
            assert!(multiplier <= 15000); // Max 150%
        }
    }

    #[test]
    fn test_distribution_math() {
        // Test proportional distribution calculation
        let adjusted_points = 100u64;
        let total_points = 1000u64;
        let remaining_emission = 10000u64;

        let tokens = (adjusted_points * remaining_emission) / total_points;

        // Should get 10% of remaining emission
        assert_eq!(tokens, 1000);
    }

    #[test]
    fn test_reviewer_split() {
        use crate::constants::*;

        let total_tokens = 10000u64;
        let reviewer_tokens = total_tokens * REVIEWER_REWARD_BPS as u64 / BPS_DENOMINATOR as u64;
        let operator_tokens = total_tokens - reviewer_tokens;

        // Should be 5% to reviewer, 95% to operator
        assert_eq!(reviewer_tokens, 500);
        assert_eq!(operator_tokens, 9500);
    }

    #[test]
    fn test_decay_calculation() {
        use crate::constants::*;

        // Test: 10,000 token balance, 5% annual rate, 30 days
        let balance = 10000u64;
        let rate_bps = 500u16; // 5%
        let days = 30u64;

        let decay = (balance * rate_bps as u64 * days) / (10000 * 365);

        // Should be approximately 41 tokens (10000 × 0.05 / 365 × 30)
        assert!(decay >= 40 && decay <= 42);
    }

    #[test]
    fn test_decay_split() {
        use crate::constants::*;

        let decay_amount = 1000u64;
        let burn = decay_amount * DECAY_BURN_PORTION_BPS as u64 / BPS_DENOMINATOR as u64;
        let recycle = decay_amount - burn;

        // Should be 10% burned, 90% recycled
        assert_eq!(burn, 100);
        assert_eq!(recycle, 900);
    }

    #[test]
    fn test_reputation_calculation() {
        use crate::state::AgentTrustRecord;

        // Perfect record: 10/10 = 100%
        assert_eq!(AgentTrustRecord::calculate_reputation(10, 0), 10000);

        // Good record: 9/10 = 90%
        assert_eq!(AgentTrustRecord::calculate_reputation(9, 1), 9000);

        // Average record: 50/100 = 50%
        assert_eq!(AgentTrustRecord::calculate_reputation(50, 50), 5000);

        // No activity: 0/0 = 0%
        assert_eq!(AgentTrustRecord::calculate_reputation(0, 0), 0);
    }

    #[test]
    fn test_trust_level_upgrades() {
        use crate::constants::*;

        // Level 1 → 2: Requires 3 completions and 5500 reputation
        assert!(can_upgrade_to_level(1, 3, 5500).unwrap());
        assert!(!can_upgrade_to_level(1, 2, 5500).unwrap());
        assert!(!can_upgrade_to_level(1, 3, 5499).unwrap());

        // Level 4 → 5: Requires 50 completions and 8500 reputation
        assert!(can_upgrade_to_level(4, 50, 8500).unwrap());
        assert!(!can_upgrade_to_level(4, 49, 8500).unwrap());
        assert!(!can_upgrade_to_level(4, 50, 8499).unwrap());

        // Level 5 cannot upgrade further
        assert!(!can_upgrade_to_level(5, 1000, 10000).unwrap());
    }
}
