// AMOS Governance Program - Research Instructions
// Handles research proposal submission, approval, milestones, and graduation

use crate::constants::*;
use crate::errors::GovernanceError;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

// ============================================================================
// Submit Research Proposal
// ============================================================================

/// Submits a new research proposal
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct SubmitResearchProposal<'info> {
    #[account(
        mut,
        seeds = [GOVERNANCE_SEED],
        bump = governance_config.bump
    )]
    pub governance_config: Account<'info, GovernanceConfig>,

    #[account(
        init,
        payer = proposer,
        space = RESEARCH_PROPOSAL_SIZE,
        seeds = [RESEARCH_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump
    )]
    pub research_proposal: Account<'info, ResearchProposal>,

    #[account(mut)]
    pub proposer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn submit_research_proposal(
    ctx: Context<SubmitResearchProposal>,
    proposal_id: u64,
    title: String,
    description: String,
    stipend: u64,
    milestones: Vec<String>,
) -> Result<()> {
    // Validate inputs
    require!(title.len() <= MAX_TITLE_LEN, GovernanceError::TitleTooLong);
    require!(
        description.len() <= MAX_DESCRIPTION_LEN,
        GovernanceError::DescriptionTooLong
    );
    require!(
        milestones.len() <= MAX_MILESTONES,
        GovernanceError::TooManyMilestones
    );
    require!(
        stipend >= MIN_RESEARCH_STIPEND,
        GovernanceError::StipendTooLow
    );
    require!(
        stipend <= MAX_RESEARCH_STIPEND,
        GovernanceError::StipendTooHigh
    );

    let proposal = &mut ctx.accounts.research_proposal;
    let clock = Clock::get()?;

    // Convert milestone strings to Milestone structs
    let milestone_structs: Vec<Milestone> = milestones
        .into_iter()
        .map(|desc| Milestone {
            description: desc,
            completed: false,
            completed_at: None,
            evidence_hash: None,
        })
        .collect();

    // Initialize proposal
    proposal.id = proposal_id;
    proposal.proposer = ctx.accounts.proposer.key();
    proposal.title = title;
    proposal.description = description;
    proposal.stipend = stipend;
    proposal.milestones = milestone_structs;
    proposal.status = ResearchStatus::Pending;
    proposal.submitted_at = clock.unix_timestamp;
    proposal.approved_at = None;
    proposal.current_milestone = 0;
    proposal.bump = ctx.bumps.research_proposal;
    proposal.reserved = [0; 128];

    msg!(
        "Research proposal {} submitted by {}",
        proposal_id,
        proposal.proposer
    );
    msg!("Title: {}", proposal.title);
    msg!("Stipend: {}", proposal.stipend);
    msg!("Milestones: {}", proposal.milestones.len());

    Ok(())
}

// ============================================================================
// Approve Research
// ============================================================================

/// Approves a research proposal and funds initial stipend
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct ApproveResearch<'info> {
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
        seeds = [RESEARCH_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = research_proposal.bump,
        constraint = research_proposal.status == ResearchStatus::Pending @ GovernanceError::InvalidProposalStatus
    )]
    pub research_proposal: Account<'info, ResearchProposal>,

    /// Treasury token account
    #[account(
        mut,
        constraint = treasury.key() == governance_config.treasury @ GovernanceError::InvalidAccount
    )]
    pub treasury: Account<'info, TokenAccount>,

    /// Researcher's token account
    #[account(
        mut,
        constraint = researcher_token_account.owner == research_proposal.proposer @ GovernanceError::InvalidAccount,
        constraint = researcher_token_account.mint == governance_config.mint @ GovernanceError::InvalidMint
    )]
    pub researcher_token_account: Account<'info, TokenAccount>,

    #[account(
        constraint = authority.key() == governance_config.authority @ GovernanceError::Unauthorized
    )]
    pub authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn approve_research(ctx: Context<ApproveResearch>, proposal_id: u64) -> Result<()> {
    let proposal = &mut ctx.accounts.research_proposal;
    let params = &ctx.accounts.governance_params;
    let governance = &ctx.accounts.governance_config;

    // Calculate stipend amount (percentage of total research budget)
    let stipend_amount = (proposal.stipend as u128)
        .checked_mul(params.research_stipend_bps as u128)
        .ok_or(GovernanceError::RewardCalculationOverflow)?
        .checked_div(BPS_DENOMINATOR as u128)
        .ok_or(GovernanceError::DivisionByZero)?;

    let stipend_amount =
        u64::try_from(stipend_amount).map_err(|_| GovernanceError::RewardCalculationOverflow)?;

    // Verify treasury has sufficient funds
    require!(
        ctx.accounts.treasury.amount >= stipend_amount,
        GovernanceError::InsufficientTreasuryFunds
    );

    // Transfer stipend to researcher
    let governance_seeds = &[GOVERNANCE_SEED, &[governance.bump]];
    let signer_seeds = &[&governance_seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.treasury.to_account_info(),
        to: ctx.accounts.researcher_token_account.to_account_info(),
        authority: governance.to_account_info(),
    };

    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );

    transfer(cpi_ctx, stipend_amount)?;

    // Update proposal status
    proposal.status = ResearchStatus::Active;
    proposal.approved_at = Some(Clock::get()?.unix_timestamp);

    msg!(
        "Research proposal {} approved with stipend: {}",
        proposal_id,
        stipend_amount
    );

    Ok(())
}

