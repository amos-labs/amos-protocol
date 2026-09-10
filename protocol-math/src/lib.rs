#![no_std]
//! Economic contract v1. Integer results, rather than floating point previews,
//! determine settlement. This crate has no runtime, network or floating point
//! dependency and can be used by both the Relay and the Solana program.

pub const CONTRACT_VERSION: u16 = 1;
pub const TOKEN_DECIMALS: u8 = 9;
pub const TOKEN_UNIT: u64 = 1_000_000_000;
pub const SECONDS_PER_DAY: u64 = 86_400;
pub const VIRTUAL_POINTS: u64 = 10_000;
const SCALE: u128 = 1_000_000_000_000_000_000;
// round(exp(-1) * 10^18), fixed as part of the versioned integer contract.
const EXP_MINUS_ONE: u128 = 367_879_441_171_442_322;

fn scaled_pow(mut base: u128, mut exponent: u32) -> u128 {
    let mut value = SCALE;
    while exponent > 0 {
        if exponent & 1 == 1 {
            value = value * base / SCALE;
        }
        base = base * base / SCALE;
        exponent >>= 1;
    }
    value
}

/// exp(-x), scaled by 10^18, for x in ten-thousandths. Reducing the
/// exponent before a bounded Taylor series avoids an overflowing exp(+x).
fn exp_negative(x: u32) -> u128 {
    if x >= 450_000 {
        return 0;
    }
    let whole = x / 10_000;
    let fraction = (x % 10_000) as u128;
    let mut sum = SCALE;
    let mut term = SCALE;
    for n in 1..=20 {
        term = term * fraction / (10_000 * n);
        sum += term;
    }
    let fractional_inverse = SCALE * SCALE / sum;
    scaled_pow(EXP_MINUS_ONE, whole) * fractional_inverse / SCALE
}

/// floor + (ceiling-floor)/(1+exp(k*(day-midpoint))). `k_scaled` is k*10,000.
/// Every multiplication is bounded in u128. There is no e^6 tail plateau,
/// hundredth-day truncation, or u64-to-i64 timestamp wrap.
pub fn sigmoid(day: u64, ceiling: u64, floor: u64, midpoint: u64, k_scaled: u64) -> u64 {
    let floor = floor.min(ceiling);
    let range = (ceiling - floor) as u128;
    let distance = day.abs_diff(midpoint) as u128;
    let exponent = distance.saturating_mul(k_scaled as u128).min(450_000) as u32;
    let inverse = exp_negative(exponent);
    let numerator = if day >= midpoint { inverse } else { SCALE };
    floor + (range * numerator / (SCALE + inverse)) as u64
}

pub fn daily_emission(day: u64) -> u64 {
    sigmoid(day, 16_000 * TOKEN_UNIT, 100 * TOKEN_UNIT, 1_460, 50)
}

pub fn growth_cap_bps(day: u64) -> u16 {
    sigmoid(day, 2_000, 300, 540, 100) as u16
}

/// A sequential, time-dripped upper bound. This is deliberately not described
/// as an end-of-day pro-rata auction: order still matters. Zero means no payout.
/// No per-submission token floor is applied, which would reward task splitting.
pub fn reward_cap(
    points: u64,
    emission: u64,
    distributed: u64,
    prior_points: u64,
    day_start: i64,
    now: i64,
) -> u64 {
    if points == 0 || emission == 0 || now <= day_start {
        return 0;
    }
    let elapsed = (now as i128 - day_start as i128).min(SECONDS_PER_DAY as i128) as u128;
    let released = emission as u128 * elapsed / SECONDS_PER_DAY as u128;
    let available = released.saturating_sub(distributed as u128);
    let denominator = prior_points as u128 + VIRTUAL_POINTS as u128 + points as u128;
    (points as u128 * available / denominator) as u64
}

