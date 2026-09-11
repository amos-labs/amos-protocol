// AMOS Governance Program - Proposal Instructions
// Handles feature proposal submission, voting, and status updates

use crate::amounts::validate_feature_bounty;
use crate::constants::*;
use crate::errors::GovernanceError;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;

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
    validate_feature_bounty(estimated_bounty)?;

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
    proposal.reserved[..8].copy_from_slice(CUSTODIED_VOTING_MARKER);

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

/// Preserved legacy account ABI; handler fails closed without custody.
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

/// Legacy instruction cannot accept votes without transferring custody.
pub fn vote_for_feature(
    _ctx: Context<VoteForFeature>,
    _proposal_id: u64,
    _vote_amount: u64,
) -> Result<()> {
    Err(GovernanceError::CustodiedVotingRequired.into())
}

// ============================================================================
// Withdraw Vote
// ============================================================================

/// Preserved legacy withdrawal ABI; no escrow exists for these records.
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

/// Legacy records never held tokens. Do not reinterpret them as V2 deposits.
pub fn withdraw_vote(_ctx: Context<WithdrawVote>, _proposal_id: u64) -> Result<()> {
    Err(GovernanceError::CustodiedVotingRequired.into())
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

    validate_status_transition(proposal, new_status, clock.unix_timestamp)?;
    if proposal.status == ProposalStatus::InDevelopment
        && new_status == ProposalStatus::AwaitingGates
    {
        proposal.completed_at = Some(clock.unix_timestamp);
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

/// The oracle remains the only status authority. Returned custody must never
/// re-enter a live tally through a terminal-to-active transition.
pub(crate) fn validate_status_transition(
    proposal: &FeatureProposal,
    new_status: ProposalStatus,
    now: i64,
) -> Result<()> {
    match (proposal.status, new_status) {
        (ProposalStatus::Active, ProposalStatus::InDevelopment) => {
            require!(
                proposal.has_custodied_voting(),
                GovernanceError::LegacyProposalRequiresResubmission
            );
            super::voting_v2::require_open_vote_window(proposal, now)
        }
        (ProposalStatus::InDevelopment, ProposalStatus::AwaitingGates)
        | (ProposalStatus::AwaitingGates, ProposalStatus::RewardsDistribution)
        | (ProposalStatus::RewardsDistribution, ProposalStatus::Finalized) => Ok(()),
        // Finalized remains finalized; cancelled remains cancelled. Historical
        // terminal identity is stable as well as the tally.
        (ProposalStatus::Finalized | ProposalStatus::Cancelled, _) => {
            Err(GovernanceError::InvalidProposalStatus.into())
        }
        (_, ProposalStatus::Cancelled) => Ok(()),
        _ => Err(GovernanceError::InvalidProposalStatus.into()),
    }
}