// ============================================================================
// Report Research Milestone
// ============================================================================

/// Reports completion of a research milestone
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct ReportResearchMilestone<'info> {
    #[account(
        seeds = [GOVERNANCE_SEED],
        bump = governance_config.bump
    )]
    pub governance_config: Account<'info, GovernanceConfig>,

    #[account(
        mut,
        seeds = [RESEARCH_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = research_proposal.bump,
        constraint = research_proposal.status == ResearchStatus::Active @ GovernanceError::ResearchNotApproved
    )]
    pub research_proposal: Account<'info, ResearchProposal>,

    #[account(
        constraint = oracle.key() == governance_config.oracle @ GovernanceError::OracleOnly
    )]
    pub oracle: Signer<'info>,
}

pub fn report_research_milestone(
    ctx: Context<ReportResearchMilestone>,
    proposal_id: u64,
    milestone_index: u8,
    evidence_hash: [u8; 32],
) -> Result<()> {
    let proposal = &mut ctx.accounts.research_proposal;

    // Validate milestone index
    require!(
        (milestone_index as usize) < proposal.milestones.len(),
        GovernanceError::InvalidMilestoneIndex
    );

    // Verify this is the current milestone (sequential completion)
    require!(
        milestone_index == proposal.current_milestone,
        GovernanceError::PreviousMilestoneNotCompleted
    );

    let milestone = &mut proposal.milestones[milestone_index as usize];

    // Check if already completed
    require!(
        !milestone.completed,
        GovernanceError::MilestoneAlreadyCompleted
    );

    let clock = Clock::get()?;

    // Mark milestone as completed
    milestone.completed = true;
    milestone.completed_at = Some(clock.unix_timestamp);
    milestone.evidence_hash = Some(evidence_hash);

    // Advance to next milestone
    proposal.current_milestone = proposal
        .current_milestone
        .checked_add(1)
        .ok_or(GovernanceError::ArithmeticOverflow)?;

    msg!(
        "Research proposal {} milestone {} completed",
        proposal_id,
        milestone_index
    );

    // Check if all milestones completed
    if proposal.current_milestone as usize >= proposal.milestones.len() {
        proposal.status = ResearchStatus::Completed;
        msg!("Research proposal {} all milestones completed", proposal_id);
    }

    Ok(())
}

// ============================================================================
// Graduate Research
// ============================================================================

/// Graduates successful research to feature development
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct GraduateResearch<'info> {
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
        seeds = [RESEARCH_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = research_proposal.bump,
        constraint = research_proposal.status == ResearchStatus::Completed @ GovernanceError::InvalidProposalStatus
    )]
    pub research_proposal: Account<'info, ResearchProposal>,

    /// Treasury token account
    #[account(
        mut,
        constraint = treasury.key() == governance_config.treasury @ GovernanceError::InvalidAccount
    )]
    pub treasury: Account<'info, TokenAccount>,

    /// Researcher's token account
    #[account(
        mut,
        constraint = researcher_token_account.owner == research_proposal.proposer @ GovernanceError::InvalidAccount,
        constraint = researcher_token_account.mint == governance_config.mint @ GovernanceError::InvalidMint
    )]
    pub researcher_token_account: Account<'info, TokenAccount>,

    #[account(
        constraint = authority.key() == governance_config.authority @ GovernanceError::Unauthorized
    )]
    pub authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn graduate_research(ctx: Context<GraduateResearch>, proposal_id: u64) -> Result<()> {
    let proposal = &mut ctx.accounts.research_proposal;
    let params = &ctx.accounts.governance_params;
    let governance = &ctx.accounts.governance_config;

    // Verify all milestones completed
    let all_completed = proposal.milestones.iter().all(|m| m.completed);

    require!(all_completed, GovernanceError::NotAllMilestonesCompleted);

    // Calculate success bonus
    let success_bonus = (proposal.stipend as u128)
        .checked_mul(params.research_success_bps as u128)
        .ok_or(GovernanceError::RewardCalculationOverflow)?
        .checked_div(BPS_DENOMINATOR as u128)
        .ok_or(GovernanceError::DivisionByZero)?;

    let success_bonus =
        u64::try_from(success_bonus).map_err(|_| GovernanceError::RewardCalculationOverflow)?;

    // Verify treasury has sufficient funds
    require!(
        ctx.accounts.treasury.amount >= success_bonus,
        GovernanceError::InsufficientTreasuryFunds
    );

    // Transfer success bonus to researcher
    let governance_seeds = &[GOVERNANCE_SEED, &[governance.bump]];
    let signer_seeds = &[&governance_seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.treasury.to_account_info(),
        to: ctx.accounts.researcher_token_account.to_account_info(),
        authority: governance.to_account_info(),
    };

    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );

    transfer(cpi_ctx, success_bonus)?;

    // Graduate research
    proposal.status = ResearchStatus::Graduated;

    msg!(
        "Research proposal {} graduated with success bonus: {}",
        proposal_id,
        success_bonus
    );

    Ok(())
}
