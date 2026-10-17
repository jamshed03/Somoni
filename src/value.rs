use std::fmt;
use std::rc::Rc;

use crate::ast::{BinaryOp, StructDecl, UnaryOp};
use crate::error::{ErrorCode, SomoniError};
use crate::map::{Key, MapValue};
use crate::span::Span;
use crate::text::Text;

/// A value while the program runs.
/// Strings, lists and structs are shared with `Rc` and only copied when one copy is changed.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Rc<Text>),
    List(Rc<Vec<Value>>),
    Struct(Rc<StructValue>),
    Map(Rc<MapValue>),
    Null,
    Void,
}

/// The value of a struct: its declaration and the field values in declaration order.
#[derive(Debug, Clone)]
pub struct StructValue {
    pub decl: Rc<StructDecl>,
    pub fields: Vec<Value>,
}

/// Two struct values are equal if they are the same struct and all fields are equal.
impl PartialEq for StructValue {
    fn eq(&self, other: &StructValue) -> bool {
        Rc::ptr_eq(&self.decl, &other.decl) && self.fields == other.fields
    }
}

/// The text `print` shows, e.g. `[1, 2]` or `Point { x: 1, y: 2 }`.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(x) => write!(f, "{}", x),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Str(s) => write!(f, "{}", s),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", Quoted(item))?;
                }
                write!(f, "]")
            }
            Value::Struct(value) => {
                write!(f, "{} {{", value.decl.name)?;
                for (i, (field, item)) in value.decl.fields.iter().zip(&value.fields).enumerate() {
                    let comma = if i > 0 { "," } else { "" };
                    write!(f, "{} {}: ", comma, field.name)?;
                    write!(f, "{}", Quoted(item))?;
                }
                if value.fields.is_empty() {
                    write!(f, "}}")
                } else {
                    write!(f, " }}")
                }
            }
            Value::Map(map) => {
                write!(f, "{{")?;
                for (i, (key, value)) in map.entries().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", Quoted(&key.to_value()))?;
                    write!(f, ": ")?;
                    write!(f, "{}", Quoted(value))?;
                }
                write!(f, "}}")
            }
            Value::Null => write!(f, "null"),
            Value::Void => write!(f, "void"),
        }
    }
}

/// A value shown like inside a list or struct: strings get quotes, `"1"`, other values not.
pub struct Quoted<'a>(pub &'a Value);

impl fmt::Display for Quoted<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            Value::Str(s) => write!(f, "\"{}\"", s),
            other => write!(f, "{}", other),
        }
    }
}

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

/// Reads `list[index]` or `map[key]`: R005 for an index outside the list, R006 for a missing key.
pub fn index(container: &Value, index: &Value) -> Result<Value, SomoniError> {
    match (container, index) {
        (Value::List(items), Value::Int(i)) => Ok(items[list_position(*i, items.len())?].clone()),
        (Value::Map(map), key) => {
            let key = map_key(key)?;
            map.get(&key).cloned().ok_or_else(|| key_not_found(&key))
        }
        _ => Err(SomoniError::internal("index on a non-list")),
    }
}

/// One step from a variable to a part of it: a list index or map key, or a field.
pub enum Step {
    Index { index: Value, span: Span },
    Field(usize),
}

/// Changes the part of `target` that `steps` lead to, e.g. `l[0].x`, with `change`.
/// A list, map or struct that another variable also uses is copied first.
/// With `may_add_key`, the last step may add a missing key to a map (`m["new"] = 1`);
/// otherwise a missing key is R006.
pub fn change_part(
    target: &mut Value,
    steps: Vec<Step>,
    may_add_key: bool,
    change: impl FnOnce(&mut Value) -> Result<(), SomoniError>,
) -> Result<(), SomoniError> {
    let mut place = target;
    let count = steps.len();
    for (i, step) in steps.into_iter().enumerate() {
        let is_last = i + 1 == count;
        place = match (step, place) {
            (
                Step::Index {
                    index: Value::Int(index),
                    span,
                },
                Value::List(items),
            ) => {
                let items = Rc::make_mut(items);
                let position = list_position(index, items.len())
                    .map_err(|error| error.with_span_if_missing(Some(span)))?;
                &mut items[position]
            }
            (Step::Index { index, span }, Value::Map(map)) => {
                let key = map_key(&index)?;
                let map = Rc::make_mut(map);
                if is_last && may_add_key {
                    map.entry(key)
                } else {
                    map.get_mut(&key)
                        .ok_or_else(|| key_not_found(&key).with_span_if_missing(Some(span)))?
                }
            }
            (Step::Field(position), Value::Struct(fields)) => Rc::make_mut(fields)
                .fields
                .get_mut(position)
                .ok_or_else(|| SomoniError::internal("unknown field"))?,
            _ => return Err(SomoniError::internal("step does not fit the value")),
        };
    }
    change(place)
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

/// Computes `+ - * / %` for two ints. Overflow and division by zero are runtime errors.
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

/// Computes `+ - * /` for two floats. Division by zero is a runtime error.
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
        _ => None,
    }
}

/// Computes `< > <= >=` for two numbers, or None for other operators.
fn order<T: PartialOrd>(op: BinaryOp, a: T, b: T) -> Option<bool> {
    match op {
        BinaryOp::Less => Some(a < b),
        BinaryOp::Greater => Some(a > b),
        BinaryOp::LessEq => Some(a <= b),
        BinaryOp::GreaterEq => Some(a >= b),
        _ => None,
    }
}

/// Appends `rest` to the string or list in `target`.
/// Copies `target` first only if another variable also uses it.
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

/// Turns a value into a map key; the type checker made sure it is an int or a string.
pub fn map_key(value: &Value) -> Result<Key, SomoniError> {
    Key::from_value(value).ok_or_else(|| SomoniError::internal("map key is no int or string"))
}

/// Shows a key in an error message, strings with quotes: `"Tom"`.
pub fn show_key(key: &Key) -> String {
    match key {
        Key::Int(n) => n.to_string(),
        Key::Str(s) => format!("\"{}\"", s),
    }
}

/// The error R006 for a key that is not in the map.
pub fn key_not_found(key: &Key) -> SomoniError {
    ErrorCode::KeyNotFound { key: show_key(key) }.into()
}

/// Returns the position of `index` in a list with `length` elements,
/// or R005 if the index is outside the list (also when negative).
pub fn list_position(index: i64, length: usize) -> Result<usize, SomoniError> {
    usize::try_from(index)
        .ok()
        .filter(|&i| i < length)
        .ok_or_else(|| ErrorCode::IndexOutOfRange { index, length }.into())
}
