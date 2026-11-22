/// AMOS Bounty Program Constants
///
/// This module defines all the constants that govern the trustless token distribution
/// system. These values are hardcoded on-chain and ensure predictable, transparent
/// operation subject to the oracle and configured administrative authorities.
use anchor_lang::prelude::*;

// ============================================================================
// Token Economics Constants
// ============================================================================

/// Intended total supply in raw units (100 million AMOS)
pub const TOTAL_SUPPLY: u64 = 100_000_000 * ONE_TOKEN;

/// Bounty treasury allocation in raw units (95 million AMOS)
/// This is the pool from which bounties are distributed
pub const TREASURY_ALLOCATION: u64 = 95_000_000 * ONE_TOKEN;

// ============================================================================
// Sigmoid Emission Schedule
//
// emission(t) = floor + (ceiling - floor) / (1 + e^(k × (t - midpoint)))
//
// Smooth decay from a ceiling of 16,000 AMOS/day at launch to 100 AMOS/day
// floor. No discrete halving events. No epochs. Emission is computed directly
// from elapsed time since launch using the same integer sigmoid math
// (shared fixed-point kernel) used for pool separation.
// ============================================================================

/// Token decimal places (must match SPL mint)
pub const TOKEN_DECIMALS: u32 = 9;

/// One whole AMOS token in lamports (10^9)
pub const ONE_TOKEN: u64 = 10u64.pow(TOKEN_DECIMALS);

/// Maximum daily emission at launch (16,000 whole AMOS tokens per day)
pub const EMISSION_CEILING: u64 = 16_000 * ONE_TOKEN;

/// Minimum daily emission floor (100 whole AMOS tokens per day)
/// Scheduled floor; payouts still require a funded treasury
pub const EMISSION_FLOOR: u64 = 100 * ONE_TOKEN;

/// Sigmoid midpoint in days (~4 years)
/// At this point, daily emission is halfway between ceiling and floor (~8,050/day)
pub const EMISSION_MIDPOINT_DAYS: u64 = 1_460;

/// Sigmoid steepness parameter × 10,000
/// 50 = k of 0.005. Lower = gentler curve, higher = steeper
pub const EMISSION_K_SCALED: u64 = 50;

// Governance bounds for emission parameters (prevent capture via parameter manipulation)

/// Minimum allowed EMISSION_K_SCALED (prevents flattening curve to near-linear)
pub const EMISSION_K_MIN: u64 = 20;
/// Maximum allowed EMISSION_K_SCALED (prevents creating quasi-halving cliff)
pub const EMISSION_K_MAX: u64 = 200;
/// Minimum allowed midpoint in days (can't make transition immediate)
pub const EMISSION_MIDPOINT_MIN_DAYS: u64 = 730;
/// Maximum allowed midpoint in days (can't push transition to infinity)
pub const EMISSION_MIDPOINT_MAX_DAYS: u64 = 3_650;

// ============================================================================
// Protocol Fee Constants (must match amos-treasury/src/constants.rs)
// ============================================================================

/// Protocol fee rate: 3% of commercial bounty payout
pub const PROTOCOL_FEE_BPS: u16 = 300;

/// 50% of fee → staked token holders
pub const FEE_HOLDER_SHARE_BPS: u16 = 5000;

/// 40% of fee → permanently burned
pub const FEE_BURN_SHARE_BPS: u16 = 4000;

/// 10% of fee → AMOS Labs operating wallet
pub const FEE_LABS_SHARE_BPS: u16 = 1000;

// ============================================================================
// Decay Mechanism Constants
// ============================================================================

/// Minimum decay rate in basis points (2% annual = 200 bps)
pub const MIN_DECAY_RATE_BPS: u16 = 200;

/// Maximum decay rate in basis points (25% annual = 2500 bps)
pub const MAX_DECAY_RATE_BPS: u16 = 2500;

/// Base annual decay rate (10% = 1000 bps)
/// Formula: Decay = 10% - (Profit_Ratio * 5%), clamped to [MIN, MAX]
pub const BASE_DECAY_RATE_BPS: u16 = 1000;

/// Default decay rate in basis points (5% annual = 500 bps)
pub const DEFAULT_DECAY_RATE_BPS: u16 = 500;

/// Profit ratio multiplier for decay formula (5% = 500 bps)
pub const DECAY_PROFIT_MULTIPLIER_BPS: u16 = 500;

/// Inactivity grace: days without bounty completion before decay triggers
pub const INACTIVITY_GRACE_PERIOD_DAYS: u64 = 90;

/// New stake grace: days after earning tokens during which they don't decay
pub const NEW_STAKE_GRACE_PERIOD_DAYS: u64 = 365;

/// Decay floor - minimum portion preserved (10% = 1000 bps)
/// At most 90% of original allocation can decay
pub const DECAY_FLOOR_BPS: u16 = 1000;

/// Portion of decayed tokens that are burned (10% = 1000 bps)
/// The remaining 90% is recycled back to treasury
pub const DECAY_BURN_PORTION_BPS: u16 = 1000;

// ============================================================================
// Tenure-Based Decay Floors (immutable social contract with long-term holders)
// ============================================================================

/// Year 0-1: 5% permanent floor
pub const TENURE_FLOOR_YEAR_0_BPS: u16 = 500;
/// Year 1-2: 10% permanent floor
pub const TENURE_FLOOR_YEAR_1_BPS: u16 = 1000;
/// Year 2-5: 15% permanent floor
pub const TENURE_FLOOR_YEAR_2_BPS: u16 = 1500;
/// Year 5+: 25% permanent floor
pub const TENURE_FLOOR_YEAR_5_BPS: u16 = 2500;

// ============================================================================
// Tenure-Based Decay Reduction
// ============================================================================

