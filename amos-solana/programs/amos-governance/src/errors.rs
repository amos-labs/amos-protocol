// AMOS Governance Program - Error Codes
// Defines all custom errors for the governance program

use anchor_lang::prelude::*;

#[error_code]
pub enum GovernanceError {
    // ========================================================================
    // Authorization Errors (6000-6009)
    // ========================================================================
    #[msg("Unauthorized: Only the governance authority can perform this action")]
    Unauthorized,

    #[msg("Unauthorized: Only the oracle can perform this action")]
    OracleOnly,

    #[msg("Unauthorized: Only the proposal owner can perform this action")]
    NotProposalOwner,

    #[msg("Unauthorized: Only approved stewards can perform this action")]
    NotSteward,

    // ========================================================================
    // Proposal State Errors (6010-6029)
    // ========================================================================
    #[msg("Invalid proposal status for this operation")]
    InvalidProposalStatus,

    #[msg("Proposal has expired")]
    ProposalExpired,

    #[msg("Proposal is not active")]
    ProposalNotActive,

    #[msg("Proposal is already finalized")]
    ProposalAlreadyFinalized,

    #[msg("Proposal is cancelled")]
    ProposalCancelled,

    #[msg("Proposal not found")]
    ProposalNotFound,

    #[msg("User has reached maximum active proposals limit")]
    TooManyActiveProposals,

    #[msg("Proposal has not been approved yet")]
    ProposalNotApproved,

    #[msg("Proposal is still pending")]
    ProposalStillPending,

    // ========================================================================
    // Voting Errors (6030-6049)
    // ========================================================================
    #[msg("Vote already exists for this proposal")]
    VoteAlreadyExists,

    #[msg("Vote does not exist")]
    VoteNotFound,

    #[msg("Cannot withdraw vote during lock period")]
    VoteLocked,

    #[msg("Vote amount is below minimum")]
    VoteAmountTooLow,

    #[msg("Vote amount is above maximum")]
    VoteAmountTooHigh,

    #[msg("Insufficient token balance to vote")]
    InsufficientBalance,

    #[msg("Vote already withdrawn")]
    VoteAlreadyWithdrawn,

    #[msg("Cannot vote on own proposal")]
    CannotVoteOnOwnProposal,

    // ========================================================================
    // Quality Gate Errors (6050-6069)
    // ========================================================================
    #[msg("Gate has already been evaluated")]
    GateAlreadyEvaluated,

    #[msg("Gate has not been evaluated yet")]
    GateNotEvaluated,

    #[msg("Gate threshold not met")]
    GateThresholdNotMet,

    #[msg("Benchmark gate failed")]
    BenchmarkGateFailed,

    #[msg("A/B test gate failed")]
    ABTestGateFailed,

    #[msg("Feedback gate failed")]
    FeedbackGateFailed,

    #[msg("Steward approval gate failed")]
    StewardApprovalFailed,

    #[msg("All required gates must pass before claiming this reward")]
    RequiredGatesNotPassed,

    #[msg("Invalid gate type")]
    InvalidGateType,

    // ========================================================================
    // Reward Errors (6070-6089)
    // ========================================================================
    #[msg("Reward already claimed for this gate")]
    RewardAlreadyClaimed,

    #[msg("Cannot claim reward: gate not passed")]
    CannotClaimRewardGateNotPassed,

    #[msg("Bounty amount exceeds maximum allowed")]
    BountyTooHigh,

    #[msg("Bounty amount below minimum allowed")]
    BountyTooLow,

    #[msg("Insufficient treasury funds")]
    InsufficientTreasuryFunds,

    #[msg("Reward calculation overflow")]
    RewardCalculationOverflow,

    #[msg("All rewards already finalized")]
    AllRewardsFinalized,

    // ========================================================================
    // Research Errors (6090-6109)
    // ========================================================================
    #[msg("Research proposal not approved")]
    ResearchNotApproved,

    #[msg("Research milestone already completed")]
    MilestoneAlreadyCompleted,

    #[msg("Invalid milestone index")]
    InvalidMilestoneIndex,

    #[msg("Previous milestone not completed")]
    PreviousMilestoneNotCompleted,

    #[msg("Research stipend too low")]
    StipendTooLow,

    #[msg("Research stipend too high")]
    StipendTooHigh,

    #[msg("Too many milestones")]
    TooManyMilestones,

    #[msg("Research already graduated")]
    ResearchAlreadyGraduated,

    #[msg("Not all milestones completed")]
    NotAllMilestonesCompleted,

    // ========================================================================
    // Validation Errors (6110-6129)
    // ========================================================================
    #[msg("Title exceeds maximum length")]
    TitleTooLong,

    #[msg("Description exceeds maximum length")]
    DescriptionTooLong,

    #[msg("Too many customer requests linked")]
    TooManyCustomerRequests,

    #[msg("Invalid parameter value")]
    InvalidParameter,

    #[msg("Parameter sum does not equal 100%")]
    InvalidParameterSum,

    #[msg("Invalid timestamp")]
    InvalidTimestamp,

    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,

    #[msg("Arithmetic underflow")]
    ArithmeticUnderflow,

    #[msg("Division by zero")]
    DivisionByZero,

    // ========================================================================
    // Budget Gate Errors (6130-6149)
    // ========================================================================
    #[msg("Budget gate proposal already exists")]
    BudgetGateAlreadyExists,

    #[msg("Budget gate voting period ended")]
    BudgetGateVotingEnded,

    #[msg("Budget gate not approved")]
    BudgetGateNotApproved,

    #[msg("Steward already voted on this proposal")]
    StewardAlreadyVoted,

    #[msg("Insufficient steward votes to execute")]
    InsufficientStewardVotes,

    #[msg("Budget threshold must be positive")]
    InvalidBudgetThreshold,

    #[msg("Allocation profile not found")]
    AllocationProfileNotFound,

    #[msg("Allocation percentages do not sum to 100%")]
    InvalidAllocationSum,

    #[msg("Cannot activate inactive profile")]
    ProfileNotActive,

    // ========================================================================
    // General Errors (6150-6169)
    // ========================================================================
    #[msg("Feature not implemented yet")]
    NotImplemented,

    #[msg("Invalid account provided")]
    InvalidAccount,

    #[msg("Account already initialized")]
    AlreadyInitialized,

    #[msg("Account not initialized")]
    NotInitialized,

    #[msg("Invalid PDA derivation")]
    InvalidPDA,

    #[msg("Invalid mint account")]
    InvalidMint,

    #[msg("Operation would result in negative value")]
    NegativeValue,
}
