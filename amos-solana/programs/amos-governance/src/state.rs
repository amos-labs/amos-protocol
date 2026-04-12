// AMOS Governance Program - State Accounts
// Defines all on-chain account structures

use crate::constants::*;
use anchor_lang::prelude::*;

// ============================================================================
// Enums
// ============================================================================

/// Status of a feature proposal throughout its lifecycle
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProposalStatus {
    /// Initial state after submission
    Draft,
    /// Proposal is open for voting
    Active,
    /// Proposal has been approved and development started
    InDevelopment,
    /// Development completed, awaiting quality gates
    AwaitingGates,
    /// All gates passed, rewards being distributed
    RewardsDistribution,
    /// Fully complete and merged
    Finalized,
    /// Proposal was rejected or abandoned
    Cancelled,
}

/// Types of quality gate evaluations
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum GateType {
    /// Performance benchmark evaluation
    Benchmark,
    /// A/B test improvement validation
    ABTest,
    /// User feedback score validation
    Feedback,
    /// Steward approval validation
    StewardApproval,
}

/// Status of a research proposal
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResearchStatus {
    /// Submitted and awaiting review
    Pending,
    /// Approved and funded
    Active,
    /// Successfully completed
    Completed,
    /// Graduated to feature development
    Graduated,
    /// Rejected or abandoned
    Rejected,
}

/// Status of a budget gate proposal
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum BudgetGateStatus {
    /// Open for steward voting
    Voting,
    /// Approved and ready to execute
    Approved,
    /// Executed and active
    Executed,
    /// Rejected by stewards
    Rejected,
}

/// Type of steward vote
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum StewardVoteType {
    /// Vote on budget gate proposal
    BudgetGate,
    /// Vote on feature approval gate
    FeatureApproval,
}

// ============================================================================
// Governance Configuration
// ============================================================================

/// Main governance configuration account
/// PDA: ["governance"]
#[account]
pub struct GovernanceConfig {
    /// Authority that can update governance parameters
    pub authority: Pubkey,

    /// Oracle account that can report gate results
    pub oracle: Pubkey,

    /// AMOS token mint
    pub mint: Pubkey,

    /// Treasury account holding funds for bounties
    pub treasury: Pubkey,

    /// Reference to stored governance parameters
    pub params: Pubkey,

    /// Total number of proposals submitted
    pub total_proposals: u64,

    /// Total number of votes cast
    pub total_votes: u64,

    /// Total bounties paid out
    pub total_bounties_paid: u64,

    /// PDA bump seed
    pub bump: u8,

    /// Reserved space for future upgrades
    pub reserved: [u8; 128],
}

/// Stored governance parameters (tunable values)
/// Separate account to allow for parameter change proposals
#[account]
pub struct StoredGovernanceParams {
    /// MRR weight in priority calculation (basis points)
    pub mrr_weight_bps: u16,

    /// Community vote weight in priority calculation (basis points)
    pub community_weight_bps: u16,

    /// Recency bonus half-life in days
    pub recency_halflife_days: u16,

    /// Minimum benchmark performance threshold (basis points)
    pub min_benchmark_bps: u16,

    /// Minimum A/B test improvement threshold (basis points)
    pub min_ab_improvement_bps: u16,

    /// Minimum feedback score threshold (basis points)
    pub min_feedback_bps: u16,

    /// Minimum number of steward approvals required
    pub steward_quorum: u16,

    /// Bounty reward percentage on completion gate (basis points)
    pub bounty_completion_bps: u16,

    /// Bounty reward percentage on A/B test gate (basis points)
    pub bounty_ab_bps: u16,

    /// Bounty reward percentage on merge gate (basis points)
    pub bounty_merge_bps: u16,

    /// Research stipend percentage (basis points)
    pub research_stipend_bps: u16,

    /// Research success bonus percentage (basis points)
    pub research_success_bps: u16,

    /// Reserved space for future parameters
    pub reserved: [u8; 64],
}

