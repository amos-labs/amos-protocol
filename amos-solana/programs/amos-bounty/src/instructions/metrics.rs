/// AMOS Bounty Program - Platform Metrics Instructions
///
/// Oracle-fed on-chain metrics for dynamic decay rate computation.
/// The PlatformMetrics singleton tracks rolling 30-day economic health
/// and derives the effective decay rate from profit ratio.
use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::BountyError;
use crate::state::*;

// ============================================================================
// Initialize Platform Metrics
// ============================================================================

#[derive(Accounts)]
pub struct InitializePlatformMetrics<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        init,
        payer = oracle_authority,
        space = PlatformMetrics::SIZE,
        seeds = [PLATFORM_METRICS_SEED],
        bump
    )]
    pub platform_metrics: Account<'info, PlatformMetrics>,

    #[account(mut)]
    pub oracle_authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler_initialize_metrics(ctx: Context<InitializePlatformMetrics>) -> Result<()> {
    let metrics = &mut ctx.accounts.platform_metrics;
    let clock = Clock::get()?;

    metrics.commercial_volume_30d = 0;
    metrics.fees_collected_30d = 0;
    metrics.fees_to_holders_30d = 0;
    metrics.fees_burned_30d = 0;
    metrics.fees_to_labs_30d = 0;
    metrics.system_volume_30d = 0;
    metrics.profit_ratio_bps = 0;
    metrics.computed_decay_rate_bps = DEFAULT_DECAY_RATE_BPS;
    metrics.commercial_bounty_count = 0;
    metrics.treasury_bounty_count = 0;
    metrics.last_updated = clock.unix_timestamp;
    metrics.bump = ctx.bumps.platform_metrics;
    metrics.reserved = [0; 16];

    msg!("Platform metrics initialized");
    Ok(())
}

// ============================================================================
// Update Platform Metrics (Oracle-only)
// ============================================================================

/// Oracle pushes rolling 30-day economic metrics on-chain.
/// From these, the effective decay rate is computed:
///   decay = base_10% - (profit_ratio × 5%), clamped to [2%, 25%]
///
/// This is a TRUSTED operation — the oracle is responsible for
/// computing accurate 30-day rolling windows off-chain.
#[derive(Accounts)]
pub struct UpdatePlatformMetrics<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        mut,
        seeds = [PLATFORM_METRICS_SEED],
        bump = platform_metrics.bump,
    )]
    pub platform_metrics: Account<'info, PlatformMetrics>,

    pub oracle_authority: Signer<'info>,
}

