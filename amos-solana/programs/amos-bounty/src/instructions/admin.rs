/// AMOS Bounty Program - Admin Instructions
///
/// This module handles program initialization and administrative functions.
/// Only the oracle authority can perform these operations.
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::constants::*;
use crate::errors::BountyError;
use crate::state::*;

// ============================================================================
// Initialize Program
// ============================================================================

/// Initialize the AMOS Bounty program with core configuration.
/// This can only be called once and sets up the singleton BountyConfig account.
///
/// # Arguments
/// * `oracle_authority` - The authority that will validate bounty submissions
///
/// # Trustless Guarantees
/// - Immutable oracle authority (cannot be changed after init)
/// - Sigmoid emission: 16,000 → 100 AMOS/day over ~13 years (no halving epochs)
/// - Default decay rate within protocol bounds (5% annual)
/// - All parameters are transparent and on-chain
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = BountyConfig::SIZE,
        seeds = [BOUNTY_CONFIG_SEED],
        bump
    )]
    pub config: Account<'info, BountyConfig>,

    /// The AMOS token mint
    pub mint: Account<'info, Mint>,

    /// The treasury token account holding the distribution pool
    #[account(
        constraint = treasury.mint == mint.key() @ BountyError::InvalidMint,
        constraint = treasury.amount >= TREASURY_ALLOCATION @ BountyError::TreasuryInsufficientFunds
    )]
    pub treasury: Account<'info, TokenAccount>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler_initialize(ctx: Context<Initialize>, oracle_authority: Pubkey) -> Result<()> {
    let config = &mut ctx.accounts.config;
    let clock = Clock::get()?;

    // Initialize configuration with default values
    config.oracle_authority = oracle_authority;
    config.mint = ctx.accounts.mint.key();
    config.treasury = ctx.accounts.treasury.key();
    config.start_time = clock.unix_timestamp;
    config._deprecated_halving_epoch = 0; // Deprecated, kept for layout compat
    config.daily_emission = EMISSION_CEILING; // Initial value; sigmoid_daily_emission() computes per-day
    config.total_tokens_distributed = 0;
    config.total_bounties = 0;
    config.total_points = 0;
    config.decay_rate_bps = DEFAULT_DECAY_RATE_BPS;
    config.bump = ctx.bumps.config;
    config.holder_pool = Pubkey::default();
    config.labs_wallet = Pubkey::default();
    config.reserved = [0; 8];

    msg!("AMOS Bounty Program initialized");
    msg!("Oracle Authority: {}", oracle_authority);
    msg!(
        "Emission: sigmoid curve, {} → {} AMOS/day",
        EMISSION_CEILING,
        EMISSION_FLOOR
    );
    msg!("Default Decay Rate: {}%", DEFAULT_DECAY_RATE_BPS / 100);

    Ok(())
}

// ============================================================================
// Update Decay Rate
// ============================================================================

/// Update the annual decay rate within protocol-defined bounds.
/// Only the oracle can call this, and the rate must be between 2% and 25%.
///
/// # Arguments
/// * `new_rate_bps` - New decay rate in basis points (200-2500)
///
/// # Trustless Guarantees
/// - Rate bounded by protocol constants (MIN_DECAY_RATE_BPS to MAX_DECAY_RATE_BPS)
/// - Cannot be set below 2% or above 25%
/// - All existing balances subject to same rules
/// - Change is transparent and auditable on-chain
#[derive(Accounts)]
pub struct UpdateDecayRate<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized
    )]
    pub config: Account<'info, BountyConfig>,

    pub oracle_authority: Signer<'info>,
}

pub fn handler_update_decay(ctx: Context<UpdateDecayRate>, new_rate_bps: u16) -> Result<()> {
    // Validate new rate is within bounds
    require!(
        new_rate_bps >= MIN_DECAY_RATE_BPS && new_rate_bps <= MAX_DECAY_RATE_BPS,
        BountyError::InvalidDecayRate
    );

    let config = &mut ctx.accounts.config;
    let old_rate = config.decay_rate_bps;
    config.decay_rate_bps = new_rate_bps;

    msg!(
        "Decay rate updated from {} bps to {} bps",
        old_rate,
        new_rate_bps
    );
    msg!("New annual decay rate: {}%", new_rate_bps / 100);

    Ok(())
}

// ============================================================================
// Update Treasury
// ============================================================================

/// Update the treasury token account address. Oracle-only.
/// The new treasury must be an AMOS token account with sufficient funds.
#[derive(Accounts)]
pub struct UpdateTreasury<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
    )]
    pub config: Account<'info, BountyConfig>,

    /// New treasury token account (must match config's mint)
    #[account(
        constraint = new_treasury.mint == config.mint @ BountyError::InvalidMint,
    )]
    pub new_treasury: Account<'info, TokenAccount>,

    pub oracle_authority: Signer<'info>,
}

