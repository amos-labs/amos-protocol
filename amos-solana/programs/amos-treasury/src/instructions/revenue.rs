//! Legacy V1 distribution is disabled because destination custody and accrual
//! were not authenticated. V2 distributes new payer funds only.
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::constants::seeds;
use crate::errors::TreasuryError;
use crate::state::{Distribution, HolderPool, TreasuryConfig};

// ============================================================================
// Distribute Protocol Fee
// ============================================================================

/// Distribute an AMOS protocol fee according to the 50/40/10 split.
///
/// Fee distribution:
/// - 50% to holder pool (staker revenue share)
/// - 40% permanently burned (deflationary)
/// - 10% to Labs wallet (operations)
///
/// Labs receives the remainder after holder and burn shares
/// to absorb any rounding dust.
///
/// # Arguments
/// * `amount` - Total fee amount in AMOS tokens
/// * `payment_reference` - Reference ID for tracking (bounty ID, etc.)
pub fn distribute_protocol_fee(
    _ctx: Context<DistributeProtocolFee>,
    _amount: u64,
    _payment_reference: String,
) -> Result<()> {
    // No migration can authenticate old arbitrary-vault balances or reward debt.
    err!(TreasuryError::LegacyAccountingDisabled)
}

#[derive(Accounts)]
#[instruction(amount: u64, payment_reference: String)]
pub struct DistributeProtocolFee<'info> {
    /// Payer of transaction fees
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Treasury configuration
    #[account(
        mut,
        seeds = [seeds::TREASURY_CONFIG],
        bump = treasury_config.bump,
    )]
    pub treasury_config: Box<Account<'info, TreasuryConfig>>,

    /// Holder pool state
    #[account(
        mut,
        seeds = [seeds::HOLDER_POOL],
        bump = holder_pool.bump,
    )]
    pub holder_pool: Box<Account<'info, HolderPool>>,

    /// Distribution record (created for this transaction)
    #[account(
        init,
        payer = payer,
        space = Distribution::LEN,
        seeds = [
            seeds::DISTRIBUTION,
            &treasury_config.distribution_count.checked_add(1).unwrap().to_le_bytes()
        ],
        bump
    )]
    pub distribution: Box<Account<'info, Distribution>>,

    /// AMOS token mint (for burning)
    #[account(
        mut,
        address = treasury_config.amos_mint,
    )]
    pub amos_mint: Box<Account<'info, Mint>>,

    /// Treasury AMOS vault
    #[account(
        mut,
        seeds = [seeds::TREASURY_AMOS],
        bump,
        token::mint = treasury_config.amos_mint,
        token::authority = treasury_config,
    )]
    pub treasury_amos_vault: Box<Account<'info, TokenAccount>>,

    /// Holder pool AMOS account
    #[account(
        mut,
        token::mint = treasury_config.amos_mint,
    )]
    pub holder_pool_amos: Box<Account<'info, TokenAccount>>,

    /// Labs wallet AMOS account
    #[account(
        mut,
        token::mint = treasury_config.amos_mint,
    )]
    pub labs_wallet_amos: Box<Account<'info, TokenAccount>>,

    /// SPL Token program
    pub token_program: Program<'info, Token>,

    /// System program
    pub system_program: Program<'info, System>,
}
