// AMOS Governance Program - Priority Calculation Instructions
// Handles MRR-weighted priority scoring with recency decay

use crate::constants::*;
use crate::errors::GovernanceError;
use crate::state::*;
use anchor_lang::prelude::*;

// ============================================================================
// Calculate Priority Score
// ============================================================================

/// Calculates the priority score for a feature proposal
/// Priority = (MRR_weight × customer_votes) + (community_votes × community_weight) + recency_bonus
/// Recency bonus uses exponential decay with configurable half-life
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct CalculatePriority<'info> {
    #[account(
        seeds = [GOVERNANCE_SEED],
        bump = governance_config.bump
    )]
    pub governance_config: Account<'info, GovernanceConfig>,

    #[account(
        constraint = governance_config.params == governance_params.key() @ GovernanceError::InvalidAccount
    )]
    pub governance_params: Account<'info, StoredGovernanceParams>,

    #[account(
        seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = feature_proposal.bump
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,
}

/// Priority calculation result
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct PriorityScore {
    /// Total priority score
    pub total_score: u64,

    /// MRR-weighted customer vote component
    pub customer_vote_score: u64,

    /// Community vote component
    pub community_vote_score: u64,

    /// Recency bonus component
    pub recency_bonus: u64,

    /// Age of proposal in days
    pub age_days: u64,
}

pub fn calculate_priority(
    ctx: Context<CalculatePriority>,
    proposal_id: u64,
    customer_vote_amount: u64, // MRR-weighted votes from customer requests
) -> Result<PriorityScore> {
    let proposal = &ctx.accounts.feature_proposal;
    let params = &ctx.accounts.governance_params;
    let clock = Clock::get()?;

    // Calculate proposal age in days
    let age_seconds = clock
        .unix_timestamp
        .checked_sub(proposal.created_at)
        .ok_or(GovernanceError::InvalidTimestamp)?;

    let age_days = age_seconds
        .checked_div(24 * 60 * 60)
        .ok_or(GovernanceError::DivisionByZero)? as u64;

    // Calculate customer vote component (MRR-weighted)
    // Score = customer_votes × (mrr_weight / BPS_DENOMINATOR)
    let customer_score = (customer_vote_amount as u128)
        .checked_mul(params.mrr_weight_bps as u128)
        .ok_or(GovernanceError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u128)
        .ok_or(GovernanceError::DivisionByZero)?;

    let customer_score =
        u64::try_from(customer_score).map_err(|_| GovernanceError::ArithmeticOverflow)?;

    // Calculate community vote component
    // Score = total_votes × (community_weight / BPS_DENOMINATOR)
    let community_score = (proposal.total_votes as u128)
        .checked_mul(params.community_weight_bps as u128)
        .ok_or(GovernanceError::ArithmeticOverflow)?
        .checked_div(BPS_DENOMINATOR as u128)
        .ok_or(GovernanceError::DivisionByZero)?;

    let community_score =
        u64::try_from(community_score).map_err(|_| GovernanceError::ArithmeticOverflow)?;

    // Calculate recency bonus with exponential decay
    // Formula: initial_score × (0.5 ^ (age_days / halflife_days))
    // Approximation: initial_score × max(0, (1 - age_days / (2 × halflife_days)))
    // For simplicity, using linear decay over 2× halflife period
    let halflife_days = params.recency_halflife_days as u64;
    let decay_period = halflife_days
        .checked_mul(2)
        .ok_or(GovernanceError::ArithmeticOverflow)?;

    let recency_bonus = if age_days >= decay_period {
        0
    } else {
        let remaining = decay_period
            .checked_sub(age_days)
            .ok_or(GovernanceError::ArithmeticUnderflow)?;

        // Base recency bonus = 1000 points, decays to 0 over decay_period
        let base_bonus = 1000u128;
        let bonus = base_bonus
            .checked_mul(remaining as u128)
            .ok_or(GovernanceError::ArithmeticOverflow)?
            .checked_div(decay_period as u128)
            .ok_or(GovernanceError::DivisionByZero)?;

        u64::try_from(bonus).map_err(|_| GovernanceError::ArithmeticOverflow)?
    };

    // Calculate total priority score
    let total_score = customer_score
        .checked_add(community_score)
        .ok_or(GovernanceError::ArithmeticOverflow)?
        .checked_add(recency_bonus)
        .ok_or(GovernanceError::ArithmeticOverflow)?;

    let result = PriorityScore {
        total_score,
        customer_vote_score: customer_score,
        community_vote_score: community_score,
        recency_bonus,
        age_days,
    };

    msg!("Priority calculated for proposal {}", proposal_id);
    msg!("  Customer vote score: {}", result.customer_vote_score);
    msg!("  Community vote score: {}", result.community_vote_score);
    msg!("  Recency bonus: {}", result.recency_bonus);
    msg!("  Total score: {}", result.total_score);
    msg!("  Age: {} days", result.age_days);

    Ok(result)
}

// ============================================================================
// Helper: Calculate Recency Decay Factor
// ============================================================================

/// Calculates the exponential decay factor for recency bonus
/// Returns a value between 0 and BPS_DENOMINATOR (0-100%)
pub fn calculate_decay_factor(age_days: u64, halflife_days: u16) -> Result<u16> {
    // Using approximation: decay_factor ≈ 1 / (1 + age_days / halflife_days)
    // This provides a smooth decay curve

    let halflife = halflife_days as u64;

    if age_days == 0 {
        return Ok(BPS_DENOMINATOR); // 100% - no decay
    }

    // Calculate: BPS_DENOMINATOR / (1 + age_days / halflife_days)
    let denominator = halflife
        .checked_add(age_days)
        .ok_or(GovernanceError::ArithmeticOverflow)?;

    let factor = (BPS_DENOMINATOR as u64)
        .checked_mul(halflife)
        .ok_or(GovernanceError::ArithmeticOverflow)?
        .checked_div(denominator)
        .ok_or(GovernanceError::DivisionByZero)?;

    u16::try_from(factor).map_err(|_| GovernanceError::ArithmeticOverflow.into())
}

// ============================================================================
// Helper: Get Proposal Rank
// ============================================================================

/// Context for comparing proposal priorities
/// Note: This is a view-only operation, actual ranking would be done off-chain
#[derive(Accounts)]
pub struct GetProposalRank<'info> {
    #[account(
        seeds = [GOVERNANCE_SEED],
        bump = governance_config.bump
    )]
    pub governance_config: Account<'info, GovernanceConfig>,
}

/// Returns metadata for ranking proposals
pub fn get_proposal_rank(_ctx: Context<GetProposalRank>) -> Result<u64> {
    // This is a placeholder for off-chain ranking logic
    // In practice, you would query multiple proposals and sort by priority
    // Returning governance total_proposals as a simple metric
    Ok(_ctx.accounts.governance_config.total_proposals)
}
