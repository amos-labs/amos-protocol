/// AMOS Treasury Claims Instructions
///
/// Handles stake registration and AMOS revenue claiming.
/// AMOS-only model: all claims are in AMOS tokens from the holder pool.
///
/// Trust guarantees:
/// - No approval needed for claims
/// - Proportional distribution based on stake
/// - 30-day minimum hold period prevents gaming
/// - All arithmetic uses checked operations
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::constants::{seeds, MIN_STAKE_AMOUNT, MIN_STAKE_DAYS};
use crate::errors::TreasuryError;
use crate::state::{ClaimableAmount, HolderPool, StakeRecord, TreasuryConfig};

// ============================================================================
// Register Stake
// ============================================================================

/// Register AMOS tokens for fee revenue sharing.
/// Minimum 100 AMOS, 30-day hold before claiming.
pub fn register_stake(ctx: Context<RegisterStake>, amount: u64) -> Result<()> {
    require!(amount >= MIN_STAKE_AMOUNT, TreasuryError::StakeAmountTooLow);
    require!(amount > 0, TreasuryError::ZeroStakeAmount);

    let stake_record = &mut ctx.accounts.stake_record;
    let treasury_config = &mut ctx.accounts.treasury_config;
    let clock = Clock::get()?;

    // Transfer AMOS tokens from user to stake vault
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_amos_account.to_account_info(),
                to: ctx.accounts.stake_vault.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
            },
        ),
        amount,
    )?;

    // Initialize stake record
    stake_record.owner = ctx.accounts.owner.key();
    stake_record.amount = amount;
    stake_record.staked_at = clock.unix_timestamp;
    stake_record.updated_at = clock.unix_timestamp;
    stake_record.last_claim_at = 0;
    stake_record.total_amos_claimed = 0;
    stake_record.claim_count = 0;
    stake_record.bump = ctx.bumps.stake_record;

    // Update treasury totals
    treasury_config.total_stakes = treasury_config
        .total_stakes
        .checked_add(1)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    treasury_config.total_staked_amount = treasury_config
        .total_staked_amount
        .checked_add(amount)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    msg!(
        "Stake registered: {} AMOS by {}",
        amount,
        stake_record.owner
    );

    Ok(())
}

#[derive(Accounts)]
#[instruction(amount: u64)]
pub struct RegisterStake<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [seeds::TREASURY_CONFIG],
        bump = treasury_config.bump,
    )]
    pub treasury_config: Account<'info, TreasuryConfig>,

    #[account(
        init,
        payer = owner,
        space = StakeRecord::LEN,
        seeds = [seeds::STAKE_RECORD, owner.key().as_ref()],
        bump
    )]
    pub stake_record: Account<'info, StakeRecord>,

    #[account(
        mut,
        token::mint = treasury_config.amos_mint,
        token::authority = owner,
    )]
    pub user_amos_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = treasury_config.amos_mint,
    )]
    pub stake_vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

// ============================================================================
// Update Stake
// ============================================================================

/// Update existing stake amount. Must maintain minimum 100 AMOS.
/// Increasing stake resets the 30-day timer.
pub fn update_stake(ctx: Context<UpdateStake>, new_amount: u64) -> Result<()> {
    require!(
        new_amount >= MIN_STAKE_AMOUNT,
        TreasuryError::StakeBelowMinimum
    );

    let stake_record = &mut ctx.accounts.stake_record;
    let treasury_config = &mut ctx.accounts.treasury_config;
    let clock = Clock::get()?;

    let old_amount = stake_record.amount;
    require!(new_amount != old_amount, TreasuryError::InvalidInput);

    if new_amount > old_amount {
        let additional = new_amount
            .checked_sub(old_amount)
            .ok_or(TreasuryError::ArithmeticUnderflow)?;

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user_amos_account.to_account_info(),
                    to: ctx.accounts.stake_vault.to_account_info(),
                    authority: ctx.accounts.owner.to_account_info(),
                },
            ),
            additional,
        )?;

        treasury_config.total_staked_amount = treasury_config
            .total_staked_amount
            .checked_add(additional)
            .ok_or(TreasuryError::ArithmeticOverflow)?;

        // Reset stake timer when increasing
        stake_record.staked_at = clock.unix_timestamp;
    } else {
        let reduction = old_amount
            .checked_sub(new_amount)
            .ok_or(TreasuryError::ArithmeticUnderflow)?;

        let treasury_seeds = &[seeds::TREASURY_CONFIG, &[treasury_config.bump]];
        let signer_seeds = &[&treasury_seeds[..]];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.stake_vault.to_account_info(),
                    to: ctx.accounts.user_amos_account.to_account_info(),
                    authority: treasury_config.to_account_info(),
                },
                signer_seeds,
            ),
            reduction,
        )?;

        treasury_config.total_staked_amount = treasury_config
            .total_staked_amount
            .checked_sub(reduction)
            .ok_or(TreasuryError::ArithmeticUnderflow)?;
    }

    stake_record.amount = new_amount;
    stake_record.updated_at = clock.unix_timestamp;

    msg!("Stake updated: {} → {} AMOS", old_amount, new_amount);

    Ok(())
}

