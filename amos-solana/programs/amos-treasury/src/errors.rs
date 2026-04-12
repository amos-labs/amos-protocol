/// AMOS Treasury Error Codes
///
/// Comprehensive error handling for all treasury operations.
/// Clear error messages help with debugging and user experience.
use anchor_lang::prelude::*;

#[error_code]
pub enum TreasuryError {
    // ============================================================================
    // Initialization Errors (6000-6009)
    // ============================================================================
    #[msg("Treasury has already been initialized")]
    AlreadyInitialized,

    #[msg("Invalid Labs wallet address provided")]
    InvalidLabsWallet,

    #[msg("Invalid mint address provided")]
    InvalidMint,

    // ============================================================================
    // Stake Registration Errors (6010-6019)
    // ============================================================================
    #[msg("Stake amount is below minimum required (100 AMOS)")]
    StakeAmountTooLow,

    #[msg("Stake amount cannot be zero")]
    ZeroStakeAmount,

    #[msg("Stake record already exists for this user")]
    StakeAlreadyExists,

    #[msg("Stake record does not exist")]
    StakeNotFound,

    #[msg("Cannot reduce stake below minimum required amount")]
    StakeBelowMinimum,

    #[msg("Insufficient AMOS tokens to register stake")]
    InsufficientStakeTokens,

    // ============================================================================
    // Claim Errors (6020-6029)
    // ============================================================================
    #[msg("Minimum stake period of 30 days has not been met")]
    MinimumStakePeriodNotMet,

    #[msg("No claimable revenue available")]
    NoClaimableRevenue,

    #[msg("Insufficient funds in holder pool to process claim")]
    InsufficientHolderPoolFunds,

    #[msg("Claim amount exceeds available balance")]
    ClaimExceedsBalance,

    #[msg("Invalid claim calculation")]
    InvalidClaimCalculation,

    // ============================================================================
    // Revenue Distribution Errors (6030-6039)
    // ============================================================================
    #[msg("Revenue amount cannot be zero")]
    ZeroRevenueAmount,

    #[msg("Revenue split calculation error")]
    RevenueSplitError,

    #[msg("Distribution record creation failed")]
    DistributionCreationFailed,

    #[msg("Invalid distribution type")]
    InvalidDistributionType,

    #[msg("Payment reference is required")]
    MissingPaymentReference,

    #[msg("Payment reference too long (max 64 characters)")]
    PaymentReferenceTooLong,

    // ============================================================================
    // Arithmetic Errors (6040-6049)
    // ============================================================================
    #[msg("Arithmetic overflow in calculation")]
    ArithmeticOverflow,

    #[msg("Arithmetic underflow in calculation")]
    ArithmeticUnderflow,

    #[msg("Division by zero")]
    DivisionByZero,

    #[msg("Invalid percentage calculation")]
    InvalidPercentage,

    #[msg("Rounding error in distribution")]
    RoundingError,

    // ============================================================================
    // Authorization Errors (6050-6059)
    // ============================================================================
    #[msg("Unauthorized: Only treasury authority can perform this action")]
    Unauthorized,

    #[msg("Invalid authority provided")]
    InvalidAuthority,

    #[msg("Signer is not the stake owner")]
    NotStakeOwner,

    #[msg("Invalid treasury configuration")]
    InvalidTreasuryConfig,

    // ============================================================================
    // Token Transfer Errors (6060-6069)
    // ============================================================================
    #[msg("Token transfer failed")]
    TokenTransferFailed,

    #[msg("Token mint operation failed")]
    TokenMintFailed,

    #[msg("Token burn operation failed")]
    TokenBurnFailed,

    #[msg("Insufficient token balance")]
    InsufficientTokenBalance,

    #[msg("Invalid token account")]
    InvalidTokenAccount,

    #[msg("Token account mint mismatch")]
    TokenMintMismatch,

    // ============================================================================
    // PDA Errors (6070-6079)
    // ============================================================================
    #[msg("PDA derivation failed")]
    PDADerivationFailed,

    #[msg("Invalid PDA bump seed")]
    InvalidBumpSeed,

    #[msg("PDA address mismatch")]
    PDAAddressMismatch,

    // ============================================================================
    // State Validation Errors (6080-6089)
    // ============================================================================
    #[msg("Treasury state is invalid or corrupted")]
    InvalidTreasuryState,

    #[msg("Stake record state is invalid")]
    InvalidStakeState,

    #[msg("Distribution record state is invalid")]
    InvalidDistributionState,

    #[msg("Holder pool state is invalid")]
    InvalidHolderPoolState,

    #[msg("Total stake amount mismatch")]
    TotalStakeMismatch,

    // ============================================================================
    // Timestamp Errors (6090-6099)
    // ============================================================================
    #[msg("Invalid timestamp")]
    InvalidTimestamp,

    #[msg("Timestamp is in the future")]
    FutureTimestamp,

    #[msg("Clock not available")]
    ClockUnavailable,

    // ============================================================================
    // Query Errors (6100-6109)
    // ============================================================================
    #[msg("Invalid query limit (max 100)")]
    InvalidQueryLimit,

    #[msg("Distribution index out of bounds")]
    DistributionIndexOutOfBounds,

    #[msg("No distribution history available")]
    NoDistributionHistory,

    // ============================================================================
    // Entity & Lockup Errors (6110-6119)
    // ============================================================================
    #[msg("Entity tokens are still locked")]
    EntityTokensLocked,

    #[msg("Invalid lockup period")]
    InvalidLockupPeriod,

    #[msg("Lockup has not expired")]
    LockupNotExpired,

    // ============================================================================
    // General Errors (6120-6129)
    // ============================================================================
    #[msg("Invalid program configuration")]
    InvalidProgramConfig,

    #[msg("Feature not implemented")]
    NotImplemented,

    #[msg("Operation not allowed in current state")]
    OperationNotAllowed,

    #[msg("Internal program error")]
    InternalError,

    #[msg("Invalid input parameter")]
    InvalidInput,
}
