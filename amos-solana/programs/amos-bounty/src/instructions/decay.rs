/// AMOS Bounty Program - Decay Instructions
///
/// This module implements the token decay mechanism that recycles unused tokens
/// back to the treasury while burning a small portion for deflationary pressure.
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};

use crate::constants::*;
use crate::errors::BountyError;
use crate::state::*;

// ============================================================================
// Apply Decay
// ============================================================================

/// Apply decay to an operator's balance and recycle tokens to treasury.
///
/// This is a PUBLIC GOOD function - anyone can trigger it to keep the system healthy.
/// Decay helps ensure tokens flow through the economy rather than accumulating.
///
/// # Decay Mechanics
/// - Grace period: 90 days before decay begins (INACTIVITY_GRACE_PERIOD_DAYS)
/// - Annual rate: 2-25% per year, configurable by oracle (decay_rate_bps)
/// - Daily decay: balance × rate / (10000 × 365), minimum 1 token
/// - Floor: Preserves at least 10% of original allocation (DECAY_FLOOR_BPS)
/// - Distribution: 10% burned, 90% recycled to treasury (DECAY_BURN_PORTION_BPS)
///
/// # Trustless Guarantees
/// - Time-locked: Cannot decay before grace period expires
/// - Rate-limited: Daily decay cannot exceed protocol bounds
/// - Floor-protected: Cannot decay below 10% of original allocation
/// - Transparent: All decay is recorded on-chain
/// - Permissionless: Anyone can trigger (public good)
/// - Reversible: Earning new tokens resets decay timeline
#[derive(Accounts)]
pub struct ApplyDecay<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = mint @ BountyError::InvalidMint,
        has_one = treasury @ BountyError::InvalidTreasury
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        mut,
        seeds = [OPERATOR_STATS_SEED, operator.key().as_ref()],
        bump = operator_stats.bump,
        has_one = operator @ BountyError::InvalidOperator
    )]
    pub operator_stats: Account<'info, OperatorStats>,

    /// The operator whose balance is being decayed
    /// CHECK: Validated through operator_stats PDA derivation
    pub operator: AccountInfo<'info>,

    /// Operator's token account
    #[account(
        mut,
        constraint = operator_token_account.mint == mint.key() @ BountyError::InvalidMint,
        constraint = operator_token_account.owner == operator.key() @ BountyError::InvalidOperator
    )]
    pub operator_token_account: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,

    #[account(mut)]
    pub treasury: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handler_apply_decay(ctx: Context<ApplyDecay>) -> Result<()> {
    let clock = Clock::get()?;
    let config = &ctx.accounts.config;
    let operator_stats = &mut ctx.accounts.operator_stats;

    // ========================================================================
    // Validation Phase
    // ========================================================================

    // Check if there's a balance to decay
    let current_balance = operator_stats.decayable_balance;
    require!(current_balance > 0, BountyError::NoDecayApplicable);

    // Check grace period (90 days since last activity)
    let time_since_activity = clock
        .unix_timestamp
        .checked_sub(operator_stats.last_activity_time)
        .ok_or(BountyError::InvalidTimestamp)?;

    let grace_period_seconds = INACTIVITY_GRACE_PERIOD_DAYS
        .checked_mul(86400)
        .ok_or(BountyError::ArithmeticOverflow)?;

    require!(
        time_since_activity >= grace_period_seconds as i64,
        BountyError::DecayGracePeriodActive
    );

    // Check decay floor (cannot decay below 10% of original)
    let floor_amount = operator_stats
        .original_allocation
        .checked_mul(DECAY_FLOOR_BPS as u64)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    let remaining_after_previous_decay = operator_stats
        .original_allocation
        .checked_sub(operator_stats.tokens_decayed)
        .ok_or(BountyError::ArithmeticUnderflow)?;

    require!(
        remaining_after_previous_decay > floor_amount,
        BountyError::DecayFloorReached
    );

    // ========================================================================
    // Calculate Decay Amount
    // ========================================================================

    // Calculate days since last decay
    let time_since_last_decay = clock
        .unix_timestamp
        .checked_sub(operator_stats.last_decay_time)
        .ok_or(BountyError::InvalidTimestamp)?;

    let days_since_decay = (time_since_last_decay as u64)
        .checked_div(86400)
        .ok_or(BountyError::ArithmeticOverflow)?
        .max(1); // At least 1 day

    // Daily decay rate = annual_rate / 365
    // Daily decay amount = balance × (rate_bps / 10000) / 365
    //
    // To maintain precision: (balance × rate_bps × days) / (10000 × 365)

    let decay_numerator = current_balance
        .checked_mul(config.decay_rate_bps as u64)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_mul(days_since_decay)
        .ok_or(BountyError::ArithmeticOverflow)?;

    let decay_denominator = (BPS_DENOMINATOR as u64)
        .checked_mul(365)
        .ok_or(BountyError::ArithmeticOverflow)?;

    let mut decay_amount = decay_numerator
        .checked_div(decay_denominator)
        .ok_or(BountyError::ArithmeticOverflow)?;

    // Minimum 1 token per decay application (if any decay is due)
    if decay_amount == 0 && time_since_last_decay >= 86400 {
        decay_amount = 1;
    }

    require!(decay_amount > 0, BountyError::NoDecayApplicable);

    // Ensure decay doesn't breach the floor
    let total_decay_after = operator_stats
        .tokens_decayed
        .checked_add(decay_amount)
        .ok_or(BountyError::ArithmeticOverflow)?;

    let remaining_after_decay = operator_stats
        .original_allocation
        .checked_sub(total_decay_after)
        .ok_or(BountyError::ArithmeticUnderflow)?;

    if remaining_after_decay < floor_amount {
        // Adjust decay to reach floor exactly
        decay_amount = remaining_after_previous_decay
            .checked_sub(floor_amount)
            .ok_or(BountyError::ArithmeticUnderflow)?;
    }

    require!(decay_amount > 0, BountyError::NoDecayApplicable);

    // Cap decay at current balance
    decay_amount = decay_amount.min(current_balance);

    // ========================================================================
    // Split Decay: 10% Burn, 90% Recycle
    // ========================================================================

    let burn_amount = decay_amount
        .checked_mul(DECAY_BURN_PORTION_BPS as u64)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    let recycle_amount = decay_amount
        .checked_sub(burn_amount)
        .ok_or(BountyError::ArithmeticUnderflow)?;

    // ========================================================================
    // Execute Token Operations
    // ========================================================================

    // Burn tokens (if burn amount > 0)
    if burn_amount > 0 {
        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.mint.to_account_info(),
                    from: ctx.accounts.operator_token_account.to_account_info(),
                    authority: ctx.accounts.operator.to_account_info(),
                },
            ),
            burn_amount,
        )?;
    }

    // Recycle tokens to treasury (if recycle amount > 0)
    if recycle_amount > 0 {
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.operator_token_account.to_account_info(),
                    to: ctx.accounts.treasury.to_account_info(),
                    authority: ctx.accounts.operator.to_account_info(),
                },
            ),
            recycle_amount,
        )?;
    }

    // ========================================================================
    // Update State
    // ========================================================================

    // Update operator stats
    operator_stats.decayable_balance = operator_stats
        .decayable_balance
        .checked_sub(decay_amount)
        .ok_or(BountyError::ArithmeticUnderflow)?;

    operator_stats.tokens_decayed = operator_stats
        .tokens_decayed
        .checked_add(decay_amount)
        .ok_or(BountyError::ArithmeticOverflow)?;

    operator_stats.tokens_burned = operator_stats
        .tokens_burned
        .checked_add(burn_amount)
        .ok_or(BountyError::ArithmeticOverflow)?;

    operator_stats.tokens_recycled = operator_stats
        .tokens_recycled
        .checked_add(recycle_amount)
        .ok_or(BountyError::ArithmeticOverflow)?;

    operator_stats.last_decay_time = clock.unix_timestamp;

    operator_stats.decay_applications = operator_stats
        .decay_applications
        .checked_add(1)
        .ok_or(BountyError::ArithmeticOverflow)?;

    // ========================================================================
    // Emit Event
    // ========================================================================

    emit!(DecayApplied {
        operator: ctx.accounts.operator.key(),
        decay_amount,
        burn_amount,
        recycle_amount,
        remaining_balance: operator_stats.decayable_balance,
        days_elapsed: days_since_decay,
        timestamp: clock.unix_timestamp,
    });

    msg!("Decay applied successfully");
    msg!("Total decayed: {} tokens", decay_amount);
    msg!(
        "Burned: {} tokens, Recycled: {} tokens",
        burn_amount,
        recycle_amount
    );
    msg!(
        "Remaining balance: {} tokens",
        operator_stats.decayable_balance
    );

    Ok(())
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct DecayApplied {
    pub operator: Pubkey,
    pub decay_amount: u64,
    pub burn_amount: u64,
    pub recycle_amount: u64,
    pub remaining_balance: u64,
    pub days_elapsed: u64,
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decay_calculation() {
        // Test: 10,000 token balance, 5% annual rate, 30 days elapsed
        let balance = 10000u64;
        let rate_bps = 500u16; // 5%
        let days = 30u64;

        let decay = (balance * rate_bps as u64 * days) / (10000 * 365);

        // Expected: 10000 × 0.05 / 365 × 30 ≈ 41 tokens
        assert!(decay >= 40 && decay <= 42);
    }

    #[test]
    fn test_decay_split() {
        let decay_amount = 1000u64;

        let burn = decay_amount * DECAY_BURN_PORTION_BPS as u64 / BPS_DENOMINATOR as u64;
        let recycle = decay_amount - burn;

        // 10% burned, 90% recycled
        assert_eq!(burn, 100);
        assert_eq!(recycle, 900);
    }

    #[test]
    fn test_decay_floor() {
        let original = 10000u64;
        let floor = original * DECAY_FLOOR_BPS as u64 / BPS_DENOMINATOR as u64;

        // Floor should be 10% of original
        assert_eq!(floor, 1000);

        // Max decay is 90% of original
        let max_decay = original - floor;
        assert_eq!(max_decay, 9000);
    }

    #[test]
    fn test_grace_period() {
        let grace_days = INACTIVITY_GRACE_PERIOD_DAYS;
        let grace_seconds = grace_days * 86400;

        // 90 days = 7,776,000 seconds
        assert_eq!(grace_seconds, 7_776_000);
    }

    #[test]
    fn test_annual_decay_bounds() {
        // Minimum 2% annual
        assert_eq!(MIN_DECAY_RATE_BPS, 200);

        // Maximum 25% annual
        assert_eq!(MAX_DECAY_RATE_BPS, 2500);

        // Default 5% annual
        assert_eq!(DEFAULT_DECAY_RATE_BPS, 500);
    }
}
