// AMOS Governance Program - Proposal Instructions
// Handles feature proposal submission, voting, and status updates

use crate::constants::*;
use crate::errors::GovernanceError;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

// ============================================================================
// Submit Feature Proposal
// ============================================================================

/// Submits a new feature proposal for community voting
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct SubmitFeatureProposal<'info> {
    #[account(
        mut,
        seeds = [GOVERNANCE_SEED],
        bump = governance_config.bump
    )]
    pub governance_config: Account<'info, GovernanceConfig>,

    #[account(
        init,
        payer = proposer,
        space = FEATURE_PROPOSAL_SIZE,
        seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,

    #[account(mut)]
    pub proposer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn submit_feature_proposal(
    ctx: Context<SubmitFeatureProposal>,
    proposal_id: u64,
    title: String,
    description: String,
    estimated_bounty: u64,
    customer_request_ids: Vec<Pubkey>,
) -> Result<()> {
    // Validate inputs
    require!(title.len() <= MAX_TITLE_LEN, GovernanceError::TitleTooLong);
    require!(
        description.len() <= MAX_DESCRIPTION_LEN,
        GovernanceError::DescriptionTooLong
    );
    require!(
        customer_request_ids.len() <= MAX_CUSTOMER_REQUESTS,
        GovernanceError::TooManyCustomerRequests
    );
    require!(
        estimated_bounty >= MIN_BOUNTY_AMOUNT,
        GovernanceError::BountyTooLow
    );
    require!(
        estimated_bounty <= MAX_BOUNTY_AMOUNT,
        GovernanceError::BountyTooHigh
    );

    let proposal = &mut ctx.accounts.feature_proposal;
    let governance = &mut ctx.accounts.governance_config;
    let clock = Clock::get()?;

    // Initialize proposal
    proposal.id = proposal_id;
    proposal.proposer = ctx.accounts.proposer.key();
    proposal.title = title;
    proposal.description = description;
    proposal.estimated_bounty = estimated_bounty;
    proposal.total_votes = 0;
    proposal.status = ProposalStatus::Active;
    proposal.customer_request_ids = customer_request_ids;
    proposal.created_at = clock.unix_timestamp;
    proposal.updated_at = clock.unix_timestamp;
    proposal.completed_at = None;
    proposal.benchmark_result = None;
    proposal.ab_test_result = None;
    proposal.feedback_result = None;
    proposal.steward_approval_result = None;
    proposal.bump = ctx.bumps.feature_proposal;
    proposal.reserved = [0; 128];

    // Increment total proposals counter
    governance.total_proposals = governance
        .total_proposals
        .checked_add(1)
        .ok_or(GovernanceError::ArithmeticOverflow)?;

    msg!(
        "Feature proposal {} submitted by {}",
        proposal_id,
        proposal.proposer
    );
    msg!("Title: {}", proposal.title);
    msg!("Estimated bounty: {}", proposal.estimated_bounty);

    Ok(())
}

// ============================================================================
// Vote for Feature
// ============================================================================

/// Casts a weighted vote for a feature proposal
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct VoteForFeature<'info> {
    #[account(
        mut,
        seeds = [GOVERNANCE_SEED],
        bump = governance_config.bump
    )]
    pub governance_config: Account<'info, GovernanceConfig>,

    #[account(
        mut,
        seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = feature_proposal.bump,
        constraint = feature_proposal.status == ProposalStatus::Active @ GovernanceError::InvalidProposalStatus
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,

    #[account(
        init,
        payer = voter,
        space = VOTE_RECORD_SIZE,
        seeds = [
            VOTE_RECORD_SEED,
            proposal_id.to_le_bytes().as_ref(),
            voter.key().as_ref()
        ],
        bump
    )]
    pub vote_record: Account<'info, VoteRecord>,

    #[account(mut)]
    pub voter: Signer<'info>,

    /// Voter's token account (for vote weight validation)
    #[account(
        constraint = voter_token_account.owner == voter.key() @ GovernanceError::InvalidAccount,
        constraint = voter_token_account.mint == governance_config.mint @ GovernanceError::InvalidMint
    )]
    pub voter_token_account: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
}

