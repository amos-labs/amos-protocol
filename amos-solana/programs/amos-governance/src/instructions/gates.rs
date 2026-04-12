// AMOS Governance Program - Quality Gate Instructions
// Handles reporting and validation of quality gate results

use crate::constants::*;
use crate::errors::GovernanceError;
use crate::state::*;
use anchor_lang::prelude::*;

// ============================================================================
// Report Benchmark Result
// ============================================================================

/// Reports benchmark performance results for a proposal
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct ReportBenchmarkResult<'info> {
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
        mut,
        seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = feature_proposal.bump,
        constraint = feature_proposal.status == ProposalStatus::AwaitingGates @ GovernanceError::InvalidProposalStatus
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,

    #[account(
        constraint = oracle.key() == governance_config.oracle @ GovernanceError::OracleOnly
    )]
    pub oracle: Signer<'info>,
}

pub fn report_benchmark_result(
    ctx: Context<ReportBenchmarkResult>,
    proposal_id: u64,
    score_bps: u16,
    evidence_hash: [u8; 32],
) -> Result<()> {
    let proposal = &mut ctx.accounts.feature_proposal;
    let params = &ctx.accounts.governance_params;

    // Check if gate already evaluated
    require!(
        proposal.benchmark_result.is_none(),
        GovernanceError::GateAlreadyEvaluated
    );

    // Validate score is within bounds (0-10000 bps)
    require!(
        score_bps <= BPS_DENOMINATOR,
        GovernanceError::InvalidParameter
    );

    let clock = Clock::get()?;

    // Determine if gate passed
    let passed = score_bps >= params.min_benchmark_bps;

    // Record gate result
    proposal.benchmark_result = Some(GateResult {
        passed,
        score_bps,
        evidence_hash,
        evaluated_at: clock.unix_timestamp,
        reward_claimed: false,
    });

    proposal.updated_at = clock.unix_timestamp;

    msg!(
        "Benchmark gate for proposal {}: {} (score: {}bps, threshold: {}bps)",
        proposal_id,
        if passed { "PASSED" } else { "FAILED" },
        score_bps,
        params.min_benchmark_bps
    );

    Ok(())
}

// ============================================================================
// Report A/B Test Result
// ============================================================================

/// Reports A/B test improvement results for a proposal
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct ReportABTestResult<'info> {
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
        mut,
        seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = feature_proposal.bump,
        constraint = feature_proposal.status == ProposalStatus::AwaitingGates @ GovernanceError::InvalidProposalStatus
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,

    #[account(
        constraint = oracle.key() == governance_config.oracle @ GovernanceError::OracleOnly
    )]
    pub oracle: Signer<'info>,
}

pub fn report_ab_test_result(
    ctx: Context<ReportABTestResult>,
    proposal_id: u64,
    improvement_bps: u16,
    evidence_hash: [u8; 32],
) -> Result<()> {
    let proposal = &mut ctx.accounts.feature_proposal;
    let params = &ctx.accounts.governance_params;

    // Check if gate already evaluated
    require!(
        proposal.ab_test_result.is_none(),
        GovernanceError::GateAlreadyEvaluated
    );

    // Validate improvement is within reasonable bounds (0-100% = 0-10000 bps)
    require!(
        improvement_bps <= BPS_DENOMINATOR,
        GovernanceError::InvalidParameter
    );

    let clock = Clock::get()?;

    // Determine if gate passed
    let passed = improvement_bps >= params.min_ab_improvement_bps;

    // Record gate result
    proposal.ab_test_result = Some(GateResult {
        passed,
        score_bps: improvement_bps,
        evidence_hash,
        evaluated_at: clock.unix_timestamp,
        reward_claimed: false,
    });

    proposal.updated_at = clock.unix_timestamp;

    msg!(
        "A/B test gate for proposal {}: {} (improvement: {}bps, threshold: {}bps)",
        proposal_id,
        if passed { "PASSED" } else { "FAILED" },
        improvement_bps,
        params.min_ab_improvement_bps
    );

    Ok(())
}