/// Year 0-1: 0% reduction (full decay)
pub const TENURE_REDUCTION_YEAR_0_BPS: u16 = 0;
/// Year 1-2: 20% reduction
pub const TENURE_REDUCTION_YEAR_1_BPS: u16 = 2000;
/// Year 2-5: 40% reduction
pub const TENURE_REDUCTION_YEAR_2_BPS: u16 = 4000;
/// Year 5+: 70% reduction
pub const TENURE_REDUCTION_YEAR_5_BPS: u16 = 7000;

// ============================================================================
// Staking Vault Tiers — Lockup periods and decay reduction bonuses
// ============================================================================

/// Bronze vault: 30-day lockup, 20% decay reduction
pub const VAULT_BRONZE_LOCKUP_DAYS: u64 = 30;
pub const VAULT_BRONZE_REDUCTION_BPS: u16 = 2000;

/// Silver vault: 90-day lockup, 50% decay reduction
pub const VAULT_SILVER_LOCKUP_DAYS: u64 = 90;
pub const VAULT_SILVER_REDUCTION_BPS: u16 = 5000;

/// Gold vault: 365-day lockup, 80% decay reduction
pub const VAULT_GOLD_LOCKUP_DAYS: u64 = 365;
pub const VAULT_GOLD_REDUCTION_BPS: u16 = 8000;

/// Permanent vault: no unlock, 95% decay reduction
pub const VAULT_PERMANENT_LOCKUP_DAYS: u64 = u64::MAX;
pub const VAULT_PERMANENT_REDUCTION_BPS: u16 = 9500;

// ============================================================================
// Bounty Validation Constants
// ============================================================================

/// Minimum quality score required for bounty acceptance (0-100 scale)
pub const MIN_QUALITY_SCORE: u8 = 30;

/// Maximum points that can be awarded for a single bounty
pub const MAX_BOUNTY_POINTS: u16 = 2000;

/// Minimum escrow amount for commercial bounties (100 AMOS, nine decimals)
/// Prevents dust bounties that waste compute and bloat account space
pub const MIN_COMMERCIAL_ESCROW: u64 = 100 * amos_protocol_math::TOKEN_UNIT;

/// Legacy unused global limit; active agent quotas use TRUST_LEVEL_DAILY_LIMITS.
pub const MAX_DAILY_BOUNTIES_PER_OPERATOR: u16 = 50;

// ============================================================================
// Agent Trust System Constants
// ============================================================================

/// Trust Level 2 requirements
pub const TRUST_LEVEL_2_MIN_COMPLETIONS: u32 = 3;
pub const TRUST_LEVEL_2_MIN_REPUTATION: u32 = 5500;

/// Trust Level 3 requirements
pub const TRUST_LEVEL_3_MIN_COMPLETIONS: u32 = 10;
pub const TRUST_LEVEL_3_MIN_REPUTATION: u32 = 6500;

/// Trust Level 4 requirements
pub const TRUST_LEVEL_4_MIN_COMPLETIONS: u32 = 25;
pub const TRUST_LEVEL_4_MIN_REPUTATION: u32 = 7500;

/// Trust Level 5 requirements
pub const TRUST_LEVEL_5_MIN_COMPLETIONS: u32 = 50;
pub const TRUST_LEVEL_5_MIN_REPUTATION: u32 = 8500;

/// Maximum points per bounty for each trust level [L1, L2, L3, L4, L5]
pub const TRUST_LEVEL_MAX_POINTS: [u16; 5] = [100, 200, 500, 1000, 2000];

/// Daily bounty limits for each trust level
pub const TRUST_LEVEL_DAILY_LIMITS: [u16; 5] = [3, 5, 10, 15, 25];

// ============================================================================
// Contribution Type Multipliers
// ============================================================================

/// These multipliers adjust the base points based on contribution type
/// All multipliers are in basis points (10000 = 100%)

/// Bug fixes and security patches - 120%
pub const MULTIPLIER_BUG_FIX_BPS: u16 = 12000;

/// New features and enhancements - 100%
pub const MULTIPLIER_FEATURE_BPS: u16 = 10000;

/// Documentation contributions - 80%
pub const MULTIPLIER_DOCUMENTATION_BPS: u16 = 8000;

/// Content creation (articles, videos) - 90%
pub const MULTIPLIER_CONTENT_BPS: u16 = 9000;

/// Support and community help - 70%
pub const MULTIPLIER_SUPPORT_BPS: u16 = 7000;

/// Testing and QA work - 110%
pub const MULTIPLIER_TESTING_BPS: u16 = 11000;

/// Design work (UI/UX) - 100%
pub const MULTIPLIER_DESIGN_BPS: u16 = 10000;

/// Infrastructure and DevOps - 130%
pub const MULTIPLIER_INFRASTRUCTURE_BPS: u16 = 13000;

// ============================================================================
// Growth Bounty Multipliers (Growth Pool)
// ============================================================================

/// Bug report: 100% (finding real bugs is high-value work)
pub const MULTIPLIER_BUG_REPORT_BPS: u16 = 10000;

/// Referral: 60% (growth work, lower than technical)
pub const MULTIPLIER_REFERRAL_BPS: u16 = 6000;

/// Signup: 40% (lowest multiplier, one-time token grant)
pub const MULTIPLIER_SIGNUP_BPS: u16 = 4000;

// ============================================================================
// Emission Pool Separation — Sigmoid Decay Model
//
// growth_cap(t) = floor + (ceiling - floor) / (1 + e^(k × (t - midpoint)))
//
// The sigmoid encodes a smooth, ungameable transition from "build the network"
// to "sustain the network." At launch, growth allocation is near the ceiling.
// Over time, it decays smoothly toward the permanent floor. No discontinuities,
// no peak to target, no phase boundaries to game.
//
// The permanent floor means onboarding new users is always compensated — even
// at year 10. Growth is not a phase, it's part of the economic model.
// ============================================================================

/// Maximum growth pool share at launch (20% = 2000 bps)
pub const SIGMOID_GROWTH_CEILING_BPS: u16 = 2000;

