//! Legacy V1 account layouts remain for decoding and explicit rejection only.
//! All handlers fail closed. Use V2 accounts/instructions; see MIGRATION_V2.md.
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

use crate::constants::seeds;
use crate::errors::TreasuryError;
use crate::state::{ClaimableAmount, HolderPool, StakeRecord, TreasuryConfig};

// ============================================================================
// Register Stake
// ============================================================================

/// Register AMOS tokens for fee revenue sharing.
/// Minimum 100 AMOS, 30-day hold before claiming.
pub fn register_stake(_ctx: Context<RegisterStake>, _amount: u64) -> Result<()> {
    // No migration can authenticate old arbitrary-vault balances or reward debt.
    err!(TreasuryError::LegacyAccountingDisabled)
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
pub fn update_stake(_ctx: Context<UpdateStake>, _new_amount: u64) -> Result<()> {
    // No migration can authenticate old arbitrary-vault balances or reward debt.
    err!(TreasuryError::LegacyAccountingDisabled)
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
pub fn claim_revenue(_ctx: Context<ClaimRevenue>) -> Result<()> {
    // No migration can authenticate old arbitrary-vault balances or reward debt.
    err!(TreasuryError::LegacyAccountingDisabled)
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
pub fn get_claimable_amount(_ctx: Context<GetClaimableAmount>) -> Result<ClaimableAmount> {
    // No migration can authenticate old arbitrary-vault balances or reward debt.
    err!(TreasuryError::LegacyAccountingDisabled)
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
