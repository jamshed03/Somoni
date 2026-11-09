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
