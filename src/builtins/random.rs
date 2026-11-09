//! `random(min, max)`: whole numbers that look random, from a small generator (SplitMix64) started from the clock.
//! Good for games, examples and tests, not for passwords: whoever sees enough numbers can work out the next ones.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::SomoniError;
use crate::value::Value;

use super::Builtin;
use super::math::no_result;

/// The state of the generator; every number moves it on.
pub struct Random {
    state: u64,
}

impl Default for Random {
    /// Starts from the nanoseconds of the clock, so every run gets other numbers.
    fn default() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos() as u64);
        Random { state: nanos }
    }
}

impl Random {
    /// Returns the next 64 bits: the state moves on by a fixed odd step, then its bits are mixed (SplitMix64).
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut bits = self.state;
        bits = (bits ^ (bits >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        bits = (bits ^ (bits >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        bits ^ (bits >> 31)
    }

    /// Returns an int from `min` to `max`, both included, or R010 if `min` is greater than `max`.
    pub fn between(&mut self, min: i64, max: i64) -> Result<Value, SomoniError> {
        if min > max {
            return Err(no_result(Builtin::Random));
        }
        let count = (i128::from(max) - i128::from(min) + 1) as u128;
        let offset = u128::from(self.next()) % count;
        Ok(Value::Int((i128::from(min) + offset as i128) as i64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns a generator that starts from `state` instead of the clock, so its numbers are always the same.
    fn seeded(state: u64) -> Random {
        Random { state }
    }

    /// Returns the int inside `value`.
    fn int(value: Value) -> i64 {
        match value {
            Value::Int(n) => n,
            other => panic!("no int: {:?}", other),
        }
    }

    #[test]
    fn the_numbers_are_those_of_splitmix64() {
        let mut random = seeded(0);
        let first: Vec<u64> = (0..3).map(|_| random.next()).collect();
        assert_eq!(
            first,
            vec![
                0xE220_A839_7B1D_CDAF,
                0x6E78_9E6A_A1B9_65F4,
                0x06C4_5D18_8009_454F
            ]
        );
    }

    #[test]
    fn every_number_lies_between_min_and_max() {
        let mut random = seeded(42);
        for _ in 0..1000 {
            let n = int(random.between(-3, 3).unwrap());
            assert!((-3..=3).contains(&n), "{}", n);
        }
    }

    #[test]
    fn the_whole_range_of_int_does_not_overflow() {
        let mut random = seeded(7);
        for _ in 0..100 {
            random.between(i64::MIN, i64::MAX).unwrap();
        }
        assert_eq!(int(random.between(i64::MAX, i64::MAX).unwrap()), i64::MAX);
        assert_eq!(int(random.between(i64::MIN, i64::MIN).unwrap()), i64::MIN);
    }

    #[test]
    fn min_greater_than_max_is_r010() {
        let error = seeded(1).between(6, 1).unwrap_err();
        assert_eq!(error.code.code(), "R010");
    }
}