/// Permanent minimum growth pool share (3% = 300 bps)
/// This floor ensures growth work is always incentivized
pub const SIGMOID_GROWTH_FLOOR_BPS: u16 = 300;

/// Midpoint of sigmoid transition in days from launch (540 days ≈ 18 months)
/// At this point, growth cap equals (ceiling + floor) / 2 ≈ 11.5%
pub const SIGMOID_MIDPOINT_DAYS: u64 = 540;

/// Steepness of sigmoid transition (scaled by 10000)
/// k=100 means k=0.01 — gives a smooth ~18-month transition window
/// Higher = sharper transition, lower = more gradual
pub const SIGMOID_K_SCALED: u64 = 100;

/// Total number of contribution types (8 technical + 3 growth + 1 discovery)
pub const CONTRIBUTION_TYPE_COUNT: u8 = 12;

// ============================================================================
// Grand Challenge: Discovery Contribution Type
//
// AMOS directs surplus capacity toward discovering fundamental physics for
// the benefit of all. The discovery multiplier is the highest in the system
// and rises over time via sigmoid, making fundamental research progressively
// more economically attractive.
//
// Constitutional protection (4th immutable provision):
// - Cannot be removed from registry
// - Floor cannot be reduced below 150%
// - Ceiling cannot be reduced below 300%
// - Exempt from registry freeze at sub-floor values
// ============================================================================

/// Discovery contribution type ID (index 11)
pub const CONTRIBUTION_TYPE_DISCOVERY: u8 = 11;

/// Discovery multiplier floor at launch (150% = 15000 bps)
/// This is the HIGHEST multiplier in the system from day one
pub const DISCOVERY_MULTIPLIER_FLOOR_BPS: u16 = 15000;

/// Discovery multiplier ceiling at maturity (300% = 30000 bps)
/// By year 10+, discovery pays triple the base rate
pub const DISCOVERY_MULTIPLIER_CEILING_BPS: u16 = 30000;

/// Discovery sigmoid midpoint in days (~5 years)
pub const DISCOVERY_SIGMOID_MIDPOINT_DAYS: u64 = 1825;

/// Discovery sigmoid steepness × 10000 (same as emission curve)
pub const DISCOVERY_SIGMOID_K_SCALED: u64 = 50;

/// Minimum trust level required for discovery bounties
pub const DISCOVERY_MIN_TRUST_LEVEL: u8 = 3;

/// Number of independent verifications required for discovery bounties
pub const DISCOVERY_VERIFICATION_COUNT: u8 = 2;

// ============================================================================
// Claim Timeout — Auto-releases abandoned bounties
// ============================================================================

/// Default maximum time to complete after claiming (hours)
pub const DEFAULT_CLAIM_TIMEOUT_HOURS: u64 = 72; // 3 days

/// Minimum allowed claim timeout (prevents unreasonably short windows)
pub const MIN_CLAIM_TIMEOUT_HOURS: u64 = 1;

/// Maximum allowed claim timeout (prevents indefinite locks)
pub const MAX_CLAIM_TIMEOUT_HOURS: u64 = 720; // 30 days

// ============================================================================
// Concurrent Claim Limits — Scales with trust level
// ============================================================================

/// Maximum active (uncompleted) claims per wallet, by trust level
pub const MAX_CONCURRENT_CLAIMS: [u8; 5] = [3, 5, 8, 12, 20];

// ============================================================================
// Dispute Mechanism — Contested rejections
// ============================================================================

/// Hours after rejection during which worker can file a dispute
pub const DISPUTE_WINDOW_HOURS: u64 = 48;

/// Stake required to file a dispute (BPS of bounty value)
pub const DISPUTE_STAKE_BPS: u16 = 500; // 5% of bounty value

/// Maximum time for dispute resolution (hours)
pub const DISPUTE_RESOLUTION_TIMEOUT_HOURS: u64 = 168; // 7 days

// ============================================================================
// ContributionTypeRegistry — Graduated freeze mechanism
// ============================================================================

/// Maximum contribution types in the registry
pub const MAX_CONTRIBUTION_TYPES: u8 = 16;

/// Auto-freeze deadline: 3 years from launch (seconds)
pub const REGISTRY_AUTO_FREEZE_SECONDS: i64 = 94_608_000; // 3 years

/// Maximum number of governance-voted extensions
pub const REGISTRY_MAX_EXTENSIONS: u8 = 2;

/// Each extension is exactly 1 year (seconds)
pub const REGISTRY_EXTENSION_DURATION_SECONDS: i64 = 31_536_000; // 1 year

// ============================================================================
// Anti-Gaming Measures
// ============================================================================

/// Reputation penalty for false submissions (BPS)
pub const FALSE_SUBMISSION_PENALTY_BPS: u16 = 500; // 5%

/// Self-dealing cooldown: poster cannot claim own bounty for this many hours
pub const SELF_DEALING_COOLDOWN_HOURS: u64 = 24;

/// Verification contribution type multiplier (same as testing_qa)
pub const MULTIPLIER_VERIFICATION_BPS: u16 = 11000; // 110%

/// Trust level required to be a verifier/reviewer
pub const VERIFICATION_MIN_TRUST_LEVEL: u8 = 3;

// ============================================================================
// Reviewer Rewards
// ============================================================================

/// Portion of bounty tokens awarded to the reviewer (5% = 500 bps)
/// This incentivizes quality validation work
pub const REVIEWER_REWARD_BPS: u16 = 500;

// ============================================================================
// General Constants
// ============================================================================

/// Basis points denominator (100% = 10000 bps)
pub const BPS_DENOMINATOR: u16 = 10000;

// ============================================================================
// PDA Seeds
// ============================================================================

/// Seed for the main bounty config account
pub const BOUNTY_CONFIG_SEED: &[u8] = b"bounty_config";

/// Seed prefix for daily pool accounts
pub const DAILY_POOL_SEED: &[u8] = b"daily_pool";

/// Seed prefix for bounty proof accounts
pub const BOUNTY_PROOF_SEED: &[u8] = b"bounty_proof";

