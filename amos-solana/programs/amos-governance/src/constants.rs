// AMOS Governance Program - Constants
// Defines all constant values, default parameters, and PDA seeds

use anchor_lang::prelude::*;

// ============================================================================
// String Length Limits
// ============================================================================

/// Maximum length for proposal titles
pub const MAX_TITLE_LEN: usize = 64;

/// Maximum length for proposal descriptions
pub const MAX_DESCRIPTION_LEN: usize = 500;

/// Maximum number of customer requests that can be linked to a proposal
pub const MAX_CUSTOMER_REQUESTS: usize = 100;

/// Maximum number of milestones in a research proposal
pub const MAX_MILESTONES: usize = 10;

// ============================================================================
// Arithmetic Constants
// ============================================================================

/// Basis points denominator (10000 = 100%)
pub const BPS_DENOMINATOR: u16 = 10000;

// ============================================================================
// Default Governance Parameters (in basis points)
// ============================================================================

/// Default MRR weight in priority calculation (6000 bps = 60%)
pub const DEFAULT_MRR_WEIGHT_BPS: u16 = 6000;

/// Default community vote weight in priority calculation (4000 bps = 40%)
pub const DEFAULT_COMMUNITY_WEIGHT_BPS: u16 = 4000;

/// Default recency bonus half-life in days (30 days)
pub const DEFAULT_RECENCY_HALFLIFE_DAYS: u16 = 30;

/// Minimum benchmark performance threshold (7000 bps = 70%)
pub const DEFAULT_MIN_BENCHMARK_BPS: u16 = 7000;

/// Minimum A/B test improvement threshold (500 bps = 5%)
pub const DEFAULT_MIN_AB_IMPROVEMENT_BPS: u16 = 500;

/// Minimum feedback score threshold (7000 bps = 70%)
pub const DEFAULT_MIN_FEEDBACK_BPS: u16 = 7000;

/// Minimum number of steward approvals required (quorum)
pub const DEFAULT_STEWARD_QUORUM: u16 = 3;

/// Bounty reward on completion gate (4000 bps = 40%)
pub const DEFAULT_BOUNTY_COMPLETION_BPS: u16 = 4000;

/// Bounty reward on A/B test gate (3000 bps = 30%)
pub const DEFAULT_BOUNTY_AB_BPS: u16 = 3000;

/// Bounty reward on merge gate (3000 bps = 30%)
pub const DEFAULT_BOUNTY_MERGE_BPS: u16 = 3000;

/// Research stipend percentage of total (2000 bps = 20%)
pub const DEFAULT_RESEARCH_STIPEND_BPS: u16 = 2000;

/// Research success bonus percentage (40000 bps = 400%)
pub const DEFAULT_RESEARCH_SUCCESS_BPS: u16 = 40000;

// ============================================================================
// Time Constants (in seconds)
// ============================================================================

/// Proposal expiration time (90 days)
pub const PROPOSAL_EXPIRATION_SECONDS: i64 = 90 * 24 * 60 * 60;

/// Vote lock period (7 days)
pub const VOTE_LOCK_SECONDS: i64 = 7 * 24 * 60 * 60;

/// Parameter change timelock (3 days)
pub const PARAM_CHANGE_TIMELOCK_SECONDS: i64 = 3 * 24 * 60 * 60;

/// Research proposal evaluation period (30 days)
pub const RESEARCH_EVAL_PERIOD_SECONDS: i64 = 30 * 24 * 60 * 60;

// ============================================================================
// PDA Seeds
// ============================================================================

/// Seed for governance config PDA
pub const GOVERNANCE_SEED: &[u8] = b"governance";

/// Seed for feature proposal PDAs
pub const FEATURE_PROPOSAL_SEED: &[u8] = b"feature_proposal";

/// Seed for vote record PDAs
pub const VOTE_RECORD_SEED: &[u8] = b"vote_record";

/// Seed for research proposal PDAs
pub const RESEARCH_PROPOSAL_SEED: &[u8] = b"research_proposal";

/// Seed for budget gate proposal PDAs
pub const BUDGET_GATE_SEED: &[u8] = b"budget_gate";

/// Seed for allocation profile PDAs
pub const ALLOCATION_PROFILE_SEED: &[u8] = b"allocation_profile";

/// Seed for steward vote record PDAs
pub const STEWARD_VOTE_SEED: &[u8] = b"steward_vote";

/// Seed for steward record PDAs (registered steward registry)
pub const STEWARD_RECORD_SEED: &[u8] = b"steward_record";

// ============================================================================
// Validation Constants
// ============================================================================

/// Minimum bounty amount (in lamports or smallest token unit)
pub const MIN_BOUNTY_AMOUNT: u64 = 1_000_000; // 1 token with 6 decimals

