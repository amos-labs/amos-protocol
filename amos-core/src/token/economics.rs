//! # AMOS Token Economics — Constants & Core Types
//!
//! Every number here is sourced from:
//!   - `docs/whitepaper_technical.md`
//!   - `docs/token_economy_equations.md`
//!   - `solana/programs/amos_treasury/src/constants.rs`
//!   - `solana/programs/amos_bounty/src/constants.rs`
//!
//! CRITICAL: These constants must remain byte-identical to the on-chain
//! program constants. Changing them here without redeploying the Solana
//! programs will cause divergence.

// ═══════════════════════════════════════════════════════════════════════════
// BASIS POINTS MATH
// ═══════════════════════════════════════════════════════════════════════════

/// 100% expressed in basis points (1 bps = 0.01%).
pub const BPS_DENOMINATOR: u64 = 10_000;

// ═══════════════════════════════════════════════════════════════════════════
// TOKEN SUPPLY
// ═══════════════════════════════════════════════════════════════════════════

/// Fixed total supply — mint authority permanently disabled.
pub const TOTAL_SUPPLY: u64 = 100_000_000;

/// Allocation: 95% Bounty Treasury (contributor rewards via daily emissions).
pub const TREASURY_ALLOCATION: u64 = 95_000_000;

/// Allocation: 5% Emergency Reserve (DAO-locked, governance vote required).
pub const RESERVE_ALLOCATION: u64 = 5_000_000;

// ═══════════════════════════════════════════════════════════════════════════
// PROTOCOL FEE — AMOS-ONLY (Applied to commercial bounties)
// All transactions denominated in AMOS tokens. No USDC track.
// ═══════════════════════════════════════════════════════════════════════════

/// Protocol fee rate: 3% of commercial bounty payout.
pub const PROTOCOL_FEE_BPS: u64 = 300;

/// 50% of fee → staked token holders (claimable proportionally).
pub const FEE_HOLDER_SHARE_BPS: u64 = 5_000;

/// 40% of fee → permanently burned (deflationary).
pub const FEE_BURN_SHARE_BPS: u64 = 4_000;

/// 10% of fee → AMOS Labs operating wallet (in AMOS tokens).
pub const FEE_LABS_SHARE_BPS: u64 = 1_000;

// ═══════════════════════════════════════════════════════════════════════════
// DECAY PARAMETERS
// ═══════════════════════════════════════════════════════════════════════════

/// Base annual decay rate (10%).
/// Formula: Decay = 10% - (Profit_Ratio * 5%), clamped to [MIN, MAX].
pub const BASE_DECAY_RATE_BPS: u64 = 1_000;

/// Minimum annual decay rate (2%) — during healthy profitability.
pub const MIN_DECAY_RATE_BPS: u64 = 200;

/// Maximum annual decay rate (25%) — during sustained losses.
pub const MAX_DECAY_RATE_BPS: u64 = 2_500;

/// Default annual decay rate before platform economics kick in (5%).
pub const DEFAULT_DECAY_RATE_BPS: u64 = 500;

/// Profit ratio multiplier for decay formula (5% = 500 bps).
pub const DECAY_PROFIT_MULTIPLIER_BPS: u64 = 500;

/// Inactivity grace: days without bounty completion before decay triggers.
pub const INACTIVITY_GRACE_PERIOD_DAYS: u64 = 90;

/// New stake grace: days after earning tokens during which they don't decay.
pub const NEW_STAKE_GRACE_PERIOD_DAYS: u64 = 365;

/// Decay floor: minimum 10% of original stake always preserved.
pub const DECAY_FLOOR_BPS: u64 = 1_000;

/// Portion of decayed tokens burned (10%), rest recycled to treasury.
pub const DECAY_BURN_PORTION_BPS: u64 = 1_000;

// ── Tenure-based decay floor progression ────────────────────────────
// Over time, long-term holders get an increasing permanent floor.

/// Year 0-1: 5% permanent floor.
pub const TENURE_FLOOR_YEAR_0_BPS: u64 = 500;
/// Year 1-2: 10% permanent floor.
pub const TENURE_FLOOR_YEAR_1_BPS: u64 = 1_000;
/// Year 2-5: 15% permanent floor.
pub const TENURE_FLOOR_YEAR_2_BPS: u64 = 1_500;
/// Year 5+: 25% permanent floor.
pub const TENURE_FLOOR_YEAR_5_BPS: u64 = 2_500;

// ── Tenure-based decay reduction ────────────────────────────────────
// Long-term holders get a percentage reduction in their decay rate.

/// Year 0-1: 0% reduction (full decay).
pub const TENURE_REDUCTION_YEAR_0_BPS: u64 = 0;
/// Year 1-2: 20% reduction.
pub const TENURE_REDUCTION_YEAR_1_BPS: u64 = 2_000;
/// Year 2-5: 40% reduction.
pub const TENURE_REDUCTION_YEAR_2_BPS: u64 = 4_000;
/// Year 5+: 70% reduction.
pub const TENURE_REDUCTION_YEAR_5_BPS: u64 = 7_000;