/// Seed prefix for operator stats accounts
pub const OPERATOR_STATS_SEED: &[u8] = b"operator_stats";

/// Seed prefix for agent trust record accounts
pub const AGENT_TRUST_SEED: &[u8] = b"agent_trust";

/// Seed for platform metrics singleton
pub const PLATFORM_METRICS_SEED: &[u8] = b"platform_metrics";

/// Seed for bounty escrow PDA
pub const BOUNTY_ESCROW_SEED: &[u8] = b"bounty_escrow";

/// Seed for bounty listing PDA
pub const BOUNTY_LISTING_SEED: &[u8] = b"bounty_listing";

/// Seed for dispute record PDA
pub const DISPUTE_SEED: &[u8] = b"dispute";

/// Seed for contribution type registry PDA
pub const CONTRIBUTION_REGISTRY_SEED: &[u8] = b"contribution_registry";

// ============================================================================
// Helper Functions
// ============================================================================

/// Compute the discovery contribution multiplier for a given elapsed day.
///
/// Uses an INVERTED sigmoid: starts at FLOOR (150%) and RISES to CEILING (300%)
/// over ~10 years. This makes discovery progressively more valuable over time.
///
/// Constitutional guarantee: always returns >= DISCOVERY_MULTIPLIER_FLOOR_BPS.
pub fn discovery_multiplier_bps(elapsed_days: u64) -> u16 {
    let descending = amos_protocol_math::sigmoid(
        elapsed_days,
        DISCOVERY_MULTIPLIER_CEILING_BPS as u64,
        DISCOVERY_MULTIPLIER_FLOOR_BPS as u64,
        DISCOVERY_SIGMOID_MIDPOINT_DAYS,
        DISCOVERY_SIGMOID_K_SCALED,
    );
    (DISCOVERY_MULTIPLIER_CEILING_BPS as u64 + DISCOVERY_MULTIPLIER_FLOOR_BPS as u64 - descending)
        as u16
}

/// Get the contribution type multiplier in basis points.
/// For discovery (type 11), requires elapsed_days for dynamic sigmoid.
/// Use get_contribution_multiplier_static for non-discovery types.
pub fn get_contribution_multiplier(contribution_type: u8) -> Result<u16> {
    match contribution_type {
        // Technical pool (0-7)
        0 => Ok(MULTIPLIER_BUG_FIX_BPS),
        1 => Ok(MULTIPLIER_FEATURE_BPS),
        2 => Ok(MULTIPLIER_DOCUMENTATION_BPS),
        3 => Ok(MULTIPLIER_CONTENT_BPS),
        4 => Ok(MULTIPLIER_SUPPORT_BPS),
        5 => Ok(MULTIPLIER_TESTING_BPS),
        6 => Ok(MULTIPLIER_DESIGN_BPS),
        7 => Ok(MULTIPLIER_INFRASTRUCTURE_BPS),
        // Growth pool (8-10)
        8 => Ok(MULTIPLIER_BUG_REPORT_BPS),
        9 => Ok(MULTIPLIER_REFERRAL_BPS),
        10 => Ok(MULTIPLIER_SIGNUP_BPS),
        // Discovery (11) — returns floor; use get_contribution_multiplier_dynamic for sigmoid
        11 => Ok(DISCOVERY_MULTIPLIER_FLOOR_BPS),
        _ => Err(error!(crate::errors::BountyError::InvalidContributionType)),
    }
}

/// Get contribution multiplier with time-dependent discovery sigmoid.
/// This is the preferred function for on-chain distribution calculations.
pub fn get_contribution_multiplier_dynamic(
    contribution_type: u8,
    elapsed_days: u64,
) -> Result<u16> {
    match contribution_type {
        11 => Ok(discovery_multiplier_bps(elapsed_days)),
        _ => get_contribution_multiplier(contribution_type),
    }
}

/// Returns true if the contribution type belongs to the growth pool.
/// Growth types: bug_report (8), referral (9), signup (10)
/// All others are technical pool.
pub fn is_growth_contribution(contribution_type: u8) -> bool {
    contribution_type >= 8 && contribution_type <= 10
}

// ============================================================================
// Sigmoid Growth Cap Computation
// ============================================================================

/// Compute the growth pool cap in BPS using sigmoid decay.
///
/// Formula: growth_cap(t) = floor + (ceiling - floor) / (1 + e^(k × (t - midpoint)))
///
/// At t=0 (launch): cap ≈ ceiling (20%)
/// At t=midpoint: cap = (ceiling + floor) / 2 ≈ 11.5%
/// At t→∞: cap → floor (3%)
///
/// All integer math, no floating point. Uses the shared fixed-point kernel.
pub fn sigmoid_growth_cap_bps(elapsed_days: u64) -> u16 {
    sigmoid_growth_cap_bps_params(
        elapsed_days,
        SIGMOID_GROWTH_CEILING_BPS,
        SIGMOID_GROWTH_FLOOR_BPS,
        SIGMOID_MIDPOINT_DAYS,
        SIGMOID_K_SCALED,
    )
}

/// Parameterized version for use with registry-stored parameters.
pub fn sigmoid_growth_cap_bps_params(
    elapsed_days: u64,
    ceiling_bps: u16,
    floor_bps: u16,
    midpoint_days: u64,
    k_scaled: u64,
) -> u16 {
    amos_protocol_math::sigmoid(
        elapsed_days,
        ceiling_bps as u64,
        floor_bps as u64,
        midpoint_days,
        k_scaled,
    ) as u16
}

// ============================================================================
// Sigmoid Emission Computation
// ============================================================================

/// Compute daily emission for a given elapsed day using sigmoid decay.
///
/// Formula: emission(t) = floor + (ceiling - floor) / (1 + e^(k × (t - midpoint)))
///
/// Uses the same fixed-point kernel as sigmoid_growth_cap_bps.
/// Returns raw token units per day (9 decimals).
pub fn sigmoid_daily_emission(elapsed_days: u64) -> u64 {
    sigmoid_daily_emission_params(
        elapsed_days,
        EMISSION_CEILING,
        EMISSION_FLOOR,
        EMISSION_MIDPOINT_DAYS,
        EMISSION_K_SCALED,
    )
}

