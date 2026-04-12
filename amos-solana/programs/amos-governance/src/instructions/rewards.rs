// AMOS Governance Program - Reward Instructions
// Handles staged bounty reward distribution

use crate::constants::*;
use crate::errors::GovernanceError;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

// ============================================================================
// Claim Bounty Reward
// ============================================================================

/// Claims a staged bounty reward after passing required gates
#[derive(Accounts)]
#[instruction(proposal_id: u64, gate_type: GateType)]
pub struct ClaimBountyReward<'info> {
    #[account(
        mut,
        seeds = [GOVERNANCE_SEED],
        bump = governance_config.bump
    )]
    pub governance_config: Account<'info, GovernanceConfig>,

    #[account(
        constraint = governance_config.params == governance_params.key() @ GovernanceError::InvalidAccount
    )]
    pub governance_params: Account<'info, StoredGovernanceParams>,

    #[account(
        mut,
        seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = feature_proposal.bump
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,

    /// Treasury token account
    #[account(
        mut,
        constraint = treasury.key() == governance_config.treasury @ GovernanceError::InvalidAccount
    )]
    pub treasury: Account<'info, TokenAccount>,

    /// Proposer's token account
    #[account(
        mut,
        constraint = proposer_token_account.owner == feature_proposal.proposer @ GovernanceError::InvalidAccount,
        constraint = proposer_token_account.mint == governance_config.mint @ GovernanceError::InvalidMint
    )]
    pub proposer_token_account: Account<'info, TokenAccount>,

    #[account(
        constraint = proposer.key() == feature_proposal.proposer @ GovernanceError::NotProposalOwner
    )]
    pub proposer: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn claim_bounty_reward(
    ctx: Context<ClaimBountyReward>,
    proposal_id: u64,
    gate_type: GateType,
) -> Result<()> {
    let proposal = &mut ctx.accounts.feature_proposal;
    let params = &ctx.accounts.governance_params;
    let governance = &mut ctx.accounts.governance_config;

    // Validate proposal is in correct state
    require!(
        proposal.status == ProposalStatus::AwaitingGates
            || proposal.status == ProposalStatus::RewardsDistribution,
        GovernanceError::InvalidProposalStatus
    );

    // Copy estimated_bounty before the mutable borrow in the match below
    let estimated_bounty = proposal.estimated_bounty;

    // Get the appropriate gate result and reward percentage
    let (gate_result, reward_bps) = match gate_type {
        GateType::Benchmark => {
            require!(
                proposal.completed_at.is_some(),
                GovernanceError::RequiredGatesNotPassed
            );
            (
                proposal
                    .benchmark_result
                    .as_mut()
                    .ok_or(GovernanceError::GateNotEvaluated)?,
                params.bounty_completion_bps,
            )
        }
        GateType::ABTest => {
            // A/B test gate requires benchmark to pass first
            require!(
                proposal
                    .benchmark_result
                    .as_ref()
                    .map(|r| r.passed)
                    .unwrap_or(false),
                GovernanceError::RequiredGatesNotPassed
            );
            (
                proposal
                    .ab_test_result
                    .as_mut()
                    .ok_or(GovernanceError::GateNotEvaluated)?,
                params.bounty_ab_bps,
            )
        }
        GateType::Feedback => {
            // Cannot claim merge reward separately - use finalize_rewards
            return Err(GovernanceError::InvalidGateType.into());
        }
        GateType::StewardApproval => {
            // Steward approval doesn't have its own reward
            return Err(GovernanceError::InvalidGateType.into());
        }
    };

    // Check if gate passed
    require!(
        gate_result.passed,
        GovernanceError::CannotClaimRewardGateNotPassed
    );

    // Check if reward already claimed
    require!(
        !gate_result.reward_claimed,
        GovernanceError::RewardAlreadyClaimed
    );

    // Calculate reward amount
    let reward_amount = (estimated_bounty as u128)
        .checked_mul(reward_bps as u128)
        .ok_or(GovernanceError::RewardCalculationOverflow)?
        .checked_div(BPS_DENOMINATOR as u128)
        .ok_or(GovernanceError::DivisionByZero)?;

    let reward_amount =
        u64::try_from(reward_amount).map_err(|_| GovernanceError::RewardCalculationOverflow)?;

    // Verify treasury has sufficient funds
    require!(
        ctx.accounts.treasury.amount >= reward_amount,
        GovernanceError::InsufficientTreasuryFunds
    );

    // Transfer reward from treasury to proposer
    let governance_seeds = &[GOVERNANCE_SEED, &[governance.bump]];
    let signer_seeds = &[&governance_seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.treasury.to_account_info(),
        to: ctx.accounts.proposer_token_account.to_account_info(),
        authority: governance.to_account_info(),
    };

    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );

    transfer(cpi_ctx, reward_amount)?;

    // Mark reward as claimed
    gate_result.reward_claimed = true;

    // Update governance totals
    governance.total_bounties_paid = governance
        .total_bounties_paid
        .checked_add(reward_amount)
        .ok_or(GovernanceError::ArithmeticOverflow)?;

    // Update proposal status if this was the completion gate
    if matches!(gate_type, GateType::Benchmark) {
        proposal.status = ProposalStatus::RewardsDistribution;
    }

    proposal.updated_at = Clock::get()?.unix_timestamp;

    msg!(
        "Bounty reward claimed for proposal {}: {} ({:?} gate, {}bps)",
        proposal_id,
        reward_amount,
        gate_type,
        reward_bps
    );

    Ok(())
}

