/// AMOS Treasury Revenue Instructions
///
/// Handles receipt and distribution of AMOS protocol fees.
/// AMOS-only model: all fees denominated in AMOS tokens.
/// Fee split: 50% to holder pool, 40% burned, 10% to Labs wallet.
/// Labs gets remainder to handle rounding dust.
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};

use crate::constants::{seeds, BPS_DENOMINATOR, FEE_BURN_SHARE_BPS, FEE_HOLDER_SHARE_BPS};
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
    ctx: Context<DistributeProtocolFee>,
    amount: u64,
    payment_reference: String,
) -> Result<()> {
    require!(amount > 0, TreasuryError::ZeroRevenueAmount);
    require!(
        payment_reference.len() <= Distribution::MAX_PAYMENT_REF_LEN,
        TreasuryError::PaymentReferenceTooLong
    );
    require!(
        !payment_reference.is_empty(),
        TreasuryError::MissingPaymentReference
    );

    let treasury_config = &mut ctx.accounts.treasury_config;
    let holder_pool = &mut ctx.accounts.holder_pool;
    let distribution = &mut ctx.accounts.distribution;
    let clock = Clock::get()?;

    // Calculate distribution amounts using checked arithmetic

    // Holder share: 50%
    let holder_amount = amount
        .checked_mul(FEE_HOLDER_SHARE_BPS as u64)
        .ok_or(TreasuryError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(TreasuryError::DivisionByZero)?;

    // Burn share: 40%
    let burn_amount = amount
        .checked_mul(FEE_BURN_SHARE_BPS as u64)
        .ok_or(TreasuryError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(TreasuryError::DivisionByZero)?;

    // Labs gets remainder (10% + rounding dust)
    let labs_amount = amount
        .checked_sub(holder_amount)
        .ok_or(TreasuryError::ArithmeticUnderflow)?
        .checked_sub(burn_amount)
        .ok_or(TreasuryError::ArithmeticUnderflow)?;

    // Verify total equals input (critical invariant)
    let total_check = holder_amount
        .checked_add(burn_amount)
        .ok_or(TreasuryError::ArithmeticOverflow)?
        .checked_add(labs_amount)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    require!(total_check == amount, TreasuryError::RevenueSplitError);

    // Get PDA signer seeds
    let treasury_seeds = &[seeds::TREASURY_CONFIG, &[treasury_config.bump]];
    let signer_seeds = &[&treasury_seeds[..]];

    // Transfer to holder pool (50%)
    if holder_amount > 0 {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.treasury_amos_vault.to_account_info(),
                    to: ctx.accounts.holder_pool_amos.to_account_info(),
                    authority: treasury_config.to_account_info(),
                },
                signer_seeds,
            ),
            holder_amount,
        )?;
    }

    // Burn tokens (40%)
    if burn_amount > 0 {
        token::burn(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.amos_mint.to_account_info(),
                    from: ctx.accounts.treasury_amos_vault.to_account_info(),
                    authority: treasury_config.to_account_info(),
                },
                signer_seeds,
            ),
            burn_amount,
        )?;
    }

    // Transfer to Labs wallet (10% + rounding)
    if labs_amount > 0 {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.treasury_amos_vault.to_account_info(),
                    to: ctx.accounts.labs_wallet_amos.to_account_info(),
                    authority: treasury_config.to_account_info(),
                },
                signer_seeds,
            ),
            labs_amount,
        )?;
    }

    // Update treasury state
    treasury_config.total_fees_collected = treasury_config
        .total_fees_collected
        .checked_add(amount)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    treasury_config.total_fees_to_holders = treasury_config
        .total_fees_to_holders
        .checked_add(holder_amount)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    treasury_config.total_fees_burned = treasury_config
        .total_fees_burned
        .checked_add(burn_amount)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    treasury_config.total_fees_to_labs = treasury_config
        .total_fees_to_labs
        .checked_add(labs_amount)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    treasury_config.total_amos_burned = treasury_config
        .total_amos_burned
        .checked_add(burn_amount)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    treasury_config.distribution_count = treasury_config
        .distribution_count
        .checked_add(1)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    treasury_config.last_distribution_at = clock.unix_timestamp;

    // Update holder pool state
    holder_pool.amos_balance = holder_pool
        .amos_balance
        .checked_add(holder_amount)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    holder_pool.total_amos_deposited = holder_pool
        .total_amos_deposited
        .checked_add(holder_amount)
        .ok_or(TreasuryError::ArithmeticOverflow)?;

    holder_pool.last_deposit_at = clock.unix_timestamp;

    // Create distribution record
    distribution.index = treasury_config.distribution_count;
    distribution.timestamp = clock.unix_timestamp;
    distribution.total_amount = amount;
    distribution.amount_to_holders = holder_amount;
    distribution.amount_burned = burn_amount;
    distribution.amount_to_labs = labs_amount;
    distribution.payment_reference = payment_reference.clone();
    distribution.bump = ctx.bumps.distribution;

    msg!("Protocol fee distributed: {} AMOS", amount);
    msg!("To holders: {} (50%)", holder_amount);
    msg!("Burned: {} (40%)", burn_amount);
    msg!("To Labs: {} (10%)", labs_amount);

    Ok(())
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