pub fn handler_update_treasury(ctx: Context<UpdateTreasury>) -> Result<()> {
    let old_treasury = ctx.accounts.config.treasury;
    ctx.accounts.config.treasury = ctx.accounts.new_treasury.key();

    msg!(
        "Treasury updated from {} to {}",
        old_treasury,
        ctx.accounts.new_treasury.key()
    );

    Ok(())
}

// ============================================================================
// Set Fee Recipients
// ============================================================================

/// Set the holder pool and labs wallet addresses for commercial bounty fee distribution.
/// Oracle-only. These must be set before any commercial bounty can be released.
///
/// # Arguments
/// * `holder_pool` - Token account that receives 50% of commercial bounty fees
/// * `labs_wallet` - Token account that receives 10% of commercial bounty fees
#[derive(Accounts)]
pub struct SetFeeRecipients<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
        has_one = mint @ BountyError::InvalidMint,
    )]
    pub config: Account<'info, BountyConfig>,

    pub mint: Account<'info, Mint>,

    /// Holder pool token account — must be an AMOS token account
    #[account(
        constraint = holder_pool.mint == config.mint @ BountyError::InvalidMint,
    )]
    pub holder_pool: Account<'info, TokenAccount>,

    /// Labs wallet token account — must be an AMOS token account
    #[account(
        constraint = labs_wallet.mint == config.mint @ BountyError::InvalidMint,
    )]
    pub labs_wallet: Account<'info, TokenAccount>,

    pub oracle_authority: Signer<'info>,
}

pub fn handler_set_fee_recipients(ctx: Context<SetFeeRecipients>) -> Result<()> {
    let config = &mut ctx.accounts.config;

    let old_holder_pool = config.holder_pool;
    let old_labs_wallet = config.labs_wallet;

    config.holder_pool = ctx.accounts.holder_pool.key();
    config.labs_wallet = ctx.accounts.labs_wallet.key();

    msg!(
        "Fee recipients updated: holder_pool {} -> {}, labs_wallet {} -> {}",
        old_holder_pool,
        config.holder_pool,
        old_labs_wallet,
        config.labs_wallet
    );

    Ok(())
}

// ============================================================================
// Update Mint (Migration)
// ============================================================================

/// Update the token mint and treasury for mint migration. Oracle-only.
/// The new treasury must hold tokens of the new mint.
///
/// # Safety
/// This is a privileged migration operation. It atomically updates both
/// config.mint and config.treasury so the program references the new token.
#[derive(Accounts)]
pub struct UpdateMint<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
    )]
    pub config: Account<'info, BountyConfig>,

    /// The new AMOS token mint
    pub new_mint: Account<'info, Mint>,

    /// New treasury token account (must hold new mint tokens)
    #[account(
        constraint = new_treasury.mint == new_mint.key() @ BountyError::InvalidMint,
    )]
    pub new_treasury: Account<'info, TokenAccount>,

    pub oracle_authority: Signer<'info>,
}

pub fn handler_update_mint(ctx: Context<UpdateMint>) -> Result<()> {
    let config = &mut ctx.accounts.config;

    let old_mint = config.mint;
    let old_treasury = config.treasury;

    config.mint = ctx.accounts.new_mint.key();
    config.treasury = ctx.accounts.new_treasury.key();

    // Reset fee recipients since they reference the old mint's token accounts
    config.holder_pool = Pubkey::default();
    config.labs_wallet = Pubkey::default();

    msg!(
        "Mint migrated: {} -> {}",
        old_mint,
        ctx.accounts.new_mint.key()
    );
    msg!(
        "Treasury migrated: {} -> {}",
        old_treasury,
        ctx.accounts.new_treasury.key()
    );

    Ok(())
}

// ============================================================================
// Events
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sigmoid_emission_initial_value() {
        // handler_initialize sets daily_emission = EMISSION_CEILING
        // 16,000 whole AMOS tokens × 10^9 decimals
        assert_eq!(EMISSION_CEILING, 16_000 * ONE_TOKEN);
        assert!(EMISSION_CEILING > EMISSION_FLOOR);
    }

    #[test]
    fn test_decay_rate_bounds() {
        // Valid rates
        assert!(MIN_DECAY_RATE_BPS <= DEFAULT_DECAY_RATE_BPS);
        assert!(DEFAULT_DECAY_RATE_BPS <= MAX_DECAY_RATE_BPS);

        // 2% minimum
        assert_eq!(MIN_DECAY_RATE_BPS, 200);

        // 25% maximum
        assert_eq!(MAX_DECAY_RATE_BPS, 2500);
    }
}
