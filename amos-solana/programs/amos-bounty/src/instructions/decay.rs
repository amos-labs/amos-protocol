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
/// Requires the holder's signature because these are ordinary SPL token accounts.
/// Applies bounded annual decay only to complete days after 90 days of inactivity.
/// New earnings restart the inactivity clock. No per-grant vesting or compulsory
/// debit of freely transferable wallet balances is implemented in this version.
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
    /// Holder authorization is required for the SPL burn and transfer.
    pub operator: Signer<'info>,

    /// Operator's token account
    #[account(
        mut,
        constraint = operator_token_account.mint == mint.key() @ BountyError::InvalidMint,
        constraint = operator_token_account.owner == operator.key() @ BountyError::InvalidOperator
    )]
    pub operator_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
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

    require!(
        (200..=2500).contains(&config.decay_rate_bps),
        BountyError::InvalidDecayRate
    );
    let current_balance = operator_stats
        .decayable_balance
        .min(ctx.accounts.operator_token_account.amount);
    let (decay_amount, accounted_through) = amos_protocol_math::decay_amount(
        current_balance,
        operator_stats.original_allocation,
        operator_stats.tokens_decayed,
        config.decay_rate_bps,
        operator_stats.last_activity_time,
        operator_stats.last_decay_time,
        clock.unix_timestamp,
    )
    .ok_or(BountyError::NoDecayApplicable)?;
    let grace_end = operator_stats
        .last_activity_time
        .checked_add(90 * 86400)
        .ok_or(BountyError::InvalidTimestamp)?;
    let days_since_decay = ((accounted_through as i128
        - operator_stats.last_decay_time.max(grace_end) as i128)
        / 86400) as u64;

    // ========================================================================
    // Split Decay: 10% Burn, 90% Recycle
    // ========================================================================

    let burn_amount =
        (decay_amount as u128 * DECAY_BURN_PORTION_BPS as u128 / BPS_DENOMINATOR as u128) as u64;

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

    operator_stats.last_decay_time = accounted_through;

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