// ============================================================================
// Report Feedback Result
// ============================================================================

/// Reports user feedback score results for a proposal
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct ReportFeedbackResult<'info> {
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
        mut,
        seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = feature_proposal.bump,
        constraint = feature_proposal.status == ProposalStatus::AwaitingGates @ GovernanceError::InvalidProposalStatus
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,

    #[account(
        constraint = oracle.key() == governance_config.oracle @ GovernanceError::OracleOnly
    )]
    pub oracle: Signer<'info>,
}

pub fn report_feedback_result(
    ctx: Context<ReportFeedbackResult>,
    proposal_id: u64,
    score_bps: u16,
    evidence_hash: [u8; 32],
) -> Result<()> {
    let proposal = &mut ctx.accounts.feature_proposal;
    let params = &ctx.accounts.governance_params;

    // Check if gate already evaluated
    require!(
        proposal.feedback_result.is_none(),
        GovernanceError::GateAlreadyEvaluated
    );

    // Validate score is within bounds (0-100% = 0-10000 bps)
    require!(
        score_bps <= BPS_DENOMINATOR,
        GovernanceError::InvalidParameter
    );

    let clock = Clock::get()?;

    // Determine if gate passed
    let passed = score_bps >= params.min_feedback_bps;

    // Record gate result
    proposal.feedback_result = Some(GateResult {
        passed,
        score_bps,
        evidence_hash,
        evaluated_at: clock.unix_timestamp,
        reward_claimed: false,
    });

    proposal.updated_at = clock.unix_timestamp;

    msg!(
        "Feedback gate for proposal {}: {} (score: {}bps, threshold: {}bps)",
        proposal_id,
        if passed { "PASSED" } else { "FAILED" },
        score_bps,
        params.min_feedback_bps
    );

    Ok(())
}

// ============================================================================
// Report Steward Approval
// ============================================================================

/// Reports steward approval results for a proposal
#[derive(Accounts)]
#[instruction(proposal_id: u64)]
pub struct ReportStewardApproval<'info> {
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
        mut,
        seeds = [FEATURE_PROPOSAL_SEED, proposal_id.to_le_bytes().as_ref()],
        bump = feature_proposal.bump,
        constraint = feature_proposal.status == ProposalStatus::AwaitingGates @ GovernanceError::InvalidProposalStatus
    )]
    pub feature_proposal: Box<Account<'info, FeatureProposal>>,

    #[account(
        constraint = oracle.key() == governance_config.oracle @ GovernanceError::OracleOnly
    )]
    pub oracle: Signer<'info>,
}

pub fn report_steward_approval(
    ctx: Context<ReportStewardApproval>,
    proposal_id: u64,
    approval_count: u16,
    evidence_hash: [u8; 32],
) -> Result<()> {
    let proposal = &mut ctx.accounts.feature_proposal;
    let params = &ctx.accounts.governance_params;

    // Check if gate already evaluated
    require!(
        proposal.steward_approval_result.is_none(),
        GovernanceError::GateAlreadyEvaluated
    );

    let clock = Clock::get()?;

    // Determine if gate passed (quorum met)
    let passed = approval_count >= params.steward_quorum;

    // Store approval count as basis points for consistency
    // (approval_count * 1000 to represent in a comparable scale)
    let score_bps = approval_count.min(10); // Cap at 10 for sanity

    // Record gate result
    proposal.steward_approval_result = Some(GateResult {
        passed,
        score_bps,
        evidence_hash,
        evaluated_at: clock.unix_timestamp,
        reward_claimed: false,
    });

    proposal.updated_at = clock.unix_timestamp;

    msg!(
        "Steward approval gate for proposal {}: {} (approvals: {}, quorum: {})",
        proposal_id,
        if passed { "PASSED" } else { "FAILED" },
        approval_count,
        params.steward_quorum
    );

    Ok(())
}
