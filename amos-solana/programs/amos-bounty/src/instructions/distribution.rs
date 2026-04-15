/// AMOS Bounty Program - Distribution Instructions
///
/// This module handles the core bounty submission and token distribution logic.
/// It implements trustless, transparent token allocation based on contribution value.
///
/// IMPORTANT: Call `prepare_bounty_submission` in the same transaction BEFORE
/// this instruction to ensure daily_pool and operator_stats accounts exist.
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use crate::constants::*;
use crate::errors::BountyError;
use crate::state::*;

// ============================================================================
// Submit Bounty Proof
// ============================================================================

/// Submit a validated bounty proof and distribute tokens proportionally.
///
/// This is the CORE distribution mechanism. Token allocation is calculated as:
/// `tokens = (adjusted_points / total_points_today) × remaining_daily_emission`
///
/// Prerequisites: `prepare_bounty_submission` must be called first in the same
/// transaction to create daily_pool and operator_stats if they don't exist.
///
/// # Arguments
/// * `bounty_id` - Unique identifier for this bounty (32 bytes)
/// * `base_points` - Base point value before multipliers (1-2000)
/// * `quality_score` - Quality assessment (30-100)
/// * `contribution_type` - Type of work (0-7)
/// * `is_agent` - Whether this is an AI agent submission
/// * `agent_id` - Agent identifier if applicable
/// * `day_index` - Current day index since program start
/// * `max_reward` - Maximum token payout for this bounty (in lamports, 0 = no cap)
/// * `reviewer` - Address of the reviewer who validated this work
/// * `evidence_hash` - Hash of the work product/evidence
/// * `external_reference` - External ID (issue number, PR number, etc.)
#[derive(Accounts)]
#[instruction(bounty_id: [u8; 32], base_points: u16, quality_score: u8, contribution_type: u8, is_agent: bool, agent_id: [u8; 32], day_index: u32, max_reward: u64)]
pub struct SubmitBountyProof<'info> {
    #[account(
        mut,
        seeds = [BOUNTY_CONFIG_SEED],
        bump = config.bump,
        has_one = oracle_authority @ BountyError::Unauthorized,
        has_one = mint @ BountyError::InvalidMint,
        has_one = treasury @ BountyError::InvalidTreasury
    )]
    pub config: Box<Account<'info, BountyConfig>>,

    /// Daily pool — must already exist (created by prepare_bounty_submission)
    #[account(
        mut,
        seeds = [DAILY_POOL_SEED, &day_index.to_le_bytes()],
        bump = daily_pool.bump
    )]
    pub daily_pool: Box<Account<'info, DailyPool>>,

    #[account(
        init,
        payer = oracle_authority,
        space = BountyProof::SIZE,
        seeds = [BOUNTY_PROOF_SEED, &bounty_id],
        bump
    )]
    pub bounty_proof: Box<Account<'info, BountyProof>>,

    /// Operator stats — must already exist (created by prepare_bounty_submission)
    #[account(
        mut,
        seeds = [OPERATOR_STATS_SEED, operator.key().as_ref()],
        bump = operator_stats.bump
    )]
    pub operator_stats: Box<Account<'info, OperatorStats>>,

    /// The operator earning this bounty
    /// CHECK: This is validated through the operator_stats PDA derivation
    pub operator: AccountInfo<'info>,

    /// Optional agent trust record (required if is_agent = true)
    #[account(
        mut,
        seeds = [AGENT_TRUST_SEED, &agent_id],
        bump = agent_trust.bump
    )]
    pub agent_trust: Option<Account<'info, AgentTrustRecord>>,

    pub mint: Box<Account<'info, Mint>>,

    #[account(mut)]
    pub treasury: Box<Account<'info, TokenAccount>>,

    /// Operator's token account (receives bounty tokens)
    #[account(
        mut,
        constraint = operator_token_account.mint == mint.key() @ BountyError::InvalidMint,
        constraint = operator_token_account.owner == operator.key() @ BountyError::InvalidOperator
    )]
    pub operator_token_account: Box<Account<'info, TokenAccount>>,

    /// Reviewer's token account (receives 5% reward)
    #[account(
        mut,
        constraint = reviewer_token_account.mint == mint.key() @ BountyError::InvalidMint
    )]
    pub reviewer_token_account: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub oracle_authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handler_submit_proof(
    ctx: Context<SubmitBountyProof>,
    bounty_id: [u8; 32],
    base_points: u16,
    quality_score: u8,
    contribution_type: u8,
    is_agent: bool,
    agent_id: [u8; 32],
    day_index: u32,
    max_reward: u64,
    reviewer: Pubkey,
    evidence_hash: [u8; 32],
    external_reference: [u8; 64],
) -> Result<()> {
    let clock = Clock::get()?;
    let config = &mut ctx.accounts.config;
    let daily_pool = &mut ctx.accounts.daily_pool;
    let bounty_proof = &mut ctx.accounts.bounty_proof;
    let operator_stats = &mut ctx.accounts.operator_stats;

    // ========================================================================
    // Validation Phase
    // ========================================================================

    require!(
        quality_score >= MIN_QUALITY_SCORE,
        BountyError::QualityScoreTooLow
    );
    require!(
        contribution_type <= 10,
        BountyError::InvalidContributionType
    );
    require!(
        base_points > 0 && base_points <= MAX_BOUNTY_POINTS,
        BountyError::InvalidBountyPoints
    );
    require!(
        reviewer != ctx.accounts.operator.key(),
        BountyError::ReviewerSameAsOperator
    );
    require!(evidence_hash != [0u8; 32], BountyError::InvalidEvidenceHash);

    // Verify operator_stats was properly initialized by prepare instruction
    require!(
        operator_stats.operator == ctx.accounts.operator.key(),
        BountyError::InvalidOperator
    );

    // Validate day_index matches current day
    let current_day = calculate_day_index(config.start_time)?;
    require!(day_index == current_day, BountyError::InvalidDayIndex);

    // Reset daily counter if new day
    if operator_stats.last_submission_day != current_day {
        operator_stats.daily_bounty_count = 0;
        operator_stats.last_submission_day = current_day;
    }

    // Verify pool is not finalized
    require!(
        !daily_pool.finalized,
        BountyError::DailyPoolAlreadyFinalized
    );

    // ========================================================================
    // Trust Level Enforcement (for AI agents)
    // ========================================================================

    let mut trust_level: u8 = 1; // Default for human operators

    if is_agent {
        let agent_trust = ctx
            .accounts
            .agent_trust
            .as_ref()
            .ok_or(BountyError::AgentNotRegistered)?;

        trust_level = agent_trust.trust_level;

        let max_points = get_max_points_for_trust_level(trust_level)?;
        require!(base_points <= max_points, BountyError::InvalidBountyPoints);

        // No daily bounty count limit — the finite daily emission pool is the
        // natural throttle. More bounties just means smaller per-bounty shares.
    } else {
        // Non-agent submissions: no daily limit either
    }

    // ========================================================================
    // Apply Contribution Type Multiplier
    // ========================================================================

    let multiplier_bps = get_contribution_multiplier(contribution_type)?;

    let adjusted_points = (base_points as u64)
        .checked_mul(multiplier_bps as u64)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(BountyError::ArithmeticOverflow)? as u16;

    let adjusted_points = adjusted_points.min(MAX_BOUNTY_POINTS);

    // Ensure rounding didn't produce zero points
    require!(adjusted_points > 0, BountyError::ZeroPointsAwarded);

    // ========================================================================
    // Emission Pool Separation — Prevents growth floods from diluting technical work
    // ========================================================================

    let remaining_emission = daily_pool
        .daily_emission
        .checked_sub(daily_pool.tokens_distributed)
        .ok_or(BountyError::InsufficientEmission)?;

    require!(remaining_emission > 0, BountyError::InsufficientEmission);

    // Determine if this bounty is growth or technical
    let is_growth = is_growth_contribution(contribution_type);

    // Calculate pool-aware token allocation
    // Growth pool is capped at growth_cap_bps of daily emission; technical gets the rest
    let growth_cap_bps = current_growth_cap_bps(clock.unix_timestamp, config.start_time);
    let growth_pool_max = daily_pool
        .daily_emission
        .checked_mul(growth_cap_bps)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    let tokens_before_split = if is_growth {
        // Growth bounty: distribute from growth pool (capped)
        let growth_remaining = growth_pool_max.saturating_sub(daily_pool.growth_tokens_distributed);

        if growth_remaining == 0 {
            // Growth pool exhausted for today — still record points but award minimum
            1u64
        } else {
            let new_growth_points = daily_pool
                .growth_points
                .checked_add(adjusted_points as u64)
                .ok_or(BountyError::ArithmeticOverflow)?;

            let tokens = (adjusted_points as u64)
                .checked_mul(growth_remaining)
                .ok_or(BountyError::ArithmeticOverflow)?
                .checked_div(new_growth_points)
                .ok_or(BountyError::ArithmeticOverflow)?;
            tokens.max(1).min(growth_remaining)
        }
    } else {
        // Technical bounty: distribute from technical pool (protected)
        let technical_pool_max = daily_pool.daily_emission.saturating_sub(growth_pool_max);
        let technical_remaining =
            technical_pool_max.saturating_sub(daily_pool.technical_tokens_distributed);

        // Technical pool also gets any unused growth allocation
        let unused_growth = growth_pool_max.saturating_sub(daily_pool.growth_tokens_distributed);
        let effective_remaining = technical_remaining
            .checked_add(unused_growth)
            .ok_or(BountyError::ArithmeticOverflow)?
            .min(remaining_emission);

        let new_technical_points = daily_pool
            .technical_points
            .checked_add(adjusted_points as u64)
            .ok_or(BountyError::ArithmeticOverflow)?;

        let tokens = (adjusted_points as u64)
            .checked_mul(effective_remaining)
            .ok_or(BountyError::ArithmeticOverflow)?
            .checked_div(new_technical_points)
            .ok_or(BountyError::ArithmeticOverflow)?;
        tokens.max(1)
    };

    // Cap payout at the bounty's stated reward (max_reward, in lamports).
    // The pool share can exceed the bounty value when few contributors are active.
    // Unclaimed tokens stay in the pool for later bounties that day.
    let tokens_before_split = if max_reward > 0 {
        tokens_before_split.min(max_reward)
    } else {
        tokens_before_split
    };

    let new_total_points = daily_pool
        .total_points
        .checked_add(adjusted_points as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    // Split tokens: 95% to operator, 5% to reviewer
    let reviewer_tokens = tokens_before_split
        .checked_mul(REVIEWER_REWARD_BPS as u64)
        .ok_or(BountyError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    let operator_tokens = tokens_before_split
        .checked_sub(reviewer_tokens)
        .ok_or(BountyError::ArithmeticUnderflow)?;

    require!(operator_tokens > 0, BountyError::ZeroTokensCalculated);

    // ========================================================================
    // Transfer Tokens
    // ========================================================================

    let config_seeds = &[BOUNTY_CONFIG_SEED, &[config.bump]];
    let signer_seeds = &[&config_seeds[..]];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.treasury.to_account_info(),
                to: ctx.accounts.operator_token_account.to_account_info(),
                authority: config.to_account_info(),
            },
            signer_seeds,
        ),
        operator_tokens,
    )?;

    if reviewer_tokens > 0 {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.treasury.to_account_info(),
                    to: ctx.accounts.reviewer_token_account.to_account_info(),
                    authority: config.to_account_info(),
                },
                signer_seeds,
            ),
            reviewer_tokens,
        )?;
    }

    // ========================================================================
    // Update State
    // ========================================================================

    daily_pool.tokens_distributed = daily_pool
        .tokens_distributed
        .checked_add(tokens_before_split)
        .ok_or(BountyError::ArithmeticOverflow)?;

    daily_pool.total_points = new_total_points;

    // Update pool-specific tracking
    if is_growth {
        daily_pool.growth_tokens_distributed = daily_pool
            .growth_tokens_distributed
            .checked_add(tokens_before_split)
            .ok_or(BountyError::ArithmeticOverflow)?;
        daily_pool.growth_points = daily_pool
            .growth_points
            .checked_add(adjusted_points as u64)
            .ok_or(BountyError::ArithmeticOverflow)?;
    } else {
        daily_pool.technical_tokens_distributed = daily_pool
            .technical_tokens_distributed
            .checked_add(tokens_before_split)
            .ok_or(BountyError::ArithmeticOverflow)?;
        daily_pool.technical_points = daily_pool
            .technical_points
            .checked_add(adjusted_points as u64)
            .ok_or(BountyError::ArithmeticOverflow)?;
    }

    daily_pool.proof_count = daily_pool
        .proof_count
        .checked_add(1)
        .ok_or(BountyError::ArithmeticOverflow)?;

    operator_stats.total_bounties = operator_stats
        .total_bounties
        .checked_add(1)
        .ok_or(BountyError::ArithmeticOverflow)?;

    operator_stats.total_points = operator_stats
        .total_points
        .checked_add(adjusted_points as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    operator_stats.total_tokens_earned = operator_stats
        .total_tokens_earned
        .checked_add(operator_tokens)
        .ok_or(BountyError::ArithmeticOverflow)?;

    operator_stats.decayable_balance = operator_stats
        .decayable_balance
        .checked_add(operator_tokens)
        .ok_or(BountyError::ArithmeticOverflow)?;

    operator_stats.original_allocation = operator_stats
        .original_allocation
        .checked_add(operator_tokens)
        .ok_or(BountyError::ArithmeticOverflow)?;

    operator_stats.daily_bounty_count = operator_stats
        .daily_bounty_count
        .checked_add(1)
        .ok_or(BountyError::ArithmeticOverflow)?;

    operator_stats.last_activity_time = clock.unix_timestamp;

    config.total_tokens_distributed = config
        .total_tokens_distributed
        .checked_add(tokens_before_split)
        .ok_or(BountyError::ArithmeticOverflow)?;

    config.total_bounties = config
        .total_bounties
        .checked_add(1)
        .ok_or(BountyError::ArithmeticOverflow)?;

    config.total_points = config
        .total_points
        .checked_add(adjusted_points as u64)
        .ok_or(BountyError::ArithmeticOverflow)?;

    // Record bounty proof (immutable record)
    bounty_proof.bounty_id = bounty_id;
    bounty_proof.bounty_source = BountySource::Treasury;
    bounty_proof.operator = ctx.accounts.operator.key();
    bounty_proof.funded_by = ctx.accounts.treasury.key();
    bounty_proof.escrow_account = Pubkey::default();
    bounty_proof.base_points = base_points;
    bounty_proof.adjusted_points = adjusted_points;
    bounty_proof.quality_score = quality_score;
    bounty_proof.contribution_type = contribution_type;
    bounty_proof.is_agent = is_agent;
    bounty_proof.agent_id = agent_id;
    bounty_proof.trust_level = trust_level;
    bounty_proof.tokens_earned = operator_tokens;
    bounty_proof.fee_collected = 0;
    bounty_proof.reviewer = reviewer;
    bounty_proof.reviewer_tokens = reviewer_tokens;
    bounty_proof.evidence_hash = evidence_hash;
    bounty_proof.timestamp = clock.unix_timestamp;
    bounty_proof.day_index = current_day;
    bounty_proof.external_reference = external_reference;
    bounty_proof.bump = ctx.bumps.bounty_proof;
    bounty_proof.reserved = [0; 8];

    // Update agent trust record if applicable
    if is_agent {
        if let Some(agent_trust) = ctx.accounts.agent_trust.as_mut() {
            agent_trust.total_tokens_earned = agent_trust
                .total_tokens_earned
                .checked_add(operator_tokens)
                .ok_or(BountyError::ArithmeticOverflow)?;

            agent_trust.total_points_earned = agent_trust
                .total_points_earned
                .checked_add(adjusted_points as u64)
                .ok_or(BountyError::ArithmeticOverflow)?;

            agent_trust.last_activity = clock.unix_timestamp;
        }
    }

    // ========================================================================
    // Emit Event
    // ========================================================================

    emit!(BountySubmitted {
        bounty_id,
        bounty_source: 0, // Treasury
        operator: ctx.accounts.operator.key(),
        base_points,
        adjusted_points,
        operator_tokens,
        reviewer_tokens,
        fee_collected: 0,
        day_index: current_day,
        timestamp: clock.unix_timestamp,
    });

    msg!("Bounty submitted successfully");
    msg!(
        "Base points: {}, Adjusted points: {}",
        base_points,
        adjusted_points
    );
    msg!(
        "Operator tokens: {}, Reviewer tokens: {}",
        operator_tokens,
        reviewer_tokens
    );

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

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

/// Determine the current growth pool cap using sigmoid decay.
///
/// growth_cap(t) = floor + (ceiling - floor) / (1 + e^(k × (t - midpoint)))
///
/// Uses the default constants. When ContributionTypeRegistry is available,
/// the registry's stored parameters are used instead (see registry.rs).
fn current_growth_cap_bps(now: i64, launch_time: i64) -> u64 {
    let elapsed_seconds = now.saturating_sub(launch_time).max(0) as u64;
    let elapsed_days = elapsed_seconds / 86400;
    sigmoid_growth_cap_bps(elapsed_days) as u64
}

// ============================================================================
// Events
// ============================================================================

#[event]
pub struct BountySubmitted {
    pub bounty_id: [u8; 32],
    pub bounty_source: u8, // 0 = Treasury, 1 = Commercial
    pub operator: Pubkey,
    pub base_points: u16,
    pub adjusted_points: u16,
    pub operator_tokens: u64,
    pub reviewer_tokens: u64,
    pub fee_collected: u64,
    pub day_index: u32,
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contribution_multipliers() {
        assert_eq!(get_contribution_multiplier(0).unwrap(), 12000);
        assert_eq!(get_contribution_multiplier(1).unwrap(), 10000);
        assert_eq!(get_contribution_multiplier(2).unwrap(), 8000);
        assert_eq!(get_contribution_multiplier(7).unwrap(), 13000);
    }

    #[test]
    fn test_reviewer_split() {
        let total_tokens = 10000u64;
        let reviewer_portion = total_tokens * REVIEWER_REWARD_BPS as u64 / BPS_DENOMINATOR as u64;
        let operator_portion = total_tokens - reviewer_portion;

        assert_eq!(reviewer_portion, 500);
        assert_eq!(operator_portion, 9500);
    }

    #[test]
    fn test_proportional_distribution() {
        let remaining_emission = 1000u64;
        let adjusted_points = 100u64;
        let existing_points = 900u64;
        let new_total = existing_points + adjusted_points;

        let tokens = (adjusted_points * remaining_emission) / new_total;
        assert_eq!(tokens, 100);
    }

    #[test]
    fn test_sigmoid_growth_cap_in_distribution() {
        let launch = 1000i64;

        // At launch: near ceiling (20%)
        let cap_launch = current_growth_cap_bps(launch + 1, launch);
        assert!(
            cap_launch >= 1950,
            "Launch cap {} should be near 2000",
            cap_launch
        );

        // At midpoint (~18 months = 540 days = 46656000 seconds): ~1150 bps
        let cap_mid = current_growth_cap_bps(launch + 540 * 86400, launch);
        assert!(
            cap_mid >= 1000 && cap_mid <= 1300,
            "Midpoint cap {} should be ~1150",
            cap_mid
        );

        // At maturity (5 years): near floor (3%)
        let cap_mature = current_growth_cap_bps(launch + 1800 * 86400, launch);
        assert!(
            cap_mature <= 350,
            "Mature cap {} should be near 300",
            cap_mature
        );

        // Monotonically decreasing
        assert!(cap_launch > cap_mid);
        assert!(cap_mid > cap_mature);
    }

    #[test]
    fn test_growth_pool_cap_enforcement() {
        // With 16000 whole AMOS daily emission and ~10% sigmoid growth cap (near midpoint):
        let daily_emission = 16_000 * ONE_TOKEN;
        let growth_cap_bps = 1000u64; // 10%
        let growth_pool_max = daily_emission * growth_cap_bps / BPS_DENOMINATOR as u64;
        assert_eq!(growth_pool_max, 1_600 * ONE_TOKEN);

        // Technical pool gets the rest
        let technical_pool = daily_emission - growth_pool_max;
        assert_eq!(technical_pool, 14_400 * ONE_TOKEN);
    }
}