impl Default for StoredGovernanceParams {
    fn default() -> Self {
        Self {
            mrr_weight_bps: DEFAULT_MRR_WEIGHT_BPS,
            community_weight_bps: DEFAULT_COMMUNITY_WEIGHT_BPS,
            recency_halflife_days: DEFAULT_RECENCY_HALFLIFE_DAYS,
            min_benchmark_bps: DEFAULT_MIN_BENCHMARK_BPS,
            min_ab_improvement_bps: DEFAULT_MIN_AB_IMPROVEMENT_BPS,
            min_feedback_bps: DEFAULT_MIN_FEEDBACK_BPS,
            steward_quorum: DEFAULT_STEWARD_QUORUM,
            bounty_completion_bps: DEFAULT_BOUNTY_COMPLETION_BPS,
            bounty_ab_bps: DEFAULT_BOUNTY_AB_BPS,
            bounty_merge_bps: DEFAULT_BOUNTY_MERGE_BPS,
            research_stipend_bps: DEFAULT_RESEARCH_STIPEND_BPS,
            research_success_bps: DEFAULT_RESEARCH_SUCCESS_BPS,
            reserved: [0; 64],
        }
    }
}

// ============================================================================
// Feature Proposals
// ============================================================================

/// A feature proposal submitted for community voting
/// PDA: ["feature_proposal", proposal_id.to_le_bytes()]
#[account]
pub struct FeatureProposal {
    /// Unique proposal ID
    pub id: u64,

    /// Proposer's public key
    pub proposer: Pubkey,

    /// Proposal title
    pub title: String,

    /// Detailed description
    pub description: String,

    /// Estimated bounty for completion
    pub estimated_bounty: u64,

    /// Total votes (weighted by token amount)
    pub total_votes: u64,

    /// Current status
    pub status: ProposalStatus,

    /// Linked customer request IDs (from customer registry)
    pub customer_request_ids: Vec<Pubkey>,

    /// Timestamp when proposal was created
    pub created_at: i64,

    /// Timestamp of last update
    pub updated_at: i64,

    /// Timestamp when development completed
    pub completed_at: Option<i64>,

    /// Benchmark gate result
    pub benchmark_result: Option<GateResult>,

    /// A/B test gate result
    pub ab_test_result: Option<GateResult>,

    /// Feedback gate result
    pub feedback_result: Option<GateResult>,

    /// Steward approval gate result
    pub steward_approval_result: Option<GateResult>,

    /// PDA bump seed
    pub bump: u8,

    /// Reserved space for future fields
    pub reserved: [u8; 128],
}

/// Result of a quality gate evaluation
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub struct GateResult {
    /// Whether the gate passed
    pub passed: bool,

    /// Score or metric value (in basis points)
    pub score_bps: u16,

    /// Hash of evidence/data
    pub evidence_hash: [u8; 32],

    /// Timestamp of evaluation
    pub evaluated_at: i64,

    /// Whether reward was claimed
    pub reward_claimed: bool,
}

// ============================================================================
// Voting Records
// ============================================================================

/// Record of a user's vote on a proposal
/// PDA: ["vote_record", proposal_id.to_le_bytes(), voter.key()]
#[account]
pub struct VoteRecord {
    /// Voter's public key
    pub voter: Pubkey,

    /// Proposal ID being voted on
    pub proposal_id: u64,

    /// Amount of tokens used for voting
    pub amount: u64,

    /// Timestamp when vote was cast
    pub voted_at: i64,

    /// Timestamp when vote was withdrawn (if applicable)
    pub withdrawn_at: Option<i64>,

    /// PDA bump seed
    pub bump: u8,

    /// Reserved space for future fields
    pub reserved: [u8; 64],
}

// ============================================================================
// Research Proposals
// ============================================================================

/// A research proposal for experimental features
/// PDA: ["research_proposal", proposal_id.to_le_bytes()]
#[account]
pub struct ResearchProposal {
    /// Unique proposal ID
    pub id: u64,

