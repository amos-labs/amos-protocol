/// AMOS Bounty Program Constants
///
/// This module defines all the constants that govern the trustless token distribution
/// system. These values are hardcoded on-chain and ensure predictable, transparent
/// operation without any centralized control beyond the oracle's role in validation.

use anchor_lang::prelude::*;

// ============================================================================
// Token Economics Constants
// ============================================================================

/// Total supply of AMOS tokens (100 million)
pub const TOTAL_SUPPLY: u64 = 100_000_000;

/// Bounty Treasury allocation (95% of total supply = 95 million tokens)
/// This is the pool from which bounties are distributed
pub const TREASURY_ALLOCATION: u64 = 95_000_000;

/// Initial daily emission rate (16,000 tokens per day)
/// This represents the starting rate before any halvings occur
pub const INITIAL_DAILY_EMISSION: u64 = 16_000;

/// Number of days between halving events (365 days = 1 year)
pub const HALVING_INTERVAL_DAYS: u64 = 365;

/// Minimum daily emission floor (100 tokens)
/// Emissions will never go below this amount, ensuring ongoing rewards
pub const MINIMUM_DAILY_EMISSION: u64 = 100;

/// Maximum number of halving epochs (10 halvings)
/// After this, emission stays at minimum
pub const MAX_HALVING_EPOCHS: u8 = 10;

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

/// Minimum escrow amount for commercial bounties (100 tokens, assuming 6 decimals)
/// Prevents dust bounties that waste compute and bloat account space
pub const MIN_COMMERCIAL_ESCROW: u64 = 100_000_000;

/// Maximum number of bounties an operator can submit per day
/// Prevents spam and ensures fair distribution
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

// ============================================================================
// Helper Functions
// ============================================================================

/// Get the contribution type multiplier in basis points
pub fn get_contribution_multiplier(contribution_type: u8) -> Result<u16> {
    match contribution_type {
        0 => Ok(MULTIPLIER_BUG_FIX_BPS),
        1 => Ok(MULTIPLIER_FEATURE_BPS),
        2 => Ok(MULTIPLIER_DOCUMENTATION_BPS),
        3 => Ok(MULTIPLIER_CONTENT_BPS),
        4 => Ok(MULTIPLIER_SUPPORT_BPS),
        5 => Ok(MULTIPLIER_TESTING_BPS),
        6 => Ok(MULTIPLIER_DESIGN_BPS),
        7 => Ok(MULTIPLIER_INFRASTRUCTURE_BPS),
        _ => Err(error!(crate::errors::BountyError::InvalidContributionType)),
    }
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
pub fn can_upgrade_to_level(
    current_level: u8,
    completions: u32,
    reputation: u32,
) -> Result<bool> {
    match current_level {
        1 => {
            Ok(completions >= TRUST_LEVEL_2_MIN_COMPLETIONS
                && reputation >= TRUST_LEVEL_2_MIN_REPUTATION)
        }
        2 => {
            Ok(completions >= TRUST_LEVEL_3_MIN_COMPLETIONS
                && reputation >= TRUST_LEVEL_3_MIN_REPUTATION)
        }
        3 => {
            Ok(completions >= TRUST_LEVEL_4_MIN_COMPLETIONS
                && reputation >= TRUST_LEVEL_4_MIN_REPUTATION)
        }
        4 => {
            Ok(completions >= TRUST_LEVEL_5_MIN_COMPLETIONS
                && reputation >= TRUST_LEVEL_5_MIN_REPUTATION)
        }
        5 => {
            Ok(false)
        }
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
        0 => 0,                              // No vault
        1 => VAULT_BRONZE_REDUCTION_BPS,     // Bronze
        2 => VAULT_SILVER_REDUCTION_BPS,     // Silver
        3 => VAULT_GOLD_REDUCTION_BPS,       // Gold
        4 => VAULT_PERMANENT_REDUCTION_BPS,  // Permanent
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for i in 0..8 {
            let multiplier = get_contribution_multiplier(i).unwrap();
            assert!(multiplier > 0);
            assert!(multiplier <= 15000);
        }
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
    fn test_halving_schedule_sensibility() {
        let mut emission = INITIAL_DAILY_EMISSION;
        for _ in 0..MAX_HALVING_EPOCHS {
            emission /= 2;
            if emission < MINIMUM_DAILY_EMISSION {
                emission = MINIMUM_DAILY_EMISSION;
            }
        }
        assert!(emission <= MINIMUM_DAILY_EMISSION);
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