pub fn vote_for_feature(
    ctx: Context<VoteForFeature>,
    proposal_id: u64,
    vote_amount: u64,
) -> Result<()> {
    let proposal = &mut ctx.accounts.feature_proposal;
    let vote_record = &mut ctx.accounts.vote_record;
    let governance = &mut ctx.accounts.governance_config;
    let voter_balance = ctx.accounts.voter_token_account.amount;

    // Validate vote amount
    require!(
        vote_amount >= MIN_VOTE_AMOUNT,
        GovernanceError::VoteAmountTooLow
    );
    require!(
        vote_amount <= voter_balance,
        GovernanceError::InsufficientBalance
    );

    // Cannot vote on own proposal
    require!(
        ctx.accounts.voter.key() != proposal.proposer,
        GovernanceError::CannotVoteOnOwnProposal
    );

    let clock = Clock::get()?;

    // Check if proposal has expired
    let age = clock
        .unix_timestamp
        .checked_sub(proposal.created_at)
        .ok_or(GovernanceError::ArithmeticUnderflow)?;
    require!(
        age <= PROPOSAL_EXPIRATION_SECONDS,
        GovernanceError::ProposalExpired
    );

    // Record vote
    vote_record.voter = ctx.accounts.voter.key();
    vote_record.proposal_id = proposal_id;
    vote_record.amount = vote_amount;
    vote_record.voted_at = clock.unix_timestamp;
    vote_record.withdrawn_at = None;
    vote_record.bump = ctx.bumps.vote_record;
    vote_record.reserved = [0; 64];

    // Update proposal vote count
    proposal.total_votes = proposal
        .total_votes
        .checked_add(vote_amount)
        .ok_or(GovernanceError::ArithmeticOverflow)?;
    proposal.updated_at = clock.unix_timestamp;

    // Update governance totals
    governance.total_votes = governance
        .total_votes
        .checked_add(1)
        .ok_or(GovernanceError::ArithmeticOverflow)?;

    msg!(
        "Vote cast: {} tokens for proposal {} by {}",
        vote_amount,
        proposal_id,
        vote_record.voter
    );
    msg!("Proposal total votes: {}", proposal.total_votes);

    Ok(())
}

// ============================================================================
// Withdraw Vote
// ============================================================================

/// Withdraws a vote from a proposal (if not locked)
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct WithdrawVote<'info> {
    #[account(
        mut,
        seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = feature_proposal.bump
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,

    #[account(
        mut,
        seeds = [
            VOTE_RECORD_SEED,
            proposal_id.to_le_bytes().as_ref(),
            voter.key().as_ref()
        ],
        bump = vote_record.bump,
        constraint = vote_record.withdrawn_at.is_none() @ GovernanceError::VoteAlreadyWithdrawn
    )]
    pub vote_record: Account<'info, VoteRecord>,

    #[account(mut)]
    pub voter: Signer<'info>,
}

pub fn withdraw_vote(ctx: Context<WithdrawVote>, proposal_id: u64) -> Result<()> {
    let proposal = &mut ctx.accounts.feature_proposal;
    let vote_record = &mut ctx.accounts.vote_record;
    let clock = Clock::get()?;

    // Check if vote is locked
    let time_since_vote = clock
        .unix_timestamp
        .checked_sub(vote_record.voted_at)
        .ok_or(GovernanceError::ArithmeticUnderflow)?;

    require!(
        time_since_vote >= VOTE_LOCK_SECONDS,
        GovernanceError::VoteLocked
    );

    // Cannot withdraw if proposal is beyond active status
    require!(
        proposal.status == ProposalStatus::Active,
        GovernanceError::InvalidProposalStatus
    );

    // Update vote record
    vote_record.withdrawn_at = Some(clock.unix_timestamp);

    // Update proposal vote count
    proposal.total_votes = proposal
        .total_votes
        .checked_sub(vote_record.amount)
        .ok_or(GovernanceError::ArithmeticUnderflow)?;
    proposal.updated_at = clock.unix_timestamp;

    msg!(
        "Vote withdrawn: {} tokens from proposal {} by {}",
        vote_record.amount,
        proposal_id,
        vote_record.voter
    );

    Ok(())
}

// ============================================================================
// Update Proposal Status
// ============================================================================

/// Updates the status of a proposal (oracle only)
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct UpdateProposalStatus<'info> {
    #[account(
        seeds = [GOVERNANCE_SEED],
        bump = governance_config.bump
    )]
    pub governance_config: Account<'info, GovernanceConfig>,

    #[account(
        mut,
        seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = feature_proposal.bump
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,

    #[account(
        constraint = oracle.key() == governance_config.oracle @ GovernanceError::OracleOnly
    )]
    pub oracle: Signer<'info>,
}

pub fn update_proposal_status(
    ctx: Context<UpdateProposalStatus>,
    proposal_id: u64,
    new_status: ProposalStatus,
) -> Result<()> {
    let proposal = &mut ctx.accounts.feature_proposal;
    let clock = Clock::get()?;

    // Validate status transition
    match (proposal.status, new_status) {
        // Valid transitions
        (ProposalStatus::Active, ProposalStatus::InDevelopment) => {}
        (ProposalStatus::InDevelopment, ProposalStatus::AwaitingGates) => {
            proposal.completed_at = Some(clock.unix_timestamp);
        }
        (ProposalStatus::AwaitingGates, ProposalStatus::RewardsDistribution) => {}
        (ProposalStatus::RewardsDistribution, ProposalStatus::Finalized) => {}
        (_, ProposalStatus::Cancelled) => {}
        // Invalid transition
        _ => return Err(GovernanceError::InvalidProposalStatus.into()),
    }

    let old_status = proposal.status;
    proposal.status = new_status;
    proposal.updated_at = clock.unix_timestamp;

    msg!(
        "Proposal {} status updated from {:?} to {:?}",
        proposal_id,
        old_status,
        new_status
    );

    Ok(())
}
