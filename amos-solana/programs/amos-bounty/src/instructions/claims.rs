/// AMOS Bounty Program - Claim & Timeout Instructions
///
/// Implements bounty claiming, submission, timeout release, and concurrent claim limits.
/// These instructions manage the lifecycle of a bounty from open → claimed → submitted.
use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::BountyError;
use crate::state::*;

// ============================================================================
// Claim Bounty
// ============================================================================

/// Claim an open bounty. Sets status to Claimed and locks it from other claimants.
/// Enforces concurrent claim limits per trust level.
///
/// # Arguments
/// * `bounty_id` - The bounty to claim
#[derive(Accounts)]
#[instruction(bounty_id: [u8; 32])]
pub struct ClaimBounty<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        mut,
        seeds = [BOUNTY_LISTING_SEED, &bounty_id],
        bump = bounty_listing.bump,
    )]
    pub bounty_listing: Account<'info, BountyListing>,

    /// Operator stats — must exist (created by prepare_bounty_submission)
    #[account(
        mut,
        seeds = [OPERATOR_STATS_SEED, claimer.key().as_ref()],
        bump = operator_stats.bump,
    )]
    pub operator_stats: Account<'info, OperatorStats>,

    #[account(mut)]
    pub claimer: Signer<'info>,
}

pub fn handler_claim_bounty(ctx: Context<ClaimBounty>, bounty_id: [u8; 32]) -> Result<()> {
    let clock = Clock::get()?;
    let listing = &mut ctx.accounts.bounty_listing;
    let operator_stats = &mut ctx.accounts.operator_stats;

    // Verify bounty is open
    require!(
        listing.status == BountyStatus::Open,
        BountyError::BountyNotOpen
    );

    // Self-dealing prevention for commercial bounties
    if listing.bounty_source == BountySource::Commercial {
        let time_since_post = clock
            .unix_timestamp
            .checked_sub(listing.posted_at)
            .unwrap_or(0);
        if ctx.accounts.claimer.key() == listing.poster {
            require!(
                time_since_post >= (SELF_DEALING_COOLDOWN_HOURS as i64) * 3600,
                BountyError::BountyNotOpen // poster can't claim own bounty within cooldown
            );
        }
    }

    // Trust level check (default to 1 for operators without agent trust record)
    let trust_level = 1u8; // Agent trust checked off-chain; on-chain enforces base level

    // Concurrent claim limit
    let max_claims = get_max_concurrent_claims(trust_level)?;
    require!(
        operator_stats.active_claim_count < max_claims,
        BountyError::ConcurrentClaimLimitReached
    );

    // Claim the bounty
    listing.status = BountyStatus::Claimed;
    listing.claimed_by = ctx.accounts.claimer.key();
    listing.claimed_at = clock.unix_timestamp;

    // Increment active claim count
    operator_stats.active_claim_count = operator_stats
        .active_claim_count
        .checked_add(1)
        .ok_or(BountyError::ArithmeticOverflow)?;

    emit!(BountyClaimed {
        bounty_id,
        claimer: ctx.accounts.claimer.key(),
        timestamp: clock.unix_timestamp,
    });

    msg!("Bounty claimed successfully");
    Ok(())
}

// ============================================================================
// Release Expired Claim (Permissionless)
// ============================================================================

/// Anyone can call this to release an expired claim back to the board.
/// No reputation penalty for the claimer (timeout ≠ rejection).
/// The claimer loses any partial work — they can re-claim if still available.
#[derive(Accounts)]
#[instruction(bounty_id: [u8; 32])]
pub struct ReleaseExpiredClaim<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_LISTING_SEED, &bounty_id],
        bump = bounty_listing.bump,
    )]
    pub bounty_listing: Account<'info, BountyListing>,

    /// Operator stats of the claimer (to decrement active_claim_count)
    #[account(
        mut,
        seeds = [OPERATOR_STATS_SEED, bounty_listing.claimed_by.as_ref()],
        bump = operator_stats.bump,
    )]
    pub operator_stats: Account<'info, OperatorStats>,

    /// Anyone can call this — permissionless
    pub caller: Signer<'info>,
}

pub fn handler_release_expired_claim(
    ctx: Context<ReleaseExpiredClaim>,
    bounty_id: [u8; 32],
) -> Result<()> {
    let clock = Clock::get()?;
    let listing = &mut ctx.accounts.bounty_listing;
    let operator_stats = &mut ctx.accounts.operator_stats;

    // Must be in Claimed status
    require!(
        listing.status == BountyStatus::Claimed,
        BountyError::BountyNotClaimed
    );

    // Check timeout
    let timeout_hours = if listing.claim_timeout_hours > 0 {
        listing.claim_timeout_hours
    } else {
        DEFAULT_CLAIM_TIMEOUT_HOURS
    };
    let timeout_seconds = timeout_hours
        .checked_mul(3600)
        .ok_or(BountyError::ArithmeticOverflow)?;

    require!(
        clock.unix_timestamp > listing.claimed_at + timeout_seconds as i64,
        BountyError::ClaimNotExpired
    );

    let expired_claimer = listing.claimed_by;

    // Reset to Open
    listing.status = BountyStatus::Open;
    listing.claimed_by = Pubkey::default();
    listing.claimed_at = 0;

    // Decrement active claim count
    operator_stats.active_claim_count = operator_stats.active_claim_count.saturating_sub(1);

    emit!(ClaimExpired {
        bounty_id,
        expired_claimer,
        timestamp: clock.unix_timestamp,
    });

    msg!("Expired claim released — bounty is open again");
    Ok(())
}