// ── Staking vault tiers (lockup bonuses) ────────────────────────────

/// Bronze vault (30-day lock): 20% decay reduction.
pub const VAULT_BRONZE_REDUCTION_BPS: u64 = 2_000;
/// Silver vault (90-day lock): 50% decay reduction.
pub const VAULT_SILVER_REDUCTION_BPS: u64 = 5_000;
/// Gold vault (365-day lock): 80% decay reduction.
pub const VAULT_GOLD_REDUCTION_BPS: u64 = 8_000;
/// Permanent vault (no unlock): 95% decay reduction.
pub const VAULT_PERMANENT_REDUCTION_BPS: u64 = 9_500;

// ═══════════════════════════════════════════════════════════════════════════
// EMISSION / HALVING
// ═══════════════════════════════════════════════════════════════════════════

/// Initial daily emission from treasury: 16,000 AMOS/day.
pub const INITIAL_DAILY_EMISSION: u64 = 16_000;

/// Halving interval: every 365 days (annual).
pub const HALVING_INTERVAL_DAYS: u64 = 365;

/// Minimum daily emission floor: 100 AMOS/day.
pub const MINIMUM_DAILY_EMISSION: u64 = 100;

/// Maximum halving epochs (prevents underflow).
pub const MAX_HALVING_EPOCHS: u64 = 10;

// ═══════════════════════════════════════════════════════════════════════════
// STAKING REQUIREMENTS
// ═══════════════════════════════════════════════════════════════════════════

/// Minimum stake to be eligible for revenue share: 100 AMOS.
pub const MIN_STAKE_AMOUNT: u64 = 100;

/// Minimum days staked before revenue eligibility: 30 days.
pub const MIN_STAKE_DAYS: u64 = 30;

// ═══════════════════════════════════════════════════════════════════════════
// BOUNTY SYSTEM
// ═══════════════════════════════════════════════════════════════════════════

/// Minimum quality score (0-100) for bounty approval.
pub const MIN_QUALITY_SCORE: u8 = 30;

/// Maximum points per single bounty.
pub const MAX_BOUNTY_POINTS: u64 = 2_000;

/// Maximum bounties per operator per day (on-chain enforcement).
pub const MAX_DAILY_BOUNTIES_PER_OPERATOR: u64 = 50;

/// Reviewer reward: 5% of bounty tokens go to the human reviewer.
pub const REVIEWER_REWARD_BPS: u64 = 500;

// ── Contribution type multipliers ───────────────────────────────────

/// Bug fix: 120% (bonus for fixing).
pub const MULTIPLIER_BUG_FIX_BPS: u64 = 12_000;
/// Feature development: 100% (baseline).
pub const MULTIPLIER_FEATURE_BPS: u64 = 10_000;
/// Documentation: 80%.
pub const MULTIPLIER_DOCS_BPS: u64 = 8_000;
/// Content/Marketing: 90%.
pub const MULTIPLIER_CONTENT_BPS: u64 = 9_000;
/// Support: 70%.
pub const MULTIPLIER_SUPPORT_BPS: u64 = 7_000;
/// Testing/QA: 110% (bonus for quality).
pub const MULTIPLIER_TESTING_BPS: u64 = 11_000;
/// Design: 100%.
pub const MULTIPLIER_DESIGN_BPS: u64 = 10_000;
/// Infrastructure: 130% (highest — core platform).
pub const MULTIPLIER_INFRA_BPS: u64 = 13_000;

// ── Growth bounty multipliers (Growth Pool) ────────────────────────────

/// Bug report: 100% (finding real bugs is high-value work).
pub const MULTIPLIER_BUG_REPORT_BPS: u64 = 10_000;
/// Referral: 60% (growth work, lower than technical).
pub const MULTIPLIER_REFERRAL_BPS: u64 = 6_000;
/// Signup: 40% (lowest multiplier, one-time token grant).
pub const MULTIPLIER_SIGNUP_BPS: u64 = 4_000;

// ═══════════════════════════════════════════════════════════════════════════
// EMISSION POOL SEPARATION — Sigmoid Decay Model
//
// growth_cap(t) = floor + (ceiling - floor) / (1 + e^(k × (t - midpoint)))
//
// Smooth, monotonic transition from growth-focus to infrastructure-focus.
// No discontinuities, no peak to game. Permanent floor ensures growth
// work is always compensated, even at network maturity.
// ═══════════════════════════════════════════════════════════════════════════

