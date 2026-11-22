//! Governance amounts are raw nine-decimal AMOS units. These helpers preserve
//! the existing staged-reward formulas; they do not apply the bounty emission cap.

use crate::constants::*;
use crate::errors::GovernanceError;
use anchor_lang::prelude::*;

pub(crate) fn validate_feature_bounty(amount: u64) -> Result<()> {
    require!(amount >= MIN_BOUNTY_AMOUNT, GovernanceError::BountyTooLow);
    require!(amount <= MAX_BOUNTY_AMOUNT, GovernanceError::BountyTooHigh);
    Ok(())
}

pub(crate) fn validate_research_stipend(amount: u64) -> Result<()> {
    require!(
        amount >= MIN_RESEARCH_STIPEND,
        GovernanceError::StipendTooLow
    );
    require!(
        amount <= MAX_RESEARCH_STIPEND,
        GovernanceError::StipendTooHigh
    );
    Ok(())
}

/// Floor each staged payment independently. Research success can exceed 100%;
/// widening before multiplication preserves the existing formula without wrapping.
pub(crate) fn reward_at_bps(amount: u64, bps: u16) -> Result<u64> {
    let reward = (amount as u128)
        .checked_mul(bps as u128)
        .ok_or(GovernanceError::RewardCalculationOverflow)?
        .checked_div(BPS_DENOMINATOR as u128)
        .ok_or(GovernanceError::DivisionByZero)?;
    u64::try_from(reward).map_err(|_| GovernanceError::RewardCalculationOverflow.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_submission_bounds_are_one_to_one_million_nine_decimal_amos() {
        assert_eq!(ONE_AMOS, 1_000_000_000);
        assert_eq!(MIN_BOUNTY_AMOUNT, 1_000_000_000);
        assert_eq!(MAX_BOUNTY_AMOUNT, 1_000_000_000_000_000);
        for amount in [0, 1_000_000, MIN_BOUNTY_AMOUNT - 1] {
            assert_eq!(
                validate_feature_bounty(amount),
                Err(GovernanceError::BountyTooLow.into())
            );
        }
        for amount in [
            MIN_BOUNTY_AMOUNT,
            MIN_BOUNTY_AMOUNT + 1,
            MAX_BOUNTY_AMOUNT - 1,
            MAX_BOUNTY_AMOUNT,
        ] {
            validate_feature_bounty(amount).unwrap();
        }
        for amount in [MAX_BOUNTY_AMOUNT + 1, u64::MAX] {
            assert_eq!(
                validate_feature_bounty(amount),
                Err(GovernanceError::BountyTooHigh.into())
            );
        }
    }

    #[test]
    fn research_submission_bounds_are_point_one_to_one_hundred_thousand_amos() {
        assert_eq!(MIN_RESEARCH_STIPEND, 100_000_000);
        assert_eq!(MAX_RESEARCH_STIPEND, 100_000_000_000_000);
        for amount in [0, 100_000, MIN_RESEARCH_STIPEND - 1] {
            assert_eq!(
                validate_research_stipend(amount),
                Err(GovernanceError::StipendTooLow.into())
            );
        }
        for amount in [
            MIN_RESEARCH_STIPEND,
            MIN_RESEARCH_STIPEND + 1,
            MAX_RESEARCH_STIPEND - 1,
            MAX_RESEARCH_STIPEND,
        ] {
            validate_research_stipend(amount).unwrap();
        }
        for amount in [MAX_RESEARCH_STIPEND + 1, u64::MAX] {
            assert_eq!(
                validate_research_stipend(amount),
                Err(GovernanceError::StipendTooHigh.into())
            );
        }
    }

    #[test]
    fn default_feature_stages_conserve_bounty_with_only_floor_dust() {
        let params = crate::state::StoredGovernanceParams::default();
        assert_eq!(
            (
                params.bounty_completion_bps,
                params.bounty_ab_bps,
                params.bounty_merge_bps
            ),
            (4000, 3000, 3000)
        );
        // Exercise every basis-point remainder at both newly corrected bounds.
        for offset in 0..10_000 {
            for amount in [MIN_BOUNTY_AMOUNT + offset, MAX_BOUNTY_AMOUNT - offset] {
                validate_feature_bounty(amount).unwrap();
                let stages = [
                    params.bounty_completion_bps,
                    params.bounty_ab_bps,
                    params.bounty_merge_bps,
                ]
                .map(|bps| reward_at_bps(amount, bps).unwrap());
                let paid: u64 = stages.into_iter().sum();
                assert!(paid <= amount);
                assert!(amount - paid <= 2);
                if offset == 0 {
                    assert_eq!(stages, [amount * 4 / 10, amount * 3 / 10, amount * 3 / 10]);
                    assert_eq!(paid, amount);
                }
            }
        }
    }

    #[test]
    fn default_research_uses_original_stipend_for_both_payments() {
        let params = crate::state::StoredGovernanceParams::default();
        assert_eq!(
            (params.research_stipend_bps, params.research_success_bps),
            (2000, 40000)
        );
        for amount in [
            MIN_RESEARCH_STIPEND,
            MIN_RESEARCH_STIPEND + 1,
            MAX_RESEARCH_STIPEND - 1,
            MAX_RESEARCH_STIPEND,
        ] {
            validate_research_stipend(amount).unwrap();
            let upfront = reward_at_bps(amount, params.research_stipend_bps).unwrap();
            let bonus = reward_at_bps(amount, params.research_success_bps).unwrap();
            assert_eq!(upfront, amount / 5);
            assert_eq!(bonus, amount * 4);
            let paid = upfront.checked_add(bonus).unwrap();
            // This path needs 4.2 times the recorded amount, not a 100% budget split.
            assert_eq!(paid, amount * 21 / 5);
            let treasury_before = MAX_RESEARCH_STIPEND * 5;
            assert_eq!((treasury_before - paid) + upfront + bonus, treasury_before);
        }
    }

    #[test]
    fn widened_reward_math_preserves_large_values_and_rejects_overflow() {
        assert_eq!(reward_at_bps(u64::MAX, 10_000).unwrap(), u64::MAX);
        assert_eq!(reward_at_bps(u64::MAX, 0).unwrap(), 0);
        assert_eq!(
            reward_at_bps(u64::MAX, 40_000),
            Err(GovernanceError::RewardCalculationOverflow.into())
        );
        assert_eq!(reward_at_bps(1, 3000).unwrap(), 0);
    }
}
