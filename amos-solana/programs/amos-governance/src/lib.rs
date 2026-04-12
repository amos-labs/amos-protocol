// AMOS Governance Program
// On-chain voting, proposals, quality gates, and staged rewards system
//
// Features:
// - Feature proposal submission and community voting (weighted by AMOS tokens)
// - Quality gate evaluation (benchmark, A/B test, feedback, steward approval)
// - Staged bounty rewards (40% completion, 30% A/B test, 30% merge)
// - Research proposals with milestone tracking and graduation to features
// - MRR-weighted priority scoring with recency decay
// - Budget gate proposals and steward voting
// - Tunable governance parameters

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;
use state::*;

declare_id!("245xpoWLEAAPmUQxMSBDqQw5qnGfqt5roi5enuFG9fZZ");

#[program]
pub mod amos_governance {
    use super::*;

    // ========================================================================
    // Governance Management
    // ========================================================================

    /// Initializes the governance system with configuration
    ///
    /// # Arguments
    /// * `ctx` - Context containing governance accounts
    /// * `params` - Optional custom governance parameters (uses defaults if None)
    ///
    /// # Access
    /// * Public - called once during program deployment
    pub fn initialize_governance(
        ctx: Context<InitializeGovernance>,
        params: Option<StoredGovernanceParams>,
    ) -> Result<()> {
        instructions::initialize_governance(ctx, params)
    }

    /// Updates governance parameters
    ///
    /// # Arguments
    /// * `ctx` - Context with governance config and authority
    /// * `new_params` - New parameter values
    ///
    /// # Access
    /// * Authority only
    pub fn update_governance_params(
        ctx: Context<UpdateGovernanceParams>,
        new_params: StoredGovernanceParams,
    ) -> Result<()> {
        instructions::update_governance_params(ctx, new_params)
    }

    // ========================================================================
    // Feature Proposals
    // ========================================================================

    /// Submits a new feature proposal for community voting
    ///
    /// # Arguments
    /// * `ctx` - Context with proposal accounts
    /// * `proposal_id` - Unique proposal identifier
    /// * `title` - Proposal title (max 64 chars)
    /// * `description` - Detailed description (max 500 chars)
    /// * `estimated_bounty` - Estimated reward amount in tokens
    /// * `customer_request_ids` - Linked customer request accounts (max 100)
    ///
    /// # Access
    /// * Public - any user can submit proposals
    pub fn submit_feature_proposal(
        ctx: Context<SubmitFeatureProposal>,
        proposal_id: u64,
        title: String,
        description: String,
        estimated_bounty: u64,
        customer_request_ids: Vec<Pubkey>,
    ) -> Result<()> {
        instructions::submit_feature_proposal(
            ctx,
            proposal_id,
            title,
            description,
            estimated_bounty,
            customer_request_ids,
        )
    }

    /// Casts a weighted vote for a feature proposal
    ///
    /// # Arguments
    /// * `ctx` - Context with proposal and voter accounts
    /// * `proposal_id` - Proposal to vote on
    /// * `vote_amount` - Number of tokens to allocate to vote
    ///
    /// # Access
    /// * Public - any AMOS token holder can vote
    ///
    /// # Notes
    /// * Vote weight is proportional to token amount
    /// * Cannot vote on own proposals
    /// * Votes are locked for 7 days
    pub fn vote_for_feature(
        ctx: Context<VoteForFeature>,
        proposal_id: u64,
        vote_amount: u64,
    ) -> Result<()> {
        instructions::vote_for_feature(ctx, proposal_id, vote_amount)
    }

    /// Withdraws a vote from a proposal
    ///
    /// # Arguments
    /// * `ctx` - Context with proposal and vote record
    /// * `proposal_id` - Proposal to withdraw vote from
    ///
    /// # Access
    /// * Voter only
    ///
    /// # Notes
    /// * Can only withdraw after 7-day lock period
    /// * Proposal must still be in Active state
    pub fn withdraw_vote(ctx: Context<WithdrawVote>, proposal_id: u64) -> Result<()> {
        instructions::withdraw_vote(ctx, proposal_id)
    }

