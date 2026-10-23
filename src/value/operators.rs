//! Computing operators on values: arithmetic, comparisons and joining strings and lists.

use std::rc::Rc;

use crate::ast::{BinaryOp, UnaryOp};
use crate::error::{ErrorCode, SomoniError};

use super::Value;

/// Computes `left op right` for all operators except the short-circuit part of `&&` and `||`.
pub fn binary(op: BinaryOp, l: Value, r: Value) -> Result<Value, SomoniError> {
    if let Some(result) = compare(op, &l, &r) {
        return Ok(Value::Bool(result));
    }
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => int_arithmetic(op, a, b),
        (Value::Float(a), Value::Float(b)) => float_arithmetic(op, a, b),
        (mut joined @ (Value::Str(_) | Value::List(_)), b) if op == BinaryOp::Add => {
            append(&mut joined, b)?;
            Ok(joined)
        }
        (Value::Bool(_), Value::Bool(b)) if matches!(op, BinaryOp::And | BinaryOp::Or) => {
            Ok(Value::Bool(b))
        }
        _ => Err(SomoniError::internal(
            "operator not supported for these values",
        )),
    }
}

/// Computes `!value` or `-value`.
pub fn unary(op: UnaryOp, value: Value) -> Result<Value, SomoniError> {
    match (op, value) {
        (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        (UnaryOp::Neg, Value::Int(n)) => int_or_overflow(n.checked_neg()),
        (UnaryOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
        _ => Err(SomoniError::internal(
            "operator not supported for this value",
        )),
    }
}

/// Appends `rest` to the string or list in `target`, copying `target` first only if it is shared.
pub fn append(target: &mut Value, rest: Value) -> Result<(), SomoniError> {
    match (target, rest) {
        (Value::Str(text), Value::Str(rest)) => Rc::make_mut(text).push(&rest),
        (Value::List(items), Value::List(rest)) => {
            Rc::make_mut(items).extend(rest.iter().cloned());
        }
        _ => return Err(SomoniError::internal("'+' needs two strings or two lists")),
    }
    Ok(())
}

/// Computes `+ - * / %` for two ints, or returns R002 on overflow and R001 for a division by zero.
fn int_arithmetic(op: BinaryOp, a: i64, b: i64) -> Result<Value, SomoniError> {
    let result = match op {
        BinaryOp::Div | BinaryOp::Mod if b == 0 => return Err(ErrorCode::DivisionByZero.into()),
        BinaryOp::Add => a.checked_add(b),
        BinaryOp::Sub => a.checked_sub(b),
        BinaryOp::Mul => a.checked_mul(b),
        BinaryOp::Div => a.checked_div(b),
        BinaryOp::Mod => a.checked_rem(b),
        _ => return Err(SomoniError::internal("operator not supported for int")),
    };
    int_or_overflow(result)
}

/// Turns the result of a `checked_*` operation into an int, or R002 on overflow.
fn int_or_overflow(result: Option<i64>) -> Result<Value, SomoniError> {
    result
        .map(Value::Int)
        .ok_or_else(|| ErrorCode::IntegerOverflow.into())
}

/// Computes `+ - * /` for two floats, or returns R001 for a division by zero.
fn float_arithmetic(op: BinaryOp, a: f64, b: f64) -> Result<Value, SomoniError> {
    let result = match op {
        BinaryOp::Div if b == 0.0 => return Err(ErrorCode::DivisionByZero.into()),
        BinaryOp::Add => a + b,
        BinaryOp::Sub => a - b,
        BinaryOp::Mul => a * b,
        BinaryOp::Div => a / b,
        _ => return Err(SomoniError::internal("operator not supported for float")),
    };
    Ok(Value::Float(result))
}

/// Computes a comparison (`== != < > <= >=`), or None if `op` is no comparison.
fn compare(op: BinaryOp, l: &Value, r: &Value) -> Option<bool> {
    match (op, l, r) {
        (BinaryOp::Eq, _, _) => Some(l == r),
        (BinaryOp::NotEq, _, _) => Some(l != r),
        (_, Value::Int(a), Value::Int(b)) => order(op, a, b),
        (_, Value::Float(a), Value::Float(b)) => order(op, a, b),
        (_, Value::Str(a), Value::Str(b)) => order(op, a.as_str(), b.as_str()),
        _ => None,
    }
}

/// Computes `< > <= >=` for two numbers or strings (by character code), or None for other operators.
fn order<T: PartialOrd>(op: BinaryOp, a: T, b: T) -> Option<bool> {
    match op {
        BinaryOp::Less => Some(a < b),
        BinaryOp::Greater => Some(a > b),
        BinaryOp::LessEq => Some(a <= b),
        BinaryOp::GreaterEq => Some(a >= b),
        _ => None,
    }
}