/// Parameterized version for registry-stored or configurable parameters.
pub fn sigmoid_daily_emission_params(
    elapsed_days: u64,
    ceiling: u64,
    floor: u64,
    midpoint_days: u64,
    k_scaled: u64,
) -> u64 {
    amos_protocol_math::sigmoid(elapsed_days, ceiling, floor, midpoint_days, k_scaled)
}

/// Get the maximum concurrent claims for a given trust level
pub fn get_max_concurrent_claims(trust_level: u8) -> Result<u8> {
    if trust_level == 0 || trust_level > 5 {
        return Err(error!(crate::errors::BountyError::InvalidTrustLevel));
    }
    Ok(MAX_CONCURRENT_CLAIMS[(trust_level - 1) as usize])
}

/// Get the maximum points allowed for a given trust level
pub fn get_max_points_for_trust_level(trust_level: u8) -> Result<u16> {
    if trust_level == 0 || trust_level > 5 {
        return Err(error!(crate::errors::BountyError::InvalidTrustLevel));
    }
    Ok(TRUST_LEVEL_MAX_POINTS[(trust_level - 1) as usize])
}

/// Get the daily bounty limit for a given trust level
pub fn get_daily_limit_for_trust_level(trust_level: u8) -> Result<u16> {
    if trust_level == 0 || trust_level > 5 {
        return Err(error!(crate::errors::BountyError::InvalidTrustLevel));
    }
    Ok(TRUST_LEVEL_DAILY_LIMITS[(trust_level - 1) as usize])
}

/// Check if an operator is eligible for a trust level upgrade
pub fn can_upgrade_to_level(current_level: u8, completions: u32, reputation: u32) -> Result<bool> {
    match current_level {
        1 => Ok(completions >= TRUST_LEVEL_2_MIN_COMPLETIONS
            && reputation >= TRUST_LEVEL_2_MIN_REPUTATION),
        2 => Ok(completions >= TRUST_LEVEL_3_MIN_COMPLETIONS
            && reputation >= TRUST_LEVEL_3_MIN_REPUTATION),
        3 => Ok(completions >= TRUST_LEVEL_4_MIN_COMPLETIONS
            && reputation >= TRUST_LEVEL_4_MIN_REPUTATION),
        4 => Ok(completions >= TRUST_LEVEL_5_MIN_COMPLETIONS
            && reputation >= TRUST_LEVEL_5_MIN_REPUTATION),
        5 => Ok(false),
        _ => Err(error!(crate::errors::BountyError::InvalidTrustLevel)),
    }
}

/// Get tenure-based decay floor in basis points based on years on network
pub fn get_tenure_floor_bps(years_on_network: u64) -> u16 {
    match years_on_network {
        0 => TENURE_FLOOR_YEAR_0_BPS,
        1 => TENURE_FLOOR_YEAR_1_BPS,
        2..=4 => TENURE_FLOOR_YEAR_2_BPS,
        _ => TENURE_FLOOR_YEAR_5_BPS,
    }
}

/// Get tenure-based decay reduction in basis points based on years on network
pub fn get_tenure_reduction_bps(years_on_network: u64) -> u16 {
    match years_on_network {
        0 => TENURE_REDUCTION_YEAR_0_BPS,
        1 => TENURE_REDUCTION_YEAR_1_BPS,
        2..=4 => TENURE_REDUCTION_YEAR_2_BPS,
        _ => TENURE_REDUCTION_YEAR_5_BPS,
    }
}

