/// AMOS Treasury Constants
///
/// These constants define the immutable rules of the AMOS Treasury system.
/// ALL transactions are denominated in AMOS tokens. No USDC track.
/// Fee distribution percentages are hardcoded to ensure trust and transparency.
use anchor_lang::prelude::*;

// ============================================================================
// Protocol Fee — AMOS-Only (Applied to commercial bounties)
// ============================================================================

/// Protocol fee rate: 3% of commercial bounty payout
pub const PROTOCOL_FEE_BPS: u16 = 300;

/// 50% of fee → staked token holders (claimable proportionally)
pub const FEE_HOLDER_SHARE_BPS: u16 = 5000;

/// 40% of fee → permanently burned (deflationary)
pub const FEE_BURN_SHARE_BPS: u16 = 4000;

/// 10% of fee → AMOS Labs operating wallet (in AMOS tokens)
pub const FEE_LABS_SHARE_BPS: u16 = 1000;

// ============================================================================
// Staking Requirements
// ============================================================================

/// Minimum stake period in days before claiming revenue
/// Prevents gaming the system with short-term stakes
pub const MIN_STAKE_DAYS: u64 = 30;

/// Minimum AMOS tokens required to register a stake
pub const MIN_STAKE_AMOUNT: u64 = 100;

// ============================================================================
// Basis Points Denominator
// ============================================================================

/// Denominator for all basis point calculations
/// 10000 basis points = 100%
pub const BPS_DENOMINATOR: u16 = 10000;

// ============================================================================
// PDA Seeds Module
// ============================================================================

pub mod seeds {
    /// Seed for treasury config PDA
    pub const TREASURY_CONFIG: &[u8] = b"treasury_config";

    /// Seed for stake record PDA
    pub const STAKE_RECORD: &[u8] = b"stake_record";

    /// Seed for distribution record PDA
    pub const DISTRIBUTION: &[u8] = b"distribution";

    /// Seed for holder pool PDA
    pub const HOLDER_POOL: &[u8] = b"holder_pool";

    /// Seed for treasury AMOS account
    pub const TREASURY_AMOS: &[u8] = b"treasury_amos";

    /// Seed for reserve vault
    pub const RESERVE_VAULT: &[u8] = b"reserve_vault";

    /// Seed for bounty escrow PDA
    pub const BOUNTY_ESCROW: &[u8] = b"bounty_escrow";
}

// ============================================================================
// Compile-Time Validation Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_shares_sum_to_100_percent() {
        let total = FEE_HOLDER_SHARE_BPS + FEE_BURN_SHARE_BPS + FEE_LABS_SHARE_BPS;
        assert_eq!(
            total, BPS_DENOMINATOR,
            "Fee shares must sum to exactly 10000 basis points (100%)"
        );
    }

    #[test]
    fn fee_share_percentages() {
        assert_eq!(FEE_HOLDER_SHARE_BPS, 5000, "Holder share should be 50%");
        assert_eq!(FEE_BURN_SHARE_BPS, 4000, "Burn share should be 40%");
        assert_eq!(FEE_LABS_SHARE_BPS, 1000, "Labs share should be 10%");
    }

    #[test]
    fn protocol_fee_is_3_percent() {
        assert_eq!(PROTOCOL_FEE_BPS, 300, "Protocol fee should be 3%");
    }

    #[test]
    fn minimum_stake_requirements() {
        assert!(
            MIN_STAKE_DAYS >= 30,
            "Minimum stake period should be at least 30 days"
        );
        assert!(
            MIN_STAKE_AMOUNT >= 100,
            "Minimum stake amount should be at least 100 tokens"
        );
    }
}
