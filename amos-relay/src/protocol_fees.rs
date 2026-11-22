//! Protocol fee calculations and distribution.
//!
//! AMOS-only model: all transactions denominated in AMOS tokens.
//! Fee split must match on-chain constants in:
//!   - `amos-solana/programs/amos-treasury/src/constants.rs`
//!   - `amos-solana/programs/amos-bounty/src/constants.rs`
//!
//! Frozen amos-core previews are historical; docs/protocol/ECONOMIC_CONTRACT.md governs.

use serde::{Deserialize, Serialize};

/// Protocol fee percentage in basis points (3% = 300 bps).
pub const PROTOCOL_FEE_BPS: u64 = 300;

/// 50% of fee → staked token holders (claimable proportionally).
pub const FEE_HOLDER_SHARE_BPS: u64 = 5_000;

/// 40% of fee → permanently burned (deflationary).
pub const FEE_BURN_SHARE_BPS: u64 = 4_000;

/// 10% of fee → AMOS Labs operating wallet (in AMOS tokens).
pub const FEE_LABS_SHARE_BPS: u64 = 1_000;

/// Total basis points (100% = 10000 bps).
const TOTAL_BPS: u64 = 10_000;

/// Protocol fee breakdown.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProtocolFee {
    /// Total protocol fee (3% of reward).
    pub total_fee: u64,
    /// Amount allocated to holder pool (50% of fee).
    pub holder_share: u64,
    /// Amount allocated to burn (40% of fee).
    pub burn_share: u64,
    /// Amount allocated to AMOS Labs (10% of fee).
    pub labs_share: u64,
}

/// Calculate protocol fee and distribution breakdown.
///
/// # Arguments
/// * `reward_tokens` - Total reward amount in AMOS tokens
///
/// # Returns
/// Protocol fee breakdown with total fee and distribution shares.
/// Labs gets remainder after holder and burn shares to handle rounding.
///
/// # Example
/// ```
/// use amos_relay::protocol_fees::calculate_fee;
///
/// let fee = calculate_fee(1000);
/// assert_eq!(fee.total_fee, 30); // 3% of 1000
/// assert_eq!(fee.holder_share, 15); // 50% of 30
/// assert_eq!(fee.burn_share, 12); // 40% of 30
/// assert_eq!(fee.labs_share, 3); // 10% of 30 (remainder)
/// ```
pub fn calculate_fee(reward_tokens: u64) -> ProtocolFee {
    // Calculate total protocol fee (3%)
    let total_fee = (reward_tokens as u128 * PROTOCOL_FEE_BPS as u128 / TOTAL_BPS as u128) as u64;

    // Distribute the fee according to shares
    let holder_share =
        (total_fee as u128 * FEE_HOLDER_SHARE_BPS as u128 / TOTAL_BPS as u128) as u64;
    let burn_share = (total_fee as u128 * FEE_BURN_SHARE_BPS as u128 / TOTAL_BPS as u128) as u64;
    // Labs gets remainder to handle rounding
    let labs_share = total_fee - holder_share - burn_share;

    ProtocolFee {
        total_fee,
        holder_share,
        burn_share,
        labs_share,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn largest_amount_preserves_accounting() {
        let fee = calculate_fee(u64::MAX);
        assert_eq!(
            fee.holder_share + fee.burn_share + fee.labs_share,
            fee.total_fee
        );
        assert_eq!(fee.total_fee, (u64::MAX as u128 * 300 / 10_000) as u64);
    }

    #[test]
    fn test_fee_shares_sum_to_100_percent() {
        let total = FEE_HOLDER_SHARE_BPS + FEE_BURN_SHARE_BPS + FEE_LABS_SHARE_BPS;
        assert_eq!(total, TOTAL_BPS, "Fee shares must sum to 100%");
    }

    #[test]
    fn test_holder_share_is_50_percent() {
        assert_eq!(FEE_HOLDER_SHARE_BPS as f64 / TOTAL_BPS as f64, 0.50);
    }

    #[test]
    fn test_burn_share_is_40_percent() {
        assert_eq!(FEE_BURN_SHARE_BPS as f64 / TOTAL_BPS as f64, 0.40);
    }

    #[test]
    fn test_labs_share_is_10_percent() {
        assert_eq!(FEE_LABS_SHARE_BPS as f64 / TOTAL_BPS as f64, 0.10);
    }

    #[test]
    fn test_protocol_fee_is_3_percent() {
        assert_eq!(PROTOCOL_FEE_BPS, 300);
        assert_eq!(PROTOCOL_FEE_BPS as f64 / TOTAL_BPS as f64, 0.03);
    }

    #[test]
    fn test_calculate_fee_basic() {
        let fee = calculate_fee(1000);
        assert_eq!(fee.total_fee, 30); // 3% of 1000
        assert_eq!(fee.holder_share, 15); // 50% of 30
        assert_eq!(fee.burn_share, 12); // 40% of 30
        assert_eq!(fee.labs_share, 3); // remainder
    }

    #[test]
    fn test_calculate_fee_large_amount() {
        let fee = calculate_fee(1_000_000);
        assert_eq!(fee.total_fee, 30_000); // 3% of 1M
        assert_eq!(fee.holder_share, 15_000); // 50% of 30k
        assert_eq!(fee.burn_share, 12_000); // 40% of 30k
        assert_eq!(fee.labs_share, 3_000); // 10% of 30k
    }

    #[test]
    fn test_calculate_fee_zero() {
        let fee = calculate_fee(0);
        assert_eq!(fee.total_fee, 0);
        assert_eq!(fee.holder_share, 0);
        assert_eq!(fee.burn_share, 0);
        assert_eq!(fee.labs_share, 0);
    }

    #[test]
    fn test_calculate_fee_rounding() {
        let fee = calculate_fee(100);
        assert_eq!(fee.total_fee, 3); // 3% of 100
                                      // Labs gets remainder to handle rounding dust
        assert_eq!(
            fee.holder_share + fee.burn_share + fee.labs_share,
            fee.total_fee
        );
    }

    #[test]
    fn test_distribution_sums_to_total_fee() {
        // Test across many values that distribution never exceeds total fee
        for amount in [1, 10, 33, 100, 999, 1000, 10_000, 100_000, 1_000_000] {
            let fee = calculate_fee(amount);
            assert_eq!(
                fee.holder_share + fee.burn_share + fee.labs_share,
                fee.total_fee,
                "Distribution must sum to total fee for amount {}",
                amount
            );
        }
    }

    #[test]
    fn test_no_old_fee_split_references() {
        // Ensure old 70/20/10 split is gone
        assert_ne!(
            FEE_HOLDER_SHARE_BPS, 7000,
            "Old 70% holder share must be removed"
        );
    }
}