#[derive(Accounts)]
#[instruction(new_amount: u64)]
pub struct UpdateStake<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [seeds::TREASURY_CONFIG],
        bump = treasury_config.bump,
    )]
    pub treasury_config: Account<'info, TreasuryConfig>,

    #[account(
        mut,
        seeds = [seeds::STAKE_RECORD, owner.key().as_ref()],
        bump = stake_record.bump,
        has_one = owner @ TreasuryError::NotStakeOwner,
    )]
    pub stake_record: Account<'info, StakeRecord>,

    #[account(
        mut,
        token::mint = treasury_config.amos_mint,
        token::authority = owner,
    )]
    pub user_amos_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = treasury_config.amos_mint,
    )]
    pub stake_vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

// ============================================================================
// Claim Revenue
// ============================================================================

/// Claim proportional share of AMOS fee revenue from holder pool.
/// Fully permissionless — no approval needed. 30-day minimum stake.
pub fn claim_revenue(ctx: Context<ClaimRevenue>) -> Result<()> {
    let stake_record = &mut ctx.accounts.stake_record;
    let treasury_config = &ctx.accounts.treasury_config;
    let holder_pool = &mut ctx.accounts.holder_pool;
    let clock = Clock::get()?;

    // Verify minimum stake period (30 days)
    let min_stake_seconds = (MIN_STAKE_DAYS as i64)
        .checked_mul(86400)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    require!(
        stake_record.can_claim(clock.unix_timestamp, min_stake_seconds),
        TreasuryError::MinimumStakePeriodNotMet
    );

    require!(
        treasury_config.total_staked_amount > 0,
        TreasuryError::DivisionByZero
    );

    // Calculate proportional AMOS claim
    let amos_claim = if holder_pool.amos_balance > 0 {
        stake_record
            .amount
            .checked_mul(holder_pool.amos_balance)
            .ok_or(TreasuryError::ArithmeticOverflow)?
            .checked_div(treasury_config.total_staked_amount)
            .ok_or(TreasuryError::DivisionByZero)?
    } else {
        0
    };

    require!(amos_claim > 0, TreasuryError::NoClaimableRevenue);
    require!(
        amos_claim <= holder_pool.amos_balance,
        TreasuryError::InsufficientHolderPoolFunds
    );

    // Transfer AMOS from holder pool to user
    let treasury_seeds = &[seeds::TREASURY_CONFIG, &[treasury_config.bump]];
    let signer_seeds = &[&treasury_seeds[..]];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.holder_pool_amos.to_account_info(),
                to: ctx.accounts.user_amos_account.to_account_info(),
                authority: treasury_config.to_account_info(),
            },
            signer_seeds,
        ),
        amos_claim,
    )?;

    // Update holder pool
    holder_pool.amos_balance = holder_pool
        .amos_balance
        .checked_sub(amos_claim)
        .ok_or(TreasuryError::ArithmeticUnderflow)?;

    holder_pool.total_amos_claimed = holder_pool
        .total_amos_claimed
        .checked_add(amos_claim)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    holder_pool.claim_count = holder_pool
        .claim_count
        .checked_add(1)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    holder_pool.last_claim_at = clock.unix_timestamp;

    // Update stake record
    stake_record.last_claim_at = clock.unix_timestamp;
    stake_record.total_amos_claimed = stake_record
        .total_amos_claimed
        .checked_add(amos_claim)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    stake_record.claim_count = stake_record
        .claim_count
        .checked_add(1)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    msg!("Revenue claimed: {} AMOS", amos_claim);

    Ok(())
}

