/// AMOS Treasury Transparency Instructions
///
/// Read-only view functions for querying treasury state and history.
/// AMOS-only model: all stats are in AMOS tokens.
use anchor_lang::prelude::*;

use crate::constants::seeds;
use crate::errors::TreasuryError;
use crate::state::{Distribution, HolderPool, TreasuryConfig, TreasuryStats};

// ============================================================================
// Get Treasury State
// ============================================================================

/// Get current treasury statistics (read-only).
pub fn get_treasury_state(ctx: Context<GetTreasuryState>) -> Result<TreasuryStats> {
    let treasury_config = &ctx.accounts.treasury_config;
    let holder_pool = &ctx.accounts.holder_pool;

    Ok(TreasuryStats {
        total_fees_collected: treasury_config.total_fees_collected,
        total_fees_to_holders: treasury_config.total_fees_to_holders,
        total_fees_burned: treasury_config.total_fees_burned,
        total_fees_to_labs: treasury_config.total_fees_to_labs,
        total_amos_burned: treasury_config.total_amos_burned,
        distribution_count: treasury_config.distribution_count,
        total_stakes: treasury_config.total_stakes,
        total_staked_amount: treasury_config.total_staked_amount,
        holder_pool_amos: holder_pool.amos_balance,
        initialized_at: treasury_config.initialized_at,
        last_distribution_at: treasury_config.last_distribution_at,
    })
}

#[derive(Accounts)]
pub struct GetTreasuryState<'info> {
    #[account(
        seeds = [seeds::TREASURY_CONFIG],
        bump = treasury_config.bump,
    )]
    pub treasury_config: Account<'info, TreasuryConfig>,

    #[account(
        seeds = [seeds::HOLDER_POOL],
        bump = holder_pool.bump,
    )]
    pub holder_pool: Account<'info, HolderPool>,
}

// ============================================================================
// Get Distribution by Index
// ============================================================================

/// Get a specific distribution record by index (read-only).
pub fn get_distribution(ctx: Context<GetDistribution>, index: u64) -> Result<Distribution> {
    let distribution = &ctx.accounts.distribution;
    let treasury_config = &ctx.accounts.treasury_config;

    require!(
        index > 0 && index <= treasury_config.distribution_count,
        TreasuryError::DistributionIndexOutOfBounds
    );

    Ok(Distribution {
        index: distribution.index,
        timestamp: distribution.timestamp,
        total_amount: distribution.total_amount,
        amount_to_holders: distribution.amount_to_holders,
        amount_burned: distribution.amount_burned,
        amount_to_labs: distribution.amount_to_labs,
        payment_reference: distribution.payment_reference.clone(),
        bump: distribution.bump,
    })
}

#[derive(Accounts)]
#[instruction(index: u64)]
pub struct GetDistribution<'info> {
    #[account(
        seeds = [seeds::TREASURY_CONFIG],
        bump = treasury_config.bump,
    )]
    pub treasury_config: Account<'info, TreasuryConfig>,

    #[account(
        seeds = [seeds::DISTRIBUTION, &index.to_le_bytes()],
        bump = distribution.bump,
    )]
    pub distribution: Account<'info, Distribution>,
}
