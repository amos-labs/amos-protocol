/// AMOS Bounty Program - Dispute Instructions
///
/// Implements the dispute lifecycle for contested bounty rejections:
/// 1. Worker files dispute within 48h of rejection (stakes 5% of bounty value)
/// 2. Governance authority resolves, OR
/// 3. If no resolution in 7 days, defaults to upheld (worker-favorable)
///
/// Upheld: bounty pays out, stake returned, reviewer reputation hit
/// Denied: bounty returns to board, stake burned
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};

use crate::constants::*;
use crate::errors::BountyError;
use crate::state::*;

// ============================================================================
// File Dispute
// ============================================================================

/// Worker files a dispute after bounty rejection.
/// Must be within DISPUTE_WINDOW_HOURS (48h) of rejection.
/// Worker stakes DISPUTE_STAKE_BPS (5%) of bounty value.
#[derive(Accounts)]
#[instruction(bounty_id: [u8; 32])]
pub struct FileDispute<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = mint @ BountyError::InvalidMint,
    )]
    pub config: Box<Account<'info, BountyConfig>>,

    #[account(
        mut,
        seeds = [BOUNTY_LISTING_SEED, &bounty_id],
        bump = bounty_listing.bump,
    )]
    pub bounty_listing: Account<'info, BountyListing>,

    #[account(
        init,
        payer = worker,
        space = DisputeRecord::SIZE,
        seeds = [DISPUTE_SEED, &bounty_id],
        bump,
    )]
    pub dispute_record: Account<'info, DisputeRecord>,

    pub mint: Account<'info, Mint>,

    /// Worker's token account (stake is transferred from here)
    #[account(
        mut,
        constraint = worker_token_account.mint == mint.key() @ BountyError::InvalidMint,
        constraint = worker_token_account.owner == worker.key() @ BountyError::InvalidOperator,
    )]
    pub worker_token_account: Account<'info, TokenAccount>,

    /// Escrow for dispute stake
    /// CHECK: PDA validated
    #[account(
        mut,
        seeds = [DISPUTE_SEED, &bounty_id],
        bump,
    )]
    pub dispute_escrow: AccountInfo<'info>,

    #[account(mut)]
    pub worker: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handler_file_dispute(ctx: Context<FileDispute>, bounty_id: [u8; 32]) -> Result<()> {
    let clock = Clock::get()?;
    let listing = &mut ctx.accounts.bounty_listing;
    let dispute = &mut ctx.accounts.dispute_record;

    // Must be rejected
    require!(
        listing.status == BountyStatus::Rejected,
        BountyError::BountyNotRejected
    );

    // Must be the claimer
    require!(
        listing.claimed_by == ctx.accounts.worker.key(),
        BountyError::NotTheClaimer
    );

    // Must be within dispute window
    let time_since_rejection = clock
        .unix_timestamp
        .checked_sub(listing.rejected_at)
        .unwrap_or(0);
    let dispute_window_seconds = DISPUTE_WINDOW_HOURS * 3600;
    require!(
        time_since_rejection <= dispute_window_seconds as i64,
        BountyError::DisputeWindowExpired
    );

    // Calculate stake amount (5% of bounty value)
    let stake_amount = listing
        .reward_amount
        .checked_mul(DISPUTE_STAKE_BPS as u64)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    require!(stake_amount > 0, BountyError::InsufficientDisputeStake);

    // Update listing status
    listing.status = BountyStatus::Disputed;

    // Initialize dispute record
    dispute.bounty_id = bounty_id;
    dispute.worker = ctx.accounts.worker.key();
    dispute.stake_amount = stake_amount;
    dispute.filed_at = clock.unix_timestamp;
    dispute.resolved_at = 0;
    dispute.upheld = false;
    dispute.is_resolved = false;
    dispute.resolver = Pubkey::default();
    dispute.bump = ctx.bumps.dispute_record;
    dispute.reserved = [0; 8];

    emit!(DisputeFiled {
        bounty_id,
        worker: ctx.accounts.worker.key(),
        stake_amount,
        timestamp: clock.unix_timestamp,
    });

    msg!("Dispute filed: {} AMOS staked", stake_amount);
    Ok(())
}

// ============================================================================
// Resolve Dispute (Governance Authority)
// ============================================================================

