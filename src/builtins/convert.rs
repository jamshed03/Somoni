//! The conversions `int(...)` and `float(...)`, which give a `result` for a text that is no number.

use std::rc::Rc;

use crate::error::{ErrorCode, SomoniError};
use crate::types::Type;
use crate::value::Value;

use super::string_value;

/// The first float that is too big for an int: 2 to the power of 63.
const INT_LIMIT: f64 = 9_223_372_036_854_775_808.0;

/// Cuts a float to an int (`int(-3.9)` is -3), or returns R002 for NaN and too big values.
pub(super) fn float_to_int(f: f64) -> Result<Value, SomoniError> {
    if f.is_nan() || f >= INT_LIMIT || f < -INT_LIMIT {
        return Err(ErrorCode::IntegerOverflow.into());
    }
    Ok(Value::Int(f.trunc() as i64))
}

/// Returns `ok(n)` for a string like `"42"`, or an `err` with the reason.
pub(super) fn parse_int(text: &str) -> Value {
    match text.parse::<i64>() {
        Ok(n) => Value::Ok(Rc::new(Value::Int(n))),
        Err(_) => cannot_convert(text, Type::Int),
    }
}

/// Returns `ok(f)` for a string like `"1.5"`, or an `err` with the reason.
pub(super) fn parse_float(text: &str) -> Value {
    match text.parse::<f64>() {
        Ok(f) if f.is_finite() => Value::Ok(Rc::new(Value::Float(f))),
        _ => cannot_convert(text, Type::Float),
    }
}

/// The type of `int(string)` or `float(string)`: the number, or the reason why there is none.
pub(super) fn conversion_result(target: Type) -> Type {
    Type::Result(Box::new(target), Box::new(Type::Str))
}

/// The `err` of a conversion that is not possible, e.g. `int("abc")`.
fn cannot_convert(text: &str, target: Type) -> Value {
    let message = format!("cannot convert '{}' to {}", text, target);
    Value::Err(Rc::new(string_value(message)))
}