    /// Updates the status of a proposal
    ///
    /// # Arguments
    /// * `ctx` - Context with governance and proposal
    /// * `proposal_id` - Proposal to update
    /// * `new_status` - New status value
    ///
    /// # Access
    /// * Oracle only
    ///
    /// # Valid Transitions
    /// * Active -> InDevelopment
    /// * InDevelopment -> AwaitingGates
    /// * AwaitingGates -> RewardsDistribution
    /// * RewardsDistribution -> Finalized
    /// * Any -> Cancelled
    pub fn update_proposal_status(
        ctx: Context<UpdateProposalStatus>,
        proposal_id: u64,
        new_status: ProposalStatus,
    ) -> Result<()> {
        instructions::update_proposal_status(ctx, proposal_id, new_status)
    }

    // ========================================================================
    // Quality Gates
    // ========================================================================

    /// Reports benchmark performance results
    ///
    /// # Arguments
    /// * `ctx` - Context with governance and proposal
    /// * `proposal_id` - Proposal being evaluated
    /// * `score_bps` - Performance score in basis points (0-10000)
    /// * `evidence_hash` - Hash of benchmark data/evidence
    ///
    /// # Access
    /// * Oracle only
    ///
    /// # Notes
    /// * Proposal must be in AwaitingGates status
    /// * Gate passes if score >= min_benchmark_bps
    pub fn report_benchmark_result(
        ctx: Context<ReportBenchmarkResult>,
        proposal_id: u64,
        score_bps: u16,
        evidence_hash: [u8; 32],
    ) -> Result<()> {
        instructions::report_benchmark_result(ctx, proposal_id, score_bps, evidence_hash)
    }

    /// Reports A/B test improvement results
    ///
    /// # Arguments
    /// * `ctx` - Context with governance and proposal
    /// * `proposal_id` - Proposal being evaluated
    /// * `improvement_bps` - Improvement percentage in basis points
    /// * `evidence_hash` - Hash of A/B test data
    ///
    /// # Access
    /// * Oracle only
    ///
    /// # Notes
    /// * Gate passes if improvement >= min_ab_improvement_bps
    pub fn report_ab_test_result(
        ctx: Context<ReportABTestResult>,
        proposal_id: u64,
        improvement_bps: u16,
        evidence_hash: [u8; 32],
    ) -> Result<()> {
        instructions::report_ab_test_result(ctx, proposal_id, improvement_bps, evidence_hash)
    }

    /// Reports user feedback score results
    ///
    /// # Arguments
    /// * `ctx` - Context with governance and proposal
    /// * `proposal_id` - Proposal being evaluated
    /// * `score_bps` - Feedback score in basis points (0-10000)
    /// * `evidence_hash` - Hash of feedback data
    ///
    /// # Access
    /// * Oracle only
    ///
    /// # Notes
    /// * Gate passes if score >= min_feedback_bps
    pub fn report_feedback_result(
        ctx: Context<ReportFeedbackResult>,
        proposal_id: u64,
        score_bps: u16,
        evidence_hash: [u8; 32],
    ) -> Result<()> {
        instructions::report_feedback_result(ctx, proposal_id, score_bps, evidence_hash)
    }

    /// Reports steward approval results
    ///
    /// # Arguments
    /// * `ctx` - Context with governance and proposal
    /// * `proposal_id` - Proposal being evaluated
    /// * `approval_count` - Number of steward approvals
    /// * `evidence_hash` - Hash of approval records
    ///
    /// # Access
    /// * Oracle only
    ///
    /// # Notes
    /// * Gate passes if approval_count >= steward_quorum
    pub fn report_steward_approval(
        ctx: Context<ReportStewardApproval>,
        proposal_id: u64,
        approval_count: u16,
        evidence_hash: [u8; 32],
    ) -> Result<()> {
        instructions::report_steward_approval(ctx, proposal_id, approval_count, evidence_hash)
    }

    // ========================================================================
    // Staged Rewards
    // ========================================================================