pub fn handler_update_metrics(
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
    let clock = Clock::get()?;
    let metrics = &mut ctx.accounts.platform_metrics;
    let config = &mut ctx.accounts.config;

    // Update raw metrics
    metrics.commercial_volume_30d = commercial_volume_30d;
    metrics.fees_collected_30d = fees_collected_30d;
    metrics.fees_to_holders_30d = fees_to_holders_30d;
    metrics.fees_burned_30d = fees_burned_30d;
    metrics.fees_to_labs_30d = fees_to_labs_30d;
    metrics.system_volume_30d = system_volume_30d;
    metrics.commercial_bounty_count = commercial_bounty_count;
    metrics.treasury_bounty_count = treasury_bounty_count;

    // ========================================================================
    // Compute Profit Ratio and Decay Rate
    // ========================================================================

    // profit_ratio = fees_collected / system_volume (as bps, capped at 10000)
    let profit_ratio_bps = if system_volume_30d > 0 {
        let ratio = fees_collected_30d
            .checked_mul(BPS_DENOMINATOR as u64)
            .ok_or(BountyError::ArithmeticOverflow)?
            .checked_div(system_volume_30d)
            .ok_or(BountyError::ArithmeticOverflow)?;
        (ratio as u16).min(BPS_DENOMINATOR)
    } else {
        0
    };
    metrics.profit_ratio_bps = profit_ratio_bps;

    // Dynamic decay: base_10% - (profit_ratio * 5%), clamped [2%, 25%]
    // BASE_DECAY_RATE_BPS = 1000 (10%)
    // DECAY_PROFIT_MULTIPLIER_BPS = 500 (5%)
    //
    // reduction = profit_ratio_bps * DECAY_PROFIT_MULTIPLIER / BPS_DENOMINATOR
    // effective_decay = BASE_DECAY - reduction, clamped
    let reduction = (profit_ratio_bps as u64)
        .checked_mul(DECAY_PROFIT_MULTIPLIER_BPS as u64)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    let effective_decay = if (BASE_DECAY_RATE_BPS as u64) > reduction {
        (BASE_DECAY_RATE_BPS as u64 - reduction) as u16
    } else {
        MIN_DECAY_RATE_BPS
    };

    let clamped_decay = effective_decay
        .max(MIN_DECAY_RATE_BPS)
        .min(MAX_DECAY_RATE_BPS);
    metrics.computed_decay_rate_bps = clamped_decay;

    // Update the config's decay rate to match computed value
    config.decay_rate_bps = clamped_decay;

    metrics.last_updated = clock.unix_timestamp;

    emit!(PlatformMetricsUpdated {
        commercial_volume_30d,
        system_volume_30d,
        fees_collected_30d,
        profit_ratio_bps,
        computed_decay_rate_bps: clamped_decay,
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "Platform metrics updated: profit_ratio={}bps, decay_rate={}bps",
        profit_ratio_bps,
        clamped_decay
    );

    Ok(())
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct PlatformMetricsUpdated {
    pub commercial_volume_30d: u64,
    pub system_volume_30d: u64,
    pub fees_collected_30d: u64,
    pub profit_ratio_bps: u16,
    pub computed_decay_rate_bps: u16,
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::*;

    #[test]
    fn test_decay_rate_formula_no_profit() {
        // 0% profit ratio → base 10% decay
        let profit_ratio_bps: u16 = 0;
        let reduction =
            (profit_ratio_bps as u64) * DECAY_PROFIT_MULTIPLIER_BPS as u64 / BPS_DENOMINATOR as u64;
        let decay = (BASE_DECAY_RATE_BPS as u64 - reduction) as u16;
        assert_eq!(decay, 1000); // 10%
    }

    #[test]
    fn test_decay_rate_formula_full_profit() {
        // 100% profit ratio → 10% - 5% = 5% decay
        let profit_ratio_bps: u16 = 10000;
        let reduction =
            (profit_ratio_bps as u64) * DECAY_PROFIT_MULTIPLIER_BPS as u64 / BPS_DENOMINATOR as u64;
        let decay = (BASE_DECAY_RATE_BPS as u64 - reduction) as u16;
        assert_eq!(decay, 500); // 5%
    }

    #[test]
    fn test_decay_rate_formula_50_percent_profit() {
        // 50% profit ratio → 10% - 2.5% = 7.5% decay
        let profit_ratio_bps: u16 = 5000;
        let reduction =
            (profit_ratio_bps as u64) * DECAY_PROFIT_MULTIPLIER_BPS as u64 / BPS_DENOMINATOR as u64;
        let decay = (BASE_DECAY_RATE_BPS as u64 - reduction) as u16;
        assert_eq!(decay, 750); // 7.5%
    }

    #[test]
    fn test_decay_rate_clamped_to_minimum() {
        // Even with extreme profit, decay cannot go below 2%
        let decay_bps: u16 = 100; // Would be below min
        let clamped = decay_bps.max(MIN_DECAY_RATE_BPS).min(MAX_DECAY_RATE_BPS);
        assert_eq!(clamped, MIN_DECAY_RATE_BPS); // 200 = 2%
    }

    #[test]
    fn test_decay_rate_clamped_to_maximum() {
        // Decay cannot exceed 25%
        let decay_bps: u16 = 3000; // Would be above max
        let clamped = decay_bps.max(MIN_DECAY_RATE_BPS).min(MAX_DECAY_RATE_BPS);
        assert_eq!(clamped, MAX_DECAY_RATE_BPS); // 2500 = 25%
    }
}