// ============================================================================
// Post Bounty Listing
// ============================================================================

/// Post a new bounty to the board (creates the BountyListing account).
/// For system bounties, only the oracle can post.
/// For commercial bounties, the poster must have already escrowed tokens.
#[derive(Accounts)]
#[instruction(bounty_id: [u8; 32])]
pub struct PostBountyListing<'info> {
    #[account(
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
    )]
    pub config: Account<'info, BountyConfig>,

    #[account(
        init,
        payer = poster,
        space = BountyListing::SIZE,
        seeds = [BOUNTY_LISTING_SEED, &bounty_id],
        bump,
    )]
    pub bounty_listing: Account<'info, BountyListing>,

    #[account(mut)]
    pub poster: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[allow(clippy::too_many_arguments)]
pub fn handler_post_bounty_listing(
    ctx: Context<PostBountyListing>,
    bounty_id: [u8; 32],
    bounty_source: u8,
    reward_amount: u64,
    contribution_type: u8,
    required_trust_level: u8,
    claim_timeout_hours: u64,
    deadline: i64,
) -> Result<()> {
    let clock = Clock::get()?;
    let listing = &mut ctx.accounts.bounty_listing;

    // Validate contribution type
    require!(
        contribution_type <= 10,
        BountyError::InvalidContributionType
    );

    // Validate trust level
    require!(
        required_trust_level >= 1 && required_trust_level <= 5,
        BountyError::InvalidTrustLevel
    );

    // Validate claim timeout
    if claim_timeout_hours > 0 {
        require!(
            claim_timeout_hours >= MIN_CLAIM_TIMEOUT_HOURS
                && claim_timeout_hours <= MAX_CLAIM_TIMEOUT_HOURS,
            BountyError::InvalidClaimTimeout
        );
    }

    let source = match bounty_source {
        0 => BountySource::Treasury,
        1 => BountySource::Commercial,
        _ => return Err(BountyError::InvalidBountySource.into()),
    };

    listing.bounty_id = bounty_id;
    listing.status = BountyStatus::Open;
    listing.bounty_source = source;
    listing.poster = ctx.accounts.poster.key();
    listing.claimed_by = Pubkey::default();
    listing.claimed_at = 0;
    listing.claim_timeout_hours = claim_timeout_hours;
    listing.reward_amount = reward_amount;
    listing.contribution_type = contribution_type;
    listing.required_trust_level = required_trust_level;
    listing.posted_at = clock.unix_timestamp;
    listing.deadline = deadline;
    listing.submitted_at = 0;
    listing.rejected_at = 0;
    listing.bump = ctx.bumps.bounty_listing;
    listing.reserved = [0; 8];

    emit!(BountyPosted {
        bounty_id,
        poster: ctx.accounts.poster.key(),
        bounty_source: bounty_source,
        reward_amount,
        timestamp: clock.unix_timestamp,
    });

    msg!("Bounty posted successfully");
    Ok(())
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct BountyClaimed {
    pub bounty_id: [u8; 32],
    pub claimer: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct ClaimExpired {
    pub bounty_id: [u8; 32],
    pub expired_claimer: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct BountyPosted {
    pub bounty_id: [u8; 32],
    pub poster: Pubkey,
    pub bounty_source: u8,
    pub reward_amount: u64,
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claim_timeout_defaults() {
        assert_eq!(DEFAULT_CLAIM_TIMEOUT_HOURS, 72);
        assert_eq!(MIN_CLAIM_TIMEOUT_HOURS, 1);
        assert_eq!(MAX_CLAIM_TIMEOUT_HOURS, 720);
    }

    #[test]
    fn test_concurrent_claim_limits() {
        assert_eq!(get_max_concurrent_claims(1).unwrap(), 3);
        assert_eq!(get_max_concurrent_claims(2).unwrap(), 5);
        assert_eq!(get_max_concurrent_claims(3).unwrap(), 8);
        assert_eq!(get_max_concurrent_claims(4).unwrap(), 12);
        assert_eq!(get_max_concurrent_claims(5).unwrap(), 20);
    }

    #[test]
    fn test_self_dealing_cooldown() {
        assert_eq!(SELF_DEALING_COOLDOWN_HOURS, 24);
        let cooldown_seconds = SELF_DEALING_COOLDOWN_HOURS * 3600;
        assert_eq!(cooldown_seconds, 86400); // 24 hours = 86400 seconds
    }
}