    /// Researcher's public key
    pub proposer: Pubkey,

    /// Research title
    pub title: String,

    /// Detailed research description
    pub description: String,

    /// Stipend amount for research
    pub stipend: u64,

    /// Research milestones
    pub milestones: Vec<Milestone>,

    /// Current status
    pub status: ResearchStatus,

    /// Timestamp when submitted
    pub submitted_at: i64,

    /// Timestamp when approved
    pub approved_at: Option<i64>,

    /// Current milestone index
    pub current_milestone: u8,

    /// PDA bump seed
    pub bump: u8,

    /// Reserved space for future fields
    pub reserved: [u8; 128],
}

/// A milestone within a research proposal
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct Milestone {
    /// Milestone description
    pub description: String,

    /// Whether milestone is completed
    pub completed: bool,

    /// Timestamp of completion
    pub completed_at: Option<i64>,

    /// Evidence hash
    pub evidence_hash: Option<[u8; 32]>,
}

// ============================================================================
// Budget Gate Proposals
// ============================================================================

/// Proposal to change budget allocation thresholds
/// PDA: ["budget_gate", proposal_id.to_le_bytes()]
#[account]
pub struct BudgetGateProposal {
    /// Unique proposal ID
    pub id: u64,

    /// Proposer (must be steward)
    pub proposer: Pubkey,

    /// Proposal title
    pub title: String,

    /// New budget threshold for major decisions
    pub new_budget_threshold: u64,

    /// Timestamp when submitted
    pub submitted_at: i64,

    /// Timestamp when executed
    pub executed_at: Option<i64>,

    /// Number of yes votes
    pub yes_votes: u8,

    /// Number of no votes
    pub no_votes: u8,

    /// Current status
    pub status: BudgetGateStatus,

    /// PDA bump seed
    pub bump: u8,

    /// Reserved space for future fields
    pub reserved: [u8; 128],
}

/// Budget allocation profile
/// PDA: ["allocation_profile", profile_id.to_le_bytes()]
#[account]
pub struct AllocationProfile {
    /// Unique profile ID
    pub id: u64,

    /// Profile name
    pub name: String,

    /// Percentage allocated to feature development (basis points)
    pub feature_dev_bps: u16,

    /// Percentage allocated to research (basis points)
    pub research_bps: u16,

    /// Percentage allocated to maintenance (basis points)
    pub maintenance_bps: u16,

    /// Percentage allocated to security (basis points)
    pub security_bps: u16,

    /// Whether this profile is currently active
    pub active: bool,

    /// Timestamp when activated
    pub activated_at: Option<i64>,

    /// PDA bump seed
    pub bump: u8,

    /// Reserved space for future fields
    pub reserved: [u8; 64],
}

/// Record of a registered steward. Only the governance authority can create/deactivate these.
/// A valid, active StewardRecord PDA is required to cast steward votes.
/// PDA: ["steward_record", steward.key()]
#[account]
pub struct StewardRecord {
    /// Steward's public key
    pub steward: Pubkey,

    /// Timestamp when steward was registered
    pub registered_at: i64,

    /// Whether this steward is currently active
    pub active: bool,

    /// PDA bump seed
    pub bump: u8,

    /// Reserved space for future fields
    pub reserved: [u8; 64],
}

/// Record of a steward's vote
/// PDA: ["steward_vote", proposal_id.to_le_bytes(), steward.key(), vote_type]
#[account]
pub struct StewardVoteRecord {
    /// Steward's public key
    pub steward: Pubkey,

    /// Proposal ID being voted on
    pub proposal_id: u64,

    /// Type of vote
    pub vote_type: StewardVoteType,

    /// Whether the steward approves
    pub approve: bool,

    /// Timestamp when vote was cast
    pub voted_at: i64,

    /// PDA bump seed
    pub bump: u8,

    /// Reserved space for future fields
    pub reserved: [u8; 64],
}