#[derive(Accounts)]
pub struct ClaimRevenue<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        seeds = [seeds::TREASURY_CONFIG],
        bump = treasury_config.bump,
    )]
    pub treasury_config: Account<'info, TreasuryConfig>,

    #[account(
        mut,
        seeds = [seeds::STAKE_RECORD, owner.key().as_ref()],
        bump = stake_record.bump,
        has_one = owner @ TreasuryError::NotStakeOwner,
    )]
    pub stake_record: Account<'info, StakeRecord>,

    #[account(
        mut,
        seeds = [seeds::HOLDER_POOL],
        bump = holder_pool.bump,
    )]
    pub holder_pool: Account<'info, HolderPool>,

    /// Holder pool AMOS account
    #[account(
        mut,
        token::mint = treasury_config.amos_mint,
    )]
    pub holder_pool_amos: Account<'info, TokenAccount>,

    /// User's AMOS account (receives claim)
    #[account(
        mut,
        token::mint = treasury_config.amos_mint,
        token::authority = owner,
    )]
    pub user_amos_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

// ============================================================================
// Get Claimable Amount (View Function)
// ============================================================================

/// Query claimable AMOS revenue amount.
pub fn get_claimable_amount(ctx: Context<GetClaimableAmount>) -> Result<ClaimableAmount> {
    let stake_record = &ctx.accounts.stake_record;
    let treasury_config = &ctx.accounts.treasury_config;
    let holder_pool = &ctx.accounts.holder_pool;
    let clock = Clock::get()?;

    let min_stake_seconds = (MIN_STAKE_DAYS as i64)
        .checked_mul(86400)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    let can_claim = stake_record.can_claim(clock.unix_timestamp, min_stake_seconds);
    let days_staked = stake_record.stake_duration_days(clock.unix_timestamp);
    let days_remaining = if days_staked >= MIN_STAKE_DAYS {
        0
    } else {
        MIN_STAKE_DAYS - days_staked
    };

    let (amos_amount, share_bps) = if treasury_config.total_staked_amount > 0 {
        let share = stake_record
            .amount
            .checked_mul(10000)
            .ok_or(TreasuryError::ArithmeticOverflow)?
            .checked_div(treasury_config.total_staked_amount)
            .ok_or(TreasuryError::DivisionByZero)? as u16;

        let amos = stake_record
            .amount
            .checked_mul(holder_pool.amos_balance)
            .ok_or(TreasuryError::ArithmeticOverflow)?
            .checked_div(treasury_config.total_staked_amount)
            .ok_or(TreasuryError::DivisionByZero)?;

        (amos, share)
    } else {
        (0, 0)
    };

    Ok(ClaimableAmount {
        amos_amount,
        stake_amount: stake_record.amount,
        total_staked: treasury_config.total_staked_amount,
        share_bps,
        can_claim,
        days_staked,
        days_remaining,
    })
}

#[derive(Accounts)]
pub struct GetClaimableAmount<'info> {
    pub owner: Signer<'info>,

    #[account(
        seeds = [seeds::TREASURY_CONFIG],
        bump = treasury_config.bump,
    )]
    pub treasury_config: Account<'info, TreasuryConfig>,

    #[account(
        seeds = [seeds::STAKE_RECORD, owner.key().as_ref()],
        bump = stake_record.bump,
        has_one = owner @ TreasuryError::NotStakeOwner,
    )]
    pub stake_record: Account<'info, StakeRecord>,

    #[account(
        seeds = [seeds::HOLDER_POOL],
        bump = holder_pool.bump,
    )]
    pub holder_pool: Account<'info, HolderPool>,
}
