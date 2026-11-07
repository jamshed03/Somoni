//! The built-in methods of strings, like `s.split(",")`.

use std::rc::Rc;

use crate::error::{ErrorCode, SomoniError};
use crate::text::Text;
use crate::value::Value;

use super::math::no_result;
use super::{Builtin, slice_range, string_value};

/// Splits `s` at every `sep`, or into its characters if `sep` is empty.
pub(super) fn split(s: &str, sep: &str) -> Value {
    let parts: Vec<Value> = if sep.is_empty() {
        s.chars().map(|c| string_value(c.to_string())).collect()
    } else {
        s.split(sep)
            .map(|part| string_value(part.to_string()))
            .collect()
    };
    Value::List(Rc::new(parts))
}

/// Joins a list of strings with `sep` between them.
pub(super) fn join(items: &[Value], sep: &str) -> Result<Value, SomoniError> {
    let parts = items
        .iter()
        .map(|item| match item {
            Value::Str(s) => Ok(s.as_str()),
            _ => Err(SomoniError::internal(
                "'join' on a list that holds no strings",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(string_value(parts.join(sep)))
}

/// Replaces every `old` in `s` with `new`; an empty `old` leaves `s` as it is.
pub(super) fn replace(s: &Rc<Text>, old: &str, new: &str) -> Value {
    if old.is_empty() {
        return Value::Str(Rc::clone(s));
    }
    string_value(s.as_str().replace(old, new))
}

/// Returns the characters of `s` from `start` up to `end`, or R009 if that is not inside `s`.
pub(super) fn slice_text(s: &Text, start: i64, end: i64) -> Result<Value, SomoniError> {
    let length = s.char_count();
    let (from, to) = slice_range(start, end, length)?;
    let part: String = s.as_str().chars().skip(from).take(to - from).collect();
    Ok(string_value(part))
}

/// Returns the place in characters of the first `part` in `s`, or `null`.
pub(super) fn index_of(s: &str, part: &str) -> Value {
    match s.find(part) {
        Some(byte) => Value::Int(s[..byte].chars().count() as i64),
        None => Value::Null,
    }
}

/// The side on which `pad_left` and `pad_right` add their spaces.
pub(super) enum Side {
    Left,
    Right,
}

/// Returns `f` with exactly `places` digits after the point, or R010 for a negative `places`.
pub(super) fn format_float(f: f64, places: i64) -> Result<Value, SomoniError> {
    let places = usize::try_from(places).map_err(|_| no_result(Builtin::Format))?;
    Ok(string_value(format!("{:.*}", places, f)))
}

/// Fills `s` with spaces on `side` up to `width` characters; a shorter `width` leaves `s` as it is.
pub(super) fn pad(s: &Rc<Text>, width: i64, side: Side) -> Value {
    let missing = usize::try_from(width)
        .unwrap_or(0)
        .saturating_sub(s.char_count());
    if missing == 0 {
        return Value::Str(Rc::clone(s));
    }
    let spaces = " ".repeat(missing);
    match side {
        Side::Left => string_value(spaces + s.as_str()),
        Side::Right => string_value(s.as_str().to_string() + &spaces),
    }
}

/// Returns `s` `times` times in a row, or R010 for a negative `times` and R002 if the result is too long.
pub(super) fn repeat(s: &str, times: i64) -> Result<Value, SomoniError> {
    let times = usize::try_from(times).map_err(|_| no_result(Builtin::Repeat))?;
    if s.len().checked_mul(times).is_none() {
        return Err(ErrorCode::IntegerOverflow.into());
    }
    Ok(string_value(s.repeat(times)))
}