/// Get vault tier decay reduction in basis points
pub fn get_vault_reduction_bps(vault_tier: u8) -> u16 {
    match vault_tier {
        0 => 0,                             // No vault
        1 => VAULT_BRONZE_REDUCTION_BPS,    // Bronze
        2 => VAULT_SILVER_REDUCTION_BPS,    // Silver
        3 => VAULT_GOLD_REDUCTION_BPS,      // Gold
        4 => VAULT_PERMANENT_REDUCTION_BPS, // Permanent
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_and_minimums_are_nine_decimal_raw_units() {
        assert_eq!(TOTAL_SUPPLY, 100_000_000_000_000_000);
        assert_eq!(TREASURY_ALLOCATION, 95_000_000_000_000_000);
        assert_eq!(MIN_COMMERCIAL_ESCROW, 100_000_000_000);
        assert!(1_000 * ONE_TOKEN < TREASURY_ALLOCATION);
    }

    #[test]
    fn test_treasury_allocation() {
        assert_eq!(TREASURY_ALLOCATION, TOTAL_SUPPLY * 95 / 100);
    }

    #[test]
    fn fee_shares_sum_to_100_percent() {
        assert_eq!(
            FEE_HOLDER_SHARE_BPS + FEE_BURN_SHARE_BPS + FEE_LABS_SHARE_BPS,
            BPS_DENOMINATOR,
            "Fee shares must sum to 100%"
        );
    }

    #[test]
    fn test_decay_bounds() {
        assert!(MIN_DECAY_RATE_BPS < MAX_DECAY_RATE_BPS);
        assert!(DEFAULT_DECAY_RATE_BPS >= MIN_DECAY_RATE_BPS);
        assert!(DEFAULT_DECAY_RATE_BPS <= MAX_DECAY_RATE_BPS);
    }

    #[test]
    fn test_contribution_multipliers() {
        // All 12 types (8 technical + 3 growth + 1 discovery)
        for i in 0..=11 {
            let multiplier = get_contribution_multiplier(i).unwrap();
            assert!(multiplier > 0);
            assert!(multiplier <= 30000); // Discovery ceiling is 30000
        }
        // Invalid type
        assert!(get_contribution_multiplier(12).is_err());
    }

    #[test]
    fn test_discovery_multiplier_at_launch() {
        let mult = discovery_multiplier_bps(0);
        assert!(
            mult >= DISCOVERY_MULTIPLIER_FLOOR_BPS,
            "Launch discovery multiplier {} below floor",
            mult
        );
        assert!(
            mult <= DISCOVERY_MULTIPLIER_FLOOR_BPS + 500,
            "Launch discovery multiplier {} too far above floor",
            mult
        );
    }

    #[test]
    fn test_discovery_multiplier_at_midpoint() {
        let mult = discovery_multiplier_bps(DISCOVERY_SIGMOID_MIDPOINT_DAYS);
        let expected_mid = (DISCOVERY_MULTIPLIER_FLOOR_BPS + DISCOVERY_MULTIPLIER_CEILING_BPS) / 2;
        let tolerance = 500;
        assert!(
            (mult as i32 - expected_mid as i32).unsigned_abs() <= tolerance,
            "Midpoint discovery multiplier {} should be near {}",
            mult,
            expected_mid
        );
    }

    #[test]
    fn test_discovery_multiplier_at_maturity() {
        let mult = discovery_multiplier_bps(5000);
        assert!(
            mult >= DISCOVERY_MULTIPLIER_CEILING_BPS - 500,
            "Mature discovery multiplier {} should be near ceiling",
            mult
        );
    }

    #[test]
    fn test_discovery_multiplier_monotonically_increasing() {
        let mut prev = discovery_multiplier_bps(0);
        for day in (30..=5000).step_by(30) {
            let mult = discovery_multiplier_bps(day);
            assert!(
                mult >= prev,
                "Discovery multiplier decreased at day {}: {} < {}",
                day,
                mult,
                prev
            );
            prev = mult;
        }
    }

    #[test]
    fn test_discovery_multiplier_never_below_floor() {
        for day in [0, 100, 540, 1000, 1825, 3650, 10000] {
            let mult = discovery_multiplier_bps(day);
            assert!(
                mult >= DISCOVERY_MULTIPLIER_FLOOR_BPS,
                "Day {}: discovery multiplier {} below floor",
                day,
                mult
            );
        }
    }

    #[test]
    fn test_discovery_multiplier_never_above_ceiling() {
        for day in [0, 1825, 3650, 10000, 50000] {
            let mult = discovery_multiplier_bps(day);
            assert!(
                mult <= DISCOVERY_MULTIPLIER_CEILING_BPS,
                "Day {}: discovery multiplier {} above ceiling",
                day,
                mult
            );
        }
    }

    #[test]
    fn test_discovery_is_always_highest_multiplier() {
        // At any point in time, discovery must be >= all other multipliers
        for day in [0, 365, 1825, 3650] {
            let discovery = get_contribution_multiplier_dynamic(11, day).unwrap();
            for i in 0..=10 {
                let other = get_contribution_multiplier(i).unwrap();
                assert!(
                    discovery >= other,
                    "Day {}: discovery {} < type {} ({})",
                    day,
                    discovery,
                    i,
                    other
                );
            }
        }
    }

    #[test]
    fn test_discovery_dynamic_vs_static() {
        // Static returns floor
        let static_mult = get_contribution_multiplier(11).unwrap();
        assert_eq!(static_mult, DISCOVERY_MULTIPLIER_FLOOR_BPS);

        // Dynamic returns sigmoid value
        let dynamic_mult = get_contribution_multiplier_dynamic(11, 3650).unwrap();
        assert!(dynamic_mult > DISCOVERY_MULTIPLIER_FLOOR_BPS);
    }

    #[test]
    fn test_growth_contribution_identification() {
        // Technical pool (0-7)
        for i in 0..=7 {
            assert!(!is_growth_contribution(i));
        }
        // Growth pool (8-10)
        assert!(is_growth_contribution(8));
        assert!(is_growth_contribution(9));
        assert!(is_growth_contribution(10));
        // Out of range
        assert!(!is_growth_contribution(11));
    }

    #[test]
    fn test_sigmoid_growth_cap_at_launch() {
        // At day 0, cap should be near ceiling (20%)
        let cap = sigmoid_growth_cap_bps(0);
        assert!(cap >= 1950, "Launch cap {} should be near ceiling", cap);
        assert!(cap <= SIGMOID_GROWTH_CEILING_BPS);
    }

    #[test]
    fn test_sigmoid_growth_cap_at_midpoint() {
        // At midpoint (day 540), cap should be ~(ceiling + floor) / 2 ≈ 1150
        let cap = sigmoid_growth_cap_bps(SIGMOID_MIDPOINT_DAYS);
        let expected_mid = (SIGMOID_GROWTH_CEILING_BPS + SIGMOID_GROWTH_FLOOR_BPS) / 2;
        let tolerance = 100; // Within 1%
        assert!(
            (cap as i32 - expected_mid as i32).unsigned_abs() <= tolerance,
            "Midpoint cap {} should be near {}",
            cap,
            expected_mid
        );
    }

    #[test]
    fn test_sigmoid_growth_cap_at_maturity() {
        // At day 1800 (5 years), cap should be near floor (3%)
        let cap = sigmoid_growth_cap_bps(1800);
        assert!(
            cap <= SIGMOID_GROWTH_FLOOR_BPS + 50,
            "Mature cap {} should be near floor",
            cap
        );
        assert!(cap >= SIGMOID_GROWTH_FLOOR_BPS);
    }

    #[test]
    fn test_sigmoid_is_monotonically_decreasing() {
        let mut prev = sigmoid_growth_cap_bps(0);
        for day in (30..=1800).step_by(30) {
            let cap = sigmoid_growth_cap_bps(day);
            assert!(
                cap <= prev,
                "Cap at day {} ({}) > cap at day {} ({})",
                day,
                cap,
                day - 30,
                prev
            );
            prev = cap;
        }
    }

    #[test]
    fn test_sigmoid_never_below_floor() {
        for day in [0, 100, 540, 1000, 3650, 10000] {
            let cap = sigmoid_growth_cap_bps(day);
            assert!(
                cap >= SIGMOID_GROWTH_FLOOR_BPS,
                "Day {}: cap {} below floor",
                day,
                cap
            );
        }
    }

    #[test]
    fn test_sigmoid_never_above_ceiling() {
        for day in [0, 1, 10, 100] {
            let cap = sigmoid_growth_cap_bps(day);
            assert!(
                cap <= SIGMOID_GROWTH_CEILING_BPS,
                "Day {}: cap {} above ceiling",
                day,
                cap
            );
        }
    }

    #[test]
    fn shared_curve_parity_and_exact_tails() {
        for day in [0, 365, 1460, 10_000, u64::MAX] {
            assert_eq!(
                sigmoid_daily_emission(day),
                amos_protocol_math::daily_emission(day)
            );
            assert_eq!(
                sigmoid_growth_cap_bps(day),
                amos_protocol_math::growth_cap_bps(day)
            );
        }
        assert_eq!(sigmoid_daily_emission(u64::MAX), EMISSION_FLOOR);
        assert_eq!(sigmoid_growth_cap_bps(u64::MAX), SIGMOID_GROWTH_FLOOR_BPS);
    }

    #[test]
    fn test_sigmoid_sample_values() {
        // Print a sample curve for visual verification
        println!("Day → Growth Cap BPS:");
        for &day in &[
            0, 90, 180, 270, 360, 450, 540, 630, 720, 810, 900, 1080, 1260, 1800,
        ] {
            println!(
                "  Day {:>5}: {:>4} bps ({:.1}%)",
                day,
                sigmoid_growth_cap_bps(day),
                sigmoid_growth_cap_bps(day) as f64 / 100.0
            );
        }
    }

    #[test]
    fn test_claim_timeout_bounds() {
        assert!(MIN_CLAIM_TIMEOUT_HOURS <= DEFAULT_CLAIM_TIMEOUT_HOURS);
        assert!(DEFAULT_CLAIM_TIMEOUT_HOURS <= MAX_CLAIM_TIMEOUT_HOURS);
    }

    #[test]
    fn test_concurrent_claim_limits_progressive() {
        for i in 0..4 {
            assert!(MAX_CONCURRENT_CLAIMS[i] < MAX_CONCURRENT_CLAIMS[i + 1]);
        }
    }

    #[test]
    fn test_dispute_constants_valid() {
        assert_eq!(DISPUTE_WINDOW_HOURS, 48);
        assert_eq!(DISPUTE_STAKE_BPS, 500);
        assert_eq!(DISPUTE_RESOLUTION_TIMEOUT_HOURS, 168);
    }

    #[test]
    fn test_registry_freeze_max_5_years() {
        let max_lifetime = REGISTRY_AUTO_FREEZE_SECONDS
            + (REGISTRY_MAX_EXTENSIONS as i64) * REGISTRY_EXTENSION_DURATION_SECONDS;
        // 3 years + 2 * 1 year = 5 years = 157,680,000 seconds
        assert_eq!(max_lifetime, 157_680_000);
    }

    #[test]
    fn test_trust_level_thresholds() {
        assert!(TRUST_LEVEL_2_MIN_COMPLETIONS < TRUST_LEVEL_3_MIN_COMPLETIONS);
        assert!(TRUST_LEVEL_3_MIN_COMPLETIONS < TRUST_LEVEL_4_MIN_COMPLETIONS);
        assert!(TRUST_LEVEL_4_MIN_COMPLETIONS < TRUST_LEVEL_5_MIN_COMPLETIONS);

        assert!(TRUST_LEVEL_2_MIN_REPUTATION < TRUST_LEVEL_3_MIN_REPUTATION);
        assert!(TRUST_LEVEL_3_MIN_REPUTATION < TRUST_LEVEL_4_MIN_REPUTATION);
        assert!(TRUST_LEVEL_4_MIN_REPUTATION < TRUST_LEVEL_5_MIN_REPUTATION);
    }

    #[test]
    fn test_trust_level_max_points() {
        for i in 0..4 {
            assert!(TRUST_LEVEL_MAX_POINTS[i] < TRUST_LEVEL_MAX_POINTS[i + 1]);
        }
        assert_eq!(TRUST_LEVEL_MAX_POINTS[4], MAX_BOUNTY_POINTS);
    }

    #[test]
    fn test_trust_level_daily_limits() {
        for i in 0..4 {
            assert!(TRUST_LEVEL_DAILY_LIMITS[i] < TRUST_LEVEL_DAILY_LIMITS[i + 1]);
        }
    }

    #[test]
    fn test_reviewer_reward_is_reasonable() {
        assert!(REVIEWER_REWARD_BPS >= 100);
        assert!(REVIEWER_REWARD_BPS <= 1000);
    }

    #[test]
    fn test_decay_floor_is_valid() {
        assert!(DECAY_FLOOR_BPS >= 500);
        assert!(DECAY_FLOOR_BPS <= 5000);
    }

    #[test]
    fn test_sigmoid_emission_at_launch() {
        let emission = sigmoid_daily_emission(0);
        assert!(
            emission >= 15_800 * ONE_TOKEN,
            "Launch emission too low: {}",
            emission
        );
        assert!(
            emission <= EMISSION_CEILING,
            "Launch emission above ceiling: {}",
            emission
        );
    }

    #[test]
    fn test_sigmoid_emission_at_midpoint() {
        let emission = sigmoid_daily_emission(EMISSION_MIDPOINT_DAYS);
        let expected_mid = (EMISSION_CEILING + EMISSION_FLOOR) / 2; // ~8,050
        let tolerance = expected_mid / 20; // 5%
        assert!(
            emission >= expected_mid - tolerance,
            "Midpoint emission too low: {}",
            emission
        );
        assert!(
            emission <= expected_mid + tolerance,
            "Midpoint emission too high: {}",
            emission
        );
    }

    #[test]
    fn test_sigmoid_emission_at_maturity() {
        let emission = sigmoid_daily_emission(5000); // ~13.7 years
        assert!(emission >= EMISSION_FLOOR, "Below floor: {}", emission);
        assert!(
            emission <= EMISSION_FLOOR + 200 * ONE_TOKEN,
            "Too far above floor at maturity: {}",
            emission
        );
    }

    #[test]
    fn test_sigmoid_emission_monotonically_decreasing() {
        let mut prev = sigmoid_daily_emission(0);
        for day in (1..5000).step_by(10) {
            let current = sigmoid_daily_emission(day);
            assert!(
                current <= prev,
                "Emission increased at day {}: {} > {}",
                day,
                current,
                prev
            );
            prev = current;
        }
    }

    #[test]
    fn test_sigmoid_emission_never_below_floor() {
        for day in (0..10000).step_by(100) {
            let emission = sigmoid_daily_emission(day);
            assert!(
                emission >= EMISSION_FLOOR,
                "Below floor at day {}: {}",
                day,
                emission
            );
        }
    }

    #[test]
    fn test_sigmoid_emission_never_above_ceiling() {
        for day in 0..10000 {
            let emission = sigmoid_daily_emission(day);
            assert!(
                emission <= EMISSION_CEILING,
                "Above ceiling at day {}: {}",
                day,
                emission
            );
        }
    }

    #[test]
    fn test_sigmoid_emission_sample_trajectory() {
        let year1 = sigmoid_daily_emission(365);
        let year2 = sigmoid_daily_emission(730);
        let year4 = sigmoid_daily_emission(1460);
        let year6 = sigmoid_daily_emission(2190);
        let year8 = sigmoid_daily_emission(2920);
        let year10 = sigmoid_daily_emission(3650);

        assert!(year1 > 13_000 * ONE_TOKEN, "Year 1 too low: {}", year1);
        assert!(year2 > 10_000 * ONE_TOKEN, "Year 2 too low: {}", year2);
        assert!(
            year4 > 7_000 * ONE_TOKEN && year4 < 9_000 * ONE_TOKEN,
            "Year 4 unexpected: {}",
            year4
        );
        assert!(
            year6 > 200 * ONE_TOKEN && year6 < 2_000 * ONE_TOKEN,
            "Year 6 unexpected: {}",
            year6
        );
        assert!(
            year8 > 100 * ONE_TOKEN && year8 < 500 * ONE_TOKEN,
            "Year 8 unexpected: {}",
            year8
        );
        assert!(
            year10 >= EMISSION_FLOOR && year10 < 200 * ONE_TOKEN,
            "Year 10 unexpected: {}",
            year10
        );
    }

    #[test]
    fn test_sigmoid_emission_governance_bounds() {
        assert!(EMISSION_K_MIN < EMISSION_K_SCALED);
        assert!(EMISSION_K_SCALED < EMISSION_K_MAX);
        assert!(EMISSION_MIDPOINT_MIN_DAYS < EMISSION_MIDPOINT_DAYS);
        assert!(EMISSION_MIDPOINT_DAYS < EMISSION_MIDPOINT_MAX_DAYS);
    }

    #[test]
    fn test_upgrade_eligibility() {
        assert!(can_upgrade_to_level(1, 3, 5500).unwrap());
        assert!(!can_upgrade_to_level(1, 2, 5500).unwrap());
        assert!(!can_upgrade_to_level(1, 3, 5499).unwrap());
        assert!(can_upgrade_to_level(4, 50, 8500).unwrap());
        assert!(!can_upgrade_to_level(4, 49, 8500).unwrap());
        assert!(!can_upgrade_to_level(5, 1000, 10000).unwrap());
    }

    #[test]
    fn tenure_floors_are_progressive() {
        assert!(TENURE_FLOOR_YEAR_0_BPS < TENURE_FLOOR_YEAR_1_BPS);
        assert!(TENURE_FLOOR_YEAR_1_BPS < TENURE_FLOOR_YEAR_2_BPS);
        assert!(TENURE_FLOOR_YEAR_2_BPS < TENURE_FLOOR_YEAR_5_BPS);
    }

    #[test]
    fn tenure_reductions_are_progressive() {
        assert!(TENURE_REDUCTION_YEAR_0_BPS < TENURE_REDUCTION_YEAR_1_BPS);
        assert!(TENURE_REDUCTION_YEAR_1_BPS < TENURE_REDUCTION_YEAR_2_BPS);
        assert!(TENURE_REDUCTION_YEAR_2_BPS < TENURE_REDUCTION_YEAR_5_BPS);
    }

    #[test]
    fn vault_reductions_are_ordered() {
        assert!(VAULT_BRONZE_REDUCTION_BPS < VAULT_SILVER_REDUCTION_BPS);
        assert!(VAULT_SILVER_REDUCTION_BPS < VAULT_GOLD_REDUCTION_BPS);
        assert!(VAULT_GOLD_REDUCTION_BPS < VAULT_PERMANENT_REDUCTION_BPS);
        assert!(VAULT_PERMANENT_REDUCTION_BPS < BPS_DENOMINATOR);
    }

    #[test]
    fn vault_lockups_are_ordered() {
        assert!(VAULT_BRONZE_LOCKUP_DAYS < VAULT_SILVER_LOCKUP_DAYS);
        assert!(VAULT_SILVER_LOCKUP_DAYS < VAULT_GOLD_LOCKUP_DAYS);
    }

    #[test]
    fn both_grace_periods_are_distinct() {
        assert_ne!(INACTIVITY_GRACE_PERIOD_DAYS, NEW_STAKE_GRACE_PERIOD_DAYS);
        assert_eq!(INACTIVITY_GRACE_PERIOD_DAYS, 90);
        assert_eq!(NEW_STAKE_GRACE_PERIOD_DAYS, 365);
    }
}
