/// AMOS Bounty Program - Prepare Bounty Submission
///
/// Creates daily pool and operator stats accounts if they don't exist.
/// Must be called before submit_bounty_proof or release_commercial_bounty
/// in the same transaction. Idempotent: no-op if accounts already exist.
///
/// This instruction exists to keep init_if_needed account creation separate
/// from the main bounty logic, avoiding SBF stack frame overflow.
use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::BountyError;
use crate::state::*;

#[derive(Accounts)]
#[instruction(operator_key: Pubkey, day_index: u32)]
pub struct PrepareBountySubmission<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        init_if_needed,
        payer = payer,
        space = DailyPool::SIZE,
        seeds = [DAILY_POOL_SEED, &day_index.to_le_bytes()],
        bump
    )]
    pub daily_pool: Account<'info, DailyPool>,

    #[account(
        init_if_needed,
        payer = payer,
        space = OperatorStats::SIZE,
        seeds = [OPERATOR_STATS_SEED, operator_key.as_ref()],
        bump
    )]
    pub operator_stats: Account<'info, OperatorStats>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler_prepare(
    ctx: Context<PrepareBountySubmission>,
    operator_key: Pubkey,
    day_index: u32,
) -> Result<()> {
    let config = &ctx.accounts.config;
    let daily_pool = &mut ctx.accounts.daily_pool;
    let operator_stats = &mut ctx.accounts.operator_stats;

    // Validate day_index matches current day (belt-and-suspenders; PDA derivation also validates)
    let current_day = calculate_day_index(config.start_time)?;
    require!(day_index == current_day, BountyError::InvalidDayIndex);
    if daily_pool.day_index == 0 {
        daily_pool.day_index = current_day;
        // Compute emission from sigmoid curve — stateless, no halving epochs needed
        daily_pool.daily_emission = sigmoid_daily_emission(current_day as u64);
        daily_pool.tokens_distributed = 0;
        daily_pool.total_points = 0;
        daily_pool.proof_count = 0;
        daily_pool.finalized = false;
        daily_pool.growth_tokens_distributed = 0;
        daily_pool.growth_points = 0;
        daily_pool.technical_tokens_distributed = 0;
        daily_pool.technical_points = 0;
        daily_pool.bump = ctx.bumps.daily_pool;
    }

    // Initialize operator stats if newly created
    if operator_stats.operator == Pubkey::default() {
        operator_stats.operator = operator_key;
        operator_stats.bump = ctx.bumps.operator_stats;
        let clock = Clock::get()?;
        operator_stats.last_activity_time = clock.unix_timestamp;
        operator_stats.last_decay_time = clock.unix_timestamp;
        operator_stats.original_allocation = 0;
        operator_stats.active_claim_count = 0;
    }

    Ok(())
}

/// Calculate the current day index since program start
fn calculate_day_index(start_time: i64) -> Result<u32> {
    let clock = Clock::get()?;
    let elapsed = clock
        .unix_timestamp
        .checked_sub(start_time)
        .ok_or(BountyError::InvalidTimestamp)?;
    let days = (elapsed as u64)
        .checked_div(86400)
        .ok_or(BountyError::ArithmeticOverflow)?;
    Ok(days as u32)
}
