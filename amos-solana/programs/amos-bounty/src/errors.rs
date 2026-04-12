/// AMOS Bounty Program Error Codes
///
/// All custom errors for the bounty distribution program.
/// These provide clear feedback when operations fail validation.

use anchor_lang::prelude::*;

#[error_code]
pub enum BountyError {
    #[msg("Unauthorized: Only the oracle authority can perform this action")]
    Unauthorized,

    #[msg("Invalid decay rate: Must be between 2% and 25% annually")]
    InvalidDecayRate,

    #[msg("Halving not yet available: 365 days must pass between halvings")]
    HalvingNotAvailable,

    #[msg("Maximum halvings reached: Already at minimum emission rate")]
    MaxHalvingsReached,

    #[msg("Insufficient emission: Not enough tokens remain in daily pool")]
    InsufficientEmission,

    #[msg("Quality score too low: Must be at least 30 out of 100")]
    QualityScoreTooLow,

    #[msg("Invalid bounty points: Exceeds maximum or trust level limit")]
    InvalidBountyPoints,

    #[msg("Daily limit exceeded: Too many bounties submitted today")]
    DailyLimitExceeded,

    #[msg("Invalid contribution type: Must be 0-7")]
    InvalidContributionType,

    #[msg("Decay grace period active: Cannot apply decay before 90 days")]
    DecayGracePeriodActive,

    #[msg("Decay floor reached: Cannot decay below 10% of original allocation")]
    DecayFloorReached,

    #[msg("No decay applicable: No tokens to decay")]
    NoDecayApplicable,

    #[msg("Invalid trust level: Must be between 1 and 5")]
    InvalidTrustLevel,

    #[msg("Trust upgrade not available: Requirements not met")]
    TrustUpgradeNotAvailable,

    #[msg("Already at maximum trust level")]
    AlreadyMaxTrustLevel,

    #[msg("Agent not registered: Must register before recording completions")]
    AgentNotRegistered,

    #[msg("Arithmetic overflow: Calculation exceeded maximum value")]
    ArithmeticOverflow,

    #[msg("Arithmetic underflow: Calculation went below zero")]
    ArithmeticUnderflow,

    #[msg("Invalid timestamp: Time value is not valid")]
    InvalidTimestamp,

    #[msg("Daily pool not finalized: Cannot proceed until pool is closed")]
    DailyPoolNotFinalized,

    #[msg("Daily pool already finalized: Cannot modify after finalization")]
    DailyPoolAlreadyFinalized,

    #[msg("Invalid day index: Day value is out of valid range")]
    InvalidDayIndex,

    #[msg("Bounty already exists: This bounty ID is already registered")]
    BountyAlreadyExists,

    #[msg("Invalid operator: Operator address mismatch or not authorized")]
    InvalidOperator,

    #[msg("Invalid agent ID: Agent identifier is malformed")]
    InvalidAgentId,

    #[msg("Reviewer same as operator: Reviewer must be different from operator")]
    ReviewerSameAsOperator,

    #[msg("Invalid evidence hash: Evidence hash cannot be empty")]
    InvalidEvidenceHash,

    #[msg("Zero points awarded: Bounty must award at least 1 point")]
    ZeroPointsAwarded,

    #[msg("Zero tokens calculated: Token distribution cannot be zero")]
    ZeroTokensCalculated,

    #[msg("Treasury insufficient funds: Not enough tokens in treasury")]
    TreasuryInsufficientFunds,

    #[msg("Token transfer failed: SPL token transfer encountered an error")]
    TokenTransferFailed,

    #[msg("Invalid mint: Token mint does not match configuration")]
    InvalidMint,

    #[msg("Invalid treasury: Treasury account does not match configuration")]
    InvalidTreasury,

    #[msg("Program not initialized: Must call initialize first")]
    ProgramNotInitialized,

    #[msg("Program already initialized: Cannot initialize twice")]
    ProgramAlreadyInitialized,

    #[msg("Invalid bump seed: PDA derivation failed")]
    InvalidBumpSeed,

    #[msg("Account already exists: Cannot create duplicate account")]
    AccountAlreadyExists,

    #[msg("Account not found: Required account does not exist")]
    AccountNotFound,

    #[msg("Invalid account owner: Account is owned by wrong program")]
    InvalidAccountOwner,

    #[msg("Invalid account data: Account data is corrupted or malformed")]
    InvalidAccountData,

    #[msg("Invalid bounty source: Must be Treasury or Commercial")]
    InvalidBountySource,

    #[msg("Escrow not funded: Commercial bounty escrow has insufficient balance")]
    EscrowNotFunded,

    #[msg("Escrow already released: Cannot release funds twice")]
    EscrowAlreadyReleased,

    #[msg("Escrow expired: Bounty deadline has passed without completion")]
    EscrowExpired,

    #[msg("Escrow not expired: Cannot refund before deadline")]
    EscrowNotExpired,

    #[msg("Invalid escrow: Escrow account does not match bounty proof")]
    InvalidEscrow,

    #[msg("Invalid poster: Only the original poster can request a refund")]
    InvalidPoster,

    #[msg("Invalid labs wallet: Labs wallet does not match configuration")]
    InvalidLabsWallet,

    #[msg("Invalid holder pool: Holder pool does not match configuration")]
    InvalidHolderPool,

    #[msg("Fee recipients not set: Must call set_fee_recipients before releasing commercial bounties")]
    FeeRecipientsNotSet,

    #[msg("Escrow below minimum: Commercial bounties require a minimum escrow amount")]
    EscrowBelowMinimum,

    #[msg("Vault lockup active: Cannot withdraw until lockup period expires")]
    VaultLockupActive,

    #[msg("Invalid vault tier: Must be 0-4")]
    InvalidVaultTier,

    #[msg("Metrics update too frequent: Must wait before updating again")]
    MetricsUpdateTooFrequent,
}