/// Authorised decay on an operator-owned balance. Full days begin after the
/// inactivity grace. Repeated calls cannot recharge a day or the grace period.
/// The rate is nominal annual simple accrual for this interval. Rounding is
/// downward in raw token units; there is no upward one-token minimum.
pub fn decay_amount(
    balance: u64,
    original: u64,
    already_decayed: u64,
    annual_bps: u16,
    last_activity: i64,
    last_decay: i64,
    now: i64,
) -> Option<(u64, i64)> {
    if !(200..=2_500).contains(&annual_bps) || balance == 0 {
        return None;
    }
    let grace_end = last_activity.checked_add(90 * SECONDS_PER_DAY as i64)?;
    let accrual_start = last_decay.max(grace_end);
    let elapsed = now.checked_sub(accrual_start)?;
    if elapsed < SECONDS_PER_DAY as i64 {
        return None;
    }
    let days = elapsed as u64 / SECONDS_PER_DAY;
    let floor = original as u128 * 1_000 / 10_000;
    let remaining = (original as u128).checked_sub(already_decayed as u128)?;
    let capacity = remaining.saturating_sub(floor).min(balance as u128);
    let amount =
        (balance as u128 * annual_bps as u128 * days as u128 / (10_000 * 365)).min(capacity) as u64;
    if amount == 0 {
        return None;
    }
    let accounted_through = accrual_start.checked_add((days * SECONDS_PER_DAY) as i64)?;
    Some((amount, accounted_through))
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn known_curve_values_and_extreme_tails() {
        assert_eq!(daily_emission(1_460), 8_050 * TOKEN_UNIT);
        assert_eq!(growth_cap_bps(540), 1_150);
        assert_eq!(growth_cap_bps(10_000), 300);
        assert_eq!(daily_emission(u64::MAX), 100 * TOKEN_UNIT);
        assert_eq!(sigmoid(0, 16_000, 100, u64::MAX, u64::MAX), 16_000);
        assert!(daily_emission(3_650) < 101 * TOKEN_UNIT);
        // These independently calculated continuous-form values are rounded to AMOS.
        assert_eq!(daily_emission(0) / TOKEN_UNIT, 15_989);
        assert_eq!(daily_emission(365) / TOKEN_UNIT, 15_933);
    }

    #[test]
    fn daily_curve_is_monotone_and_matches_continuous_reference() {
        let mut previous = daily_emission(0);
        for day in 0..=20_000 {
            let actual = daily_emission(day);
            assert!(actual <= previous);
            assert!((100 * TOKEN_UNIT..=16_000 * TOKEN_UNIT).contains(&actual));
            let expected = 100.0 + 15_900.0 / (1.0 + (0.005 * (day as f64 - 1460.0)).exp());
            assert!((actual as f64 / TOKEN_UNIT as f64 - expected).abs() < 0.000001);
            previous = actual;
        }
    }

    #[test]
    fn reward_never_exceeds_released_pool_and_zero_is_zero() {
        let emission = 16_000 * TOKEN_UNIT;
        assert_eq!(reward_cap(0, emission, 0, 0, 0, 1), 0);
        assert_eq!(reward_cap(1, emission, 0, 0, 100, 99), 0);
        assert_eq!(reward_cap(1, emission, 0, 0, 100, 100), 0);
        let cap = reward_cap(2_000, emission, 0, 0, 0, 43_200);
        assert_eq!(
            cap,
            (2_000u128 * 8_000 * TOKEN_UNIT as u128 / 12_000) as u64
        );
        assert!(cap < emission / 2);
        assert_eq!(reward_cap(1, 1, 0, 0, 0, 86_400), 0);
        assert_eq!(reward_cap(1_000, emission, emission, 0, 0, 86_400), 0);
        assert_eq!(
            reward_cap(1_000, emission, 0, 0, 0, i64::MAX),
            reward_cap(1_000, emission, 0, 0, 0, 86_400)
        );
        assert!(reward_cap(u64::MAX, u64::MAX, 0, u64::MAX, i64::MIN, i64::MAX) <= u64::MAX / 2);
    }

    #[test]
    fn grace_repeated_calls_rounding_and_floor_are_bounded() {
        let day = SECONDS_PER_DAY as i64;
        let balance = 1_000 * TOKEN_UNIT;
        assert_eq!(decay_amount(balance, balance, 0, 500, 0, 0, 90 * day), None);
        let (amount, stamp) = decay_amount(balance, balance, 0, 500, 0, 0, 91 * day).unwrap();
        assert_eq!(amount, balance * 500 / (10_000 * 365));
        assert_eq!(stamp, 91 * day);
        assert_eq!(
            decay_amount(balance - amount, balance, amount, 500, 0, stamp, stamp),
            None
        );
        assert_eq!(decay_amount(1, 1, 0, 500, 0, 0, 91 * day), None);
        let (maximum, _) = decay_amount(balance, balance, 0, 2_500, 0, 0, 10_000 * day).unwrap();
        assert_eq!(maximum, 900 * TOKEN_UNIT);
        assert_eq!(
            decay_amount(balance, balance, 0, 5_000, 0, 0, 91 * day),
            None
        );
        assert_eq!(decay_amount(balance, balance, 0, 500, 0, 0, -1), None);
    }
}