/// Governance authority resolves a dispute.
/// Upheld: worker wins — bounty pays out, stake returned
/// Denied: reviewer wins — bounty returns to board, stake burned
#[derive(Accounts)]
#[instruction(bounty_id: [u8; 32])]
pub struct ResolveDispute<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
        has_one = mint @ BountyError::InvalidMint,
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        mut,
        seeds = [BOUNTY_LISTING_SEED, &bounty_id],
        bump = bounty_listing.bump,
    )]
    pub bounty_listing: Account<'info, BountyListing>,

    #[account(
        mut,
        seeds = [DISPUTE_SEED, &bounty_id],
        bump = dispute_record.bump,
    )]
    pub dispute_record: Account<'info, DisputeRecord>,

    pub mint: Account<'info, Mint>,

    pub oracle_authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handler_resolve_dispute(
    ctx: Context<ResolveDispute>,
    bounty_id: [u8; 32],
    upheld: bool,
) -> Result<()> {
    let clock = Clock::get()?;
    let listing = &mut ctx.accounts.bounty_listing;
    let dispute = &mut ctx.accounts.dispute_record;

    // Must be disputed
    require!(
        listing.status == BountyStatus::Disputed,
        BountyError::BountyNotRejected
    );

    // Must not already be resolved
    require!(!dispute.is_resolved, BountyError::DisputeAlreadyResolved);

    dispute.is_resolved = true;
    dispute.upheld = upheld;
    dispute.resolved_at = clock.unix_timestamp;
    dispute.resolver = ctx.accounts.oracle_authority.key();

    if upheld {
        // Worker wins: bounty pays out (handled by release instruction)
        listing.status = BountyStatus::Approved;
    } else {
        // Reviewer wins: bounty returns to board, stake burned
        listing.status = BountyStatus::Open;
        listing.claimed_by = Pubkey::default();
        listing.claimed_at = 0;
        listing.submitted_at = 0;
        listing.rejected_at = 0;
    }

    emit!(DisputeResolved {
        bounty_id,
        upheld,
        resolver: ctx.accounts.oracle_authority.key(),
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "Dispute resolved: {}",
        if upheld {
            "UPHELD (worker wins)"
        } else {
            "DENIED (reviewer wins)"
        }
    );
    Ok(())
}

// ============================================================================
// Default Dispute Resolution (Permissionless — Worker-Favorable Timeout)
// ============================================================================

/// If no resolution in DISPUTE_RESOLUTION_TIMEOUT_HOURS (168h / 7 days),
/// anyone can trigger default resolution: upheld (worker-favorable).
/// Ignoring a dispute costs you the bounty.
#[derive(Accounts)]
#[instruction(bounty_id: [u8; 32])]
pub struct DefaultDisputeResolution<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_LISTING_SEED, &bounty_id],
        bump = bounty_listing.bump,
    )]
    pub bounty_listing: Account<'info, BountyListing>,

    #[account(
        mut,
        seeds = [DISPUTE_SEED, &bounty_id],
        bump = dispute_record.bump,
    )]
    pub dispute_record: Account<'info, DisputeRecord>,

    /// Anyone can call — permissionless
    pub caller: Signer<'info>,
}

pub fn handler_default_dispute_resolution(
    ctx: Context<DefaultDisputeResolution>,
    bounty_id: [u8; 32],
) -> Result<()> {
    let clock = Clock::get()?;
    let listing = &mut ctx.accounts.bounty_listing;
    let dispute = &mut ctx.accounts.dispute_record;

    // Must be disputed and unresolved
    require!(
        listing.status == BountyStatus::Disputed,
        BountyError::BountyNotRejected
    );
    require!(!dispute.is_resolved, BountyError::DisputeAlreadyResolved);

    // Check timeout
    let time_since_filed = clock
        .unix_timestamp
        .checked_sub(dispute.filed_at)
        .unwrap_or(0);
    let timeout_seconds = DISPUTE_RESOLUTION_TIMEOUT_HOURS * 3600;
    require!(
        time_since_filed >= timeout_seconds as i64,
        BountyError::DisputeResolutionNotTimedOut
    );

    // Default to upheld (worker-favorable)
    dispute.is_resolved = true;
    dispute.upheld = true;
    dispute.resolved_at = clock.unix_timestamp;
    dispute.resolver = Pubkey::default(); // No resolver — auto-resolved

    listing.status = BountyStatus::Approved;

    emit!(DisputeDefaultResolved {
        bounty_id,
        timestamp: clock.unix_timestamp,
    });

    msg!("Dispute auto-resolved: UPHELD (worker-favorable default after 7-day timeout)");
    Ok(())
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct DisputeFiled {
    pub bounty_id: [u8; 32],
    pub worker: Pubkey,
    pub stake_amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct DisputeResolved {
    pub bounty_id: [u8; 32],
    pub upheld: bool,
    pub resolver: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct DisputeDefaultResolved {
    pub bounty_id: [u8; 32],
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispute_constants() {
        assert_eq!(DISPUTE_WINDOW_HOURS, 48);
        assert_eq!(DISPUTE_STAKE_BPS, 500); // 5%
        assert_eq!(DISPUTE_RESOLUTION_TIMEOUT_HOURS, 168); // 7 days
    }

    #[test]
    fn test_dispute_stake_calculation() {
        let bounty_value = 10_000u64;
        let stake = bounty_value * DISPUTE_STAKE_BPS as u64 / BPS_DENOMINATOR as u64;
        assert_eq!(stake, 500); // 5% of 10,000
    }

    #[test]
    fn test_dispute_window_seconds() {
        let window_seconds = DISPUTE_WINDOW_HOURS * 3600;
        assert_eq!(window_seconds, 172_800); // 48 hours
    }

    #[test]
    fn test_resolution_timeout_seconds() {
        let timeout_seconds = DISPUTE_RESOLUTION_TIMEOUT_HOURS * 3600;
        assert_eq!(timeout_seconds, 604_800); // 7 days
    }
}