// ============================================================================
// Finalize Rewards
// ============================================================================

/// Finalizes remaining rewards after full merge and all gates pass
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct FinalizeRewards<'info> {
    #[account(
        mut,
        seeds = [GOVERNANCE_SEED],
        bump = governance_config.bump
    )]
    pub governance_config: Account<'info, GovernanceConfig>,

    #[account(
        constraint = governance_config.params == governance_params.key() @ GovernanceError::InvalidAccount
    )]
    pub governance_params: Account<'info, StoredGovernanceParams>,

    #[account(
        mut,
        seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = feature_proposal.bump,
        constraint = feature_proposal.status == ProposalStatus::RewardsDistribution @ GovernanceError::InvalidProposalStatus
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,

    /// Treasury token account
    #[account(
        mut,
        constraint = treasury.key() == governance_config.treasury @ GovernanceError::InvalidAccount
    )]
    pub treasury: Account<'info, TokenAccount>,

    /// Proposer's token account
    #[account(
        mut,
        constraint = proposer_token_account.owner == feature_proposal.proposer @ GovernanceError::InvalidAccount,
        constraint = proposer_token_account.mint == governance_config.mint @ GovernanceError::InvalidMint
    )]
    pub proposer_token_account: Account<'info, TokenAccount>,

    #[account(
        constraint = oracle.key() == governance_config.oracle @ GovernanceError::OracleOnly
    )]
    pub oracle: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn finalize_rewards(ctx: Context<FinalizeRewards>, proposal_id: u64) -> Result<()> {
    let proposal = &mut ctx.accounts.feature_proposal;
    let params = &ctx.accounts.governance_params;
    let governance = &mut ctx.accounts.governance_config;

    // Verify all required gates have passed
    let benchmark_passed = proposal
        .benchmark_result
        .as_ref()
        .map(|r| r.passed)
        .unwrap_or(false);

    let ab_test_passed = proposal
        .ab_test_result
        .as_ref()
        .map(|r| r.passed)
        .unwrap_or(false);

    let feedback_passed = proposal
        .feedback_result
        .as_ref()
        .map(|r| r.passed)
        .unwrap_or(false);

    let steward_approved = proposal
        .steward_approval_result
        .as_ref()
        .map(|r| r.passed)
        .unwrap_or(false);

    require!(
        benchmark_passed && ab_test_passed && feedback_passed && steward_approved,
        GovernanceError::RequiredGatesNotPassed
    );

    // Calculate remaining reward (merge reward = bounty_merge_bps)
    let remaining_reward = (proposal.estimated_bounty as u128)
        .checked_mul(params.bounty_merge_bps as u128)
        .ok_or(GovernanceError::RewardCalculationOverflow)?
        .checked_div(BPS_DENOMINATOR as u128)
        .ok_or(GovernanceError::DivisionByZero)?;

    let remaining_reward =
        u64::try_from(remaining_reward).map_err(|_| GovernanceError::RewardCalculationOverflow)?;

    // Verify treasury has sufficient funds
    require!(
        ctx.accounts.treasury.amount >= remaining_reward,
        GovernanceError::InsufficientTreasuryFunds
    );

    // Transfer remaining reward
    let governance_seeds = &[GOVERNANCE_SEED, &[governance.bump]];
    let signer_seeds = &[&governance_seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.treasury.to_account_info(),
        to: ctx.accounts.proposer_token_account.to_account_info(),
        authority: governance.to_account_info(),
    };

    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );

    transfer(cpi_ctx, remaining_reward)?;

    // Update governance totals
    governance.total_bounties_paid = governance
        .total_bounties_paid
        .checked_add(remaining_reward)
        .ok_or(GovernanceError::ArithmeticOverflow)?;

    // Finalize proposal
    proposal.status = ProposalStatus::Finalized;
    proposal.updated_at = Clock::get()?.unix_timestamp;

    msg!(
        "Proposal {} finalized with remaining reward: {}",
        proposal_id,
        remaining_reward
    );

    Ok(())
}