/// Maximum growth pool share at launch (20%).
pub const SIGMOID_GROWTH_CEILING_BPS: u64 = 2_000;
/// Permanent minimum growth pool share (3%).
pub const SIGMOID_GROWTH_FLOOR_BPS: u64 = 300;
/// Midpoint of transition in days (540 ≈ 18 months).
pub const SIGMOID_MIDPOINT_DAYS: u64 = 540;
/// Steepness parameter × 10000 (100 = k of 0.01).
pub const SIGMOID_K_SCALED: u64 = 100;

// ═══════════════════════════════════════════════════════════════════════════
// CLAIM TIMEOUT
// ═══════════════════════════════════════════════════════════════════════════

/// Default claim timeout: 72 hours (3 days).
pub const DEFAULT_CLAIM_TIMEOUT_HOURS: u64 = 72;
/// Minimum claim timeout: 1 hour.
pub const MIN_CLAIM_TIMEOUT_HOURS: u64 = 1;
/// Maximum claim timeout: 720 hours (30 days).
pub const MAX_CLAIM_TIMEOUT_HOURS: u64 = 720;

// ═══════════════════════════════════════════════════════════════════════════
// CONCURRENT CLAIM LIMITS
// ═══════════════════════════════════════════════════════════════════════════

/// Max concurrent claims per trust level: [L1, L2, L3, L4, L5].
pub const MAX_CONCURRENT_CLAIMS: [u8; 5] = [3, 5, 8, 12, 20];

// ═══════════════════════════════════════════════════════════════════════════
// DISPUTE MECHANISM
// ═══════════════════════════════════════════════════════════════════════════

/// Dispute window: 48 hours after rejection.
pub const DISPUTE_WINDOW_HOURS: u64 = 48;
/// Dispute stake: 5% of bounty value.
pub const DISPUTE_STAKE_BPS: u64 = 500;
/// Dispute resolution timeout: 168 hours (7 days), defaults to upheld.
pub const DISPUTE_RESOLUTION_TIMEOUT_HOURS: u64 = 168;

// ═══════════════════════════════════════════════════════════════════════════
// CONTRIBUTION TYPE REGISTRY
// ═══════════════════════════════════════════════════════════════════════════

/// Maximum contribution types in registry.
pub const MAX_CONTRIBUTION_TYPES: u8 = 16;
/// Auto-freeze deadline: 3 years from launch.
pub const REGISTRY_AUTO_FREEZE_SECONDS: i64 = 94_608_000;
/// Max governance extensions: 2.
pub const REGISTRY_MAX_EXTENSIONS: u8 = 2;
/// Extension duration: exactly 1 year.
pub const REGISTRY_EXTENSION_DURATION_SECONDS: i64 = 31_536_000;

// ═══════════════════════════════════════════════════════════════════════════
// COMPILE-TIME VERIFICATION
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supply_allocations_sum_to_total() {
        assert_eq!(
            TREASURY_ALLOCATION + RESERVE_ALLOCATION,
            TOTAL_SUPPLY,
            "Allocations must sum to 100M"
        );
    }

    #[test]
    fn fee_shares_sum_to_100_percent() {
        assert_eq!(
            FEE_HOLDER_SHARE_BPS + FEE_BURN_SHARE_BPS + FEE_LABS_SHARE_BPS,
            BPS_DENOMINATOR,
            "Fee shares must sum to 10000 bps"
        );
    }

    #[test]
    fn decay_range_is_valid() {
        const {
            assert!(MIN_DECAY_RATE_BPS < MAX_DECAY_RATE_BPS);
        }
        const {
            assert!(DEFAULT_DECAY_RATE_BPS >= MIN_DECAY_RATE_BPS);
        }
        const {
            assert!(DEFAULT_DECAY_RATE_BPS <= MAX_DECAY_RATE_BPS);
        }
    }

    #[test]
    fn tenure_floors_are_progressive() {
        const {
            assert!(TENURE_FLOOR_YEAR_0_BPS < TENURE_FLOOR_YEAR_1_BPS);
        }
        const {
            assert!(TENURE_FLOOR_YEAR_1_BPS < TENURE_FLOOR_YEAR_2_BPS);
        }
        const {
            assert!(TENURE_FLOOR_YEAR_2_BPS < TENURE_FLOOR_YEAR_5_BPS);
        }
    }

    #[test]
    fn tenure_reductions_are_progressive() {
        const {
            assert!(TENURE_REDUCTION_YEAR_0_BPS < TENURE_REDUCTION_YEAR_1_BPS);
        }
        const {
            assert!(TENURE_REDUCTION_YEAR_1_BPS < TENURE_REDUCTION_YEAR_2_BPS);
        }
        const {
            assert!(TENURE_REDUCTION_YEAR_2_BPS < TENURE_REDUCTION_YEAR_5_BPS);
        }
    }

    #[test]
    fn grace_periods_are_distinct() {
        assert_eq!(INACTIVITY_GRACE_PERIOD_DAYS, 90);
        assert_eq!(NEW_STAKE_GRACE_PERIOD_DAYS, 365);
    }
}