/// Maximum bounty amount (in lamports or smallest token unit)
pub const MAX_BOUNTY_AMOUNT: u64 = 1_000_000_000_000; // 1 million tokens with 6 decimals

/// Minimum research stipend
pub const MIN_RESEARCH_STIPEND: u64 = 100_000; // 0.1 token with 6 decimals

/// Maximum research stipend
pub const MAX_RESEARCH_STIPEND: u64 = 100_000_000_000; // 100k tokens with 6 decimals

/// Minimum vote amount
pub const MIN_VOTE_AMOUNT: u64 = 1;

/// Maximum number of active proposals per proposer
pub const MAX_ACTIVE_PROPOSALS_PER_USER: u8 = 10;

// ============================================================================
// Account Space Calculations
// ============================================================================

/// Space for GovernanceConfig account
/// 8 (discriminator) + 32 (authority) + 32 (oracle) + 32 (mint) + 32 (treasury)
/// + 32 (params) + 8 (total_proposals) + 8 (total_votes) + 8 (total_bounties_paid)
/// + 1 (bump) + 128 (reserved)
pub const GOVERNANCE_CONFIG_SIZE: usize = 8 + 32 + 32 + 32 + 32 + 32 + 8 + 8 + 8 + 1 + 128;

/// Space for StoredGovernanceParams account
/// 8 (discriminator) + (16 * 2 bytes for u16 params) + 64 (reserved)
pub const GOVERNANCE_PARAMS_SIZE: usize = 8 + 32 + 64;

/// Space for FeatureProposal account
/// 8 (discriminator) + 8 (id) + 32 (proposer) + (64 + 4) (title) + (500 + 4) (description)
/// + 8 (estimated_bounty) + 8 (total_votes) + 1 (status) + (100 * 32 + 4) (customer_requests)
/// + 8 (created_at) + 8 (updated_at) + 8 (completed_at) + 4 * 16 (gate results) + 1 (bump) + 128 (reserved)
pub const FEATURE_PROPOSAL_SIZE: usize =
    8 + 8 + 32 + 68 + 504 + 8 + 8 + 1 + 3204 + 8 + 8 + 8 + 64 + 1 + 128;

/// Space for VoteRecord account
/// 8 (discriminator) + 32 (voter) + 8 (proposal_id) + 8 (amount) + 8 (voted_at) + 8 (withdrawn_at) + 1 (bump) + 64 (reserved)
pub const VOTE_RECORD_SIZE: usize = 8 + 32 + 8 + 8 + 8 + 8 + 1 + 64;

/// Space for ResearchProposal account
/// 8 (discriminator) + 8 (id) + 32 (proposer) + (64 + 4) (title) + (500 + 4) (description)
/// + 8 (stipend) + (10 * 100 + 4) (milestones) + 1 (status) + 8 (submitted_at) + 8 (approved_at)
/// + 1 (current_milestone) + 1 (bump) + 128 (reserved)
pub const RESEARCH_PROPOSAL_SIZE: usize =
    8 + 8 + 32 + 68 + 504 + 8 + 1004 + 1 + 8 + 8 + 1 + 1 + 128;

/// Space for BudgetGateProposal account
/// 8 (discriminator) + 8 (id) + 32 (proposer) + (64 + 4) (title) + 8 (new_budget_threshold)
/// + 8 (submitted_at) + 8 (executed_at) + 8 (yes_votes) + 8 (no_votes) + 1 (status) + 1 (bump) + 128 (reserved)
pub const BUDGET_GATE_SIZE: usize = 8 + 8 + 32 + 68 + 8 + 8 + 8 + 8 + 8 + 1 + 1 + 128;

/// Space for AllocationProfile account
/// 8 (discriminator) + 8 (id) + (64 + 4) (name) + 2 (feature_dev_bps) + 2 (research_bps)
/// + 2 (maintenance_bps) + 2 (security_bps) + 1 (active) + 8 (activated_at) + 1 (bump) + 64 (reserved)
pub const ALLOCATION_PROFILE_SIZE: usize = 8 + 8 + 68 + 2 + 2 + 2 + 2 + 1 + 8 + 1 + 64;

/// Space for StewardVoteRecord account
/// 8 (discriminator) + 32 (steward) + 8 (proposal_id) + 1 (vote_type) + 1 (approve) + 8 (voted_at) + 1 (bump) + 64 (reserved)
pub const STEWARD_VOTE_SIZE: usize = 8 + 32 + 8 + 1 + 1 + 8 + 1 + 64;

/// Space for StewardRecord account
/// 8 (discriminator) + 32 (steward) + 8 (registered_at) + 1 (active) + 1 (bump) + 64 (reserved)
pub const STEWARD_RECORD_SIZE: usize = 8 + 32 + 8 + 1 + 1 + 64;
