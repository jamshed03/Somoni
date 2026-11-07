//! The math functions `abs`, `min`, `max`, `sqrt`, `pow`, `floor`, `ceil` and `round`.

use crate::error::{ErrorCode, SomoniError};
use crate::value::Value;

use super::Builtin;
use super::convert::float_to_int;

/// Returns `n` without its sign, or R002 for the smallest int, whose positive value does not fit.
pub(super) fn abs_int(n: i64) -> Result<Value, SomoniError> {
    n.checked_abs()
        .map(Value::Int)
        .ok_or_else(|| ErrorCode::IntegerOverflow.into())
}

/// Returns the square root of `f`, or R010 for a negative number.
pub(super) fn sqrt(f: f64) -> Result<Value, SomoniError> {
    if f < 0.0 {
        return Err(no_result(Builtin::Sqrt));
    }
    Ok(Value::Float(f.sqrt()))
}

/// Returns `base` to the power of `exponent`, or R010 for a negative exponent and R002 if the result is too big.
pub(super) fn pow_int(base: i64, exponent: i64) -> Result<Value, SomoniError> {
    let exponent = u32::try_from(exponent).map_err(|_| no_result(Builtin::Pow))?;
    base.checked_pow(exponent)
        .map(Value::Int)
        .ok_or_else(|| ErrorCode::IntegerOverflow.into())
}

/// Returns `base` to the power of `exponent`, or R010 if that is no finite number.
pub(super) fn pow_float(base: f64, exponent: f64) -> Result<Value, SomoniError> {
    let result = base.powf(exponent);
    if !result.is_finite() {
        return Err(no_result(Builtin::Pow));
    }
    Ok(Value::Float(result))
}

/// Rounds `f` with `round` (like `f64::floor`) and turns it into an int, or R002 if it does not fit.
pub(super) fn to_int(f: f64, round: fn(f64) -> f64) -> Result<Value, SomoniError> {
    float_to_int(round(f))
}

/// The error R010 for a built-in function that has no result for its arguments.
pub(super) fn no_result(builtin: Builtin) -> SomoniError {
    ErrorCode::NoMathResult {
        function: builtin.name(),
    }
    .into()
}