    /// Claims a staged bounty reward after passing a quality gate
    ///
    /// # Arguments
    /// * `ctx` - Context with proposal, treasury, and proposer accounts
    /// * `proposal_id` - Proposal to claim reward for
    /// * `gate_type` - Which gate reward to claim (Benchmark or ABTest)
    ///
    /// # Access
    /// * Proposal owner only
    ///
    /// # Reward Stages
    /// * Benchmark gate: 40% of bounty (completion)
    /// * ABTest gate: 30% of bounty (A/B test pass)
    /// * Merge gate: 30% of bounty (via finalize_rewards)
    ///
    /// # Notes
    /// * Gate must have passed
    /// * Reward can only be claimed once per gate
    /// * For merge reward, use finalize_rewards instead
    pub fn claim_bounty_reward(
        ctx: Context<ClaimBountyReward>,
        proposal_id: u64,
        gate_type: GateType,
    ) -> Result<()> {
        instructions::claim_bounty_reward(ctx, proposal_id, gate_type)
    }

    /// Finalizes all remaining rewards after full merge
    ///
    /// # Arguments
    /// * `ctx` - Context with proposal, treasury, and proposer accounts
    /// * `proposal_id` - Proposal to finalize
    ///
    /// # Access
    /// * Oracle only
    ///
    /// # Notes
    /// * All quality gates must have passed
    /// * Pays remaining 30% of bounty
    /// * Marks proposal as Finalized
    pub fn finalize_rewards(ctx: Context<FinalizeRewards>, proposal_id: u64) -> Result<()> {
        instructions::finalize_rewards(ctx, proposal_id)
    }

    // ========================================================================
    // Research Proposals
    // ========================================================================

    /// Submits a new research proposal
    ///
    /// # Arguments
    /// * `ctx` - Context with research proposal account
    /// * `proposal_id` - Unique identifier
    /// * `title` - Research title (max 64 chars)
    /// * `description` - Detailed description (max 500 chars)
    /// * `stipend` - Total research budget
    /// * `milestones` - List of milestone descriptions (max 10)
    ///
    /// # Access
    /// * Public - any user can submit
    pub fn submit_research_proposal(
        ctx: Context<SubmitResearchProposal>,
        proposal_id: u64,
        title: String,
        description: String,
        stipend: u64,
        milestones: Vec<String>,
    ) -> Result<()> {
        instructions::submit_research_proposal(
            ctx,
            proposal_id,
            title,
            description,
            stipend,
            milestones,
        )
    }

    /// Approves a research proposal and funds initial stipend
    ///
    /// # Arguments
    /// * `ctx` - Context with proposal, treasury, and researcher accounts
    /// * `proposal_id` - Proposal to approve
    ///
    /// # Access
    /// * Authority only
    ///
    /// # Notes
    /// * Pays research_stipend_bps% of budget upfront (default 20%)
    pub fn approve_research(ctx: Context<ApproveResearch>, proposal_id: u64) -> Result<()> {
        instructions::approve_research(ctx, proposal_id)
    }

    /// Reports completion of a research milestone
    ///
    /// # Arguments
    /// * `ctx` - Context with governance and proposal
    /// * `proposal_id` - Research proposal
    /// * `milestone_index` - Which milestone was completed
    /// * `evidence_hash` - Hash of milestone deliverable
    ///
    /// # Access
    /// * Oracle only
    ///
    /// # Notes
    /// * Milestones must be completed sequentially
    pub fn report_research_milestone(
        ctx: Context<ReportResearchMilestone>,
        proposal_id: u64,
        milestone_index: u8,
        evidence_hash: [u8; 32],
    ) -> Result<()> {
        instructions::report_research_milestone(ctx, proposal_id, milestone_index, evidence_hash)
    }

    /// Graduates successful research to feature development
    ///
    /// # Arguments
    /// * `ctx` - Context with proposal, treasury, and researcher accounts
    /// * `proposal_id` - Research to graduate
    ///
    /// # Access
    /// * Authority only
    ///
    /// # Notes
    /// * All milestones must be completed
    /// * Pays success bonus (default 400% of remaining budget)
    /// * Research can then be converted to a feature proposal
    pub fn graduate_research(ctx: Context<GraduateResearch>, proposal_id: u64) -> Result<()> {
        instructions::graduate_research(ctx, proposal_id)
    }

    // ========================================================================
    // Priority Calculation
    // ========================================================================

    /// Calculates MRR-weighted priority score for a proposal
    ///
    /// # Arguments
    /// * `ctx` - Context with governance and proposal
    /// * `proposal_id` - Proposal to calculate priority for
    /// * `customer_vote_amount` - MRR-weighted votes from customer requests
    ///
    /// # Returns
    /// * PriorityScore struct with breakdown of score components
    ///
    /// # Formula
    /// Priority = (MRR_weight × customer_votes) + (community_weight × community_votes) + recency_bonus
    ///
    /// # Notes
    /// * Recency bonus decays with half-life (default 30 days)
    /// * Higher scores = higher priority
    pub fn calculate_priority(
        ctx: Context<CalculatePriority>,
        proposal_id: u64,
        customer_vote_amount: u64,
    ) -> Result<priority::PriorityScore> {
        instructions::calculate_priority(ctx, proposal_id, customer_vote_amount)
    }

    // ========================================================================
    // Steward Registry
    // ========================================================================

    /// Register a new steward in the on-chain registry.
    /// Only the governance authority can register stewards.
    ///
    /// # Access
    /// * Authority only
    pub fn register_steward(ctx: Context<RegisterSteward>) -> Result<()> {
        instructions::register_steward(ctx)
    }

    /// Remove (deactivate) a steward from the registry.
    /// Only the governance authority can remove stewards.
    /// The steward record is kept but marked inactive.
    ///
    /// # Access
    /// * Authority only
    pub fn remove_steward(ctx: Context<RemoveSteward>) -> Result<()> {
        instructions::remove_steward(ctx)
    }

    // ========================================================================
    // Budget Gate Proposals
    // ========================================================================

    /// Submits a proposal to change budget allocation thresholds
    ///
    /// # Arguments
    /// * `ctx` - Context with budget proposal account
    /// * `proposal_id` - Unique identifier
    /// * `title` - Proposal title
    /// * `new_budget_threshold` - Proposed new threshold
    ///
    /// # Access
    /// * Public (typically stewards)
    pub fn submit_budget_gate_proposal(
        ctx: Context<SubmitBudgetGateProposal>,
        proposal_id: u64,
        title: String,
        new_budget_threshold: u64,
    ) -> Result<()> {
        instructions::submit_budget_gate_proposal(ctx, proposal_id, title, new_budget_threshold)
    }

    /// Casts a steward vote on a budget gate proposal
    ///
    /// # Arguments
    /// * `ctx` - Context with proposal and vote record
    /// * `proposal_id` - Proposal to vote on
    /// * `vote_type` - Type of steward vote
    /// * `approve` - Whether to approve or reject
    ///
    /// # Access
    /// * Stewards only (validated by steward list)
    ///
    /// # Notes
    /// * Proposal approved when yes_votes >= steward_quorum
    /// * Proposal rejected when no_votes >= steward_quorum
    pub fn cast_steward_vote(
        ctx: Context<CastStewardVote>,
        proposal_id: u64,
        vote_type: StewardVoteType,
        approve: bool,
    ) -> Result<()> {
        instructions::cast_steward_vote(ctx, proposal_id, vote_type, approve)
    }

    /// Executes an approved budget gate proposal and activates allocation profile
    ///
    /// # Arguments
    /// * `ctx` - Context with proposal and allocation profile
    /// * `proposal_id` - Budget proposal to execute
    /// * `profile_id` - Allocation profile identifier
    /// * `name` - Profile name
    /// * `feature_dev_bps` - Percentage for feature development (basis points)
    /// * `research_bps` - Percentage for research (basis points)
    /// * `maintenance_bps` - Percentage for maintenance (basis points)
    /// * `security_bps` - Percentage for security (basis points)
    ///
    /// # Access
    /// * Authority only
    ///
    /// # Notes
    /// * All percentages must sum to 10000 (100%)
    /// * Proposal must be approved by steward quorum
    pub fn execute_budget_profile_activation(
        ctx: Context<ExecuteBudgetProfileActivation>,
        proposal_id: u64,
        profile_id: u64,
        name: String,
        feature_dev_bps: u16,
        research_bps: u16,
        maintenance_bps: u16,
        security_bps: u16,
    ) -> Result<()> {
        instructions::execute_budget_profile_activation(
            ctx,
            proposal_id,
            profile_id,
            name,
            feature_dev_bps,
            research_bps,
            maintenance_bps,
            security_bps,
        )
    }
}
