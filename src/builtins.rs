use std::rc::Rc;

use crate::error::{ErrorCode, SomoniError};
use crate::text::Text;
use crate::types::Type;
use crate::value::{Value, map_key};

/// The functions built into Somoni.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Builtin {
    Print,
    Int,
    Float,
    Str,
    Len,
    Has,
    Keys,
    Ok,
    Err,
    IsOk,
    IsErr,
    Upper,
    Lower,
    Trim,
    Split,
    Join,
    Contains,
    StartsWith,
    EndsWith,
    Replace,
    Slice,
    IndexOf,
}

/// Every built-in function and method, so that a name only has to be written in `name`.
const ALL: &[Builtin] = &[
    Builtin::Print,
    Builtin::Int,
    Builtin::Float,
    Builtin::Str,
    Builtin::Len,
    Builtin::Has,
    Builtin::Keys,
    Builtin::Ok,
    Builtin::Err,
    Builtin::IsOk,
    Builtin::IsErr,
    Builtin::Upper,
    Builtin::Lower,
    Builtin::Trim,
    Builtin::Split,
    Builtin::Join,
    Builtin::Contains,
    Builtin::StartsWith,
    Builtin::EndsWith,
    Builtin::Replace,
    Builtin::Slice,
    Builtin::IndexOf,
];

impl Builtin {
    /// Returns the built-in function with this name, if there is one; methods like `upper` are no functions.
    pub fn from_name(name: &str) -> Option<Builtin> {
        Builtin::named(name).filter(|builtin| !builtin.is_method())
    }

    /// Returns the built-in method with this name, e.g. `upper` for `s.upper()`.
    pub fn method_named(name: &str) -> Option<Builtin> {
        Builtin::named(name).filter(|builtin| builtin.is_method())
    }

    /// Returns the function or method with this name.
    fn named(name: &str) -> Option<Builtin> {
        ALL.iter().copied().find(|builtin| builtin.name() == name)
    }

    /// Returns true for a method, which is called on a value (`s.upper()`) and gets that value as its first argument.
    pub fn is_method(self) -> bool {
        match self {
            Builtin::Upper
            | Builtin::Lower
            | Builtin::Trim
            | Builtin::Split
            | Builtin::Join
            | Builtin::Contains
            | Builtin::StartsWith
            | Builtin::EndsWith
            | Builtin::Replace
            | Builtin::Slice
            | Builtin::IndexOf => true,
            Builtin::Print
            | Builtin::Int
            | Builtin::Float
            | Builtin::Str
            | Builtin::Len
            | Builtin::Has
            | Builtin::Keys
            | Builtin::Ok
            | Builtin::Err
            | Builtin::IsOk
            | Builtin::IsErr => false,
        }
    }

    /// Returns true for `ok` and `err`, which take their result type from where they are used.
    pub fn builds_result(self) -> bool {
        matches!(self, Builtin::Ok | Builtin::Err)
    }

    /// Returns `ok` or `err` if `name` is one of them.
    pub fn result_builder(name: &str) -> Option<Builtin> {
        Builtin::from_name(name).filter(|builtin| builtin.builds_result())
    }

    /// Returns the name of the function as written in Somoni.
    pub fn name(self) -> &'static str {
        match self {
            Builtin::Print => "print",
            Builtin::Int => "int",
            Builtin::Float => "float",
            Builtin::Str => "string",
            Builtin::Len => "len",
            Builtin::Has => "has",
            Builtin::Keys => "keys",
            Builtin::Ok => "ok",
            Builtin::Err => "err",
            Builtin::IsOk => "is_ok",
            Builtin::IsErr => "is_err",
            Builtin::Upper => "upper",
            Builtin::Lower => "lower",
            Builtin::Trim => "trim",
            Builtin::Split => "split",
            Builtin::Join => "join",
            Builtin::Contains => "contains",
            Builtin::StartsWith => "starts_with",
            Builtin::EndsWith => "ends_with",
            Builtin::Replace => "replace",
            Builtin::Slice => "slice",
            Builtin::IndexOf => "index_of",
        }
    }

    /// Returns the number of arguments.
    pub fn arity(self) -> usize {
        match self {
            Builtin::Print
            | Builtin::Int
            | Builtin::Float
            | Builtin::Str
            | Builtin::Len
            | Builtin::Keys
            | Builtin::Ok
            | Builtin::Err
            | Builtin::IsOk
            | Builtin::IsErr
            | Builtin::Upper
            | Builtin::Lower
            | Builtin::Trim => 1,
            Builtin::Has
            | Builtin::Split
            | Builtin::Join
            | Builtin::Contains
            | Builtin::StartsWith
            | Builtin::EndsWith
            | Builtin::IndexOf => 2,
            Builtin::Replace | Builtin::Slice => 3,
        }
    }

    /// Returns the result type for these argument types, or `Err(i)` if argument `i` is not allowed.
    pub fn result_type(self, args: &[Type]) -> Result<Type, usize> {
        match (self, args) {
            (Builtin::Print, [Type::Void | Type::Function(_)]) => Err(0),
            (Builtin::Print, [_]) => Ok(Type::Void),
            (Builtin::Int, [Type::Float]) => Ok(Type::Int),
            (Builtin::Int, [Type::Str]) => Ok(conversion_result(Type::Int)),
            (Builtin::Float, [Type::Int]) => Ok(Type::Float),
            (Builtin::Float, [Type::Str]) => Ok(conversion_result(Type::Float)),
            (Builtin::Str, [Type::Int | Type::Float | Type::Bool]) => Ok(Type::Str),
            (Builtin::Len, [Type::Str | Type::List(_) | Type::Map(_, _)]) => Ok(Type::Int),
            (Builtin::Has, [Type::Map(key, _), found]) if **key == *found => Ok(Type::Bool),
            (Builtin::Has, [Type::Map(_, _), _]) => Err(1),
            (Builtin::Keys, [Type::Map(key, _)]) => Ok(Type::List(key.clone())),
            (Builtin::IsOk | Builtin::IsErr, [Type::Result(_, _)]) => Ok(Type::Bool),
            (Builtin::Upper | Builtin::Lower | Builtin::Trim, [Type::Str]) => Ok(Type::Str),
            (Builtin::Split, [Type::Str, Type::Str]) => Ok(Type::List(Box::new(Type::Str))),
            (Builtin::Join, [Type::List(element), Type::Str]) if **element == Type::Str => {
                Ok(Type::Str)
            }
            (
                Builtin::Contains | Builtin::StartsWith | Builtin::EndsWith,
                [Type::Str, Type::Str],
            ) => Ok(Type::Bool),
            (Builtin::IndexOf, [Type::Str, Type::Str]) => Ok(Type::Optional(Box::new(Type::Int))),
            (Builtin::Replace, [Type::Str, Type::Str, Type::Str]) => Ok(Type::Str),
            (Builtin::Slice, [Type::Str, Type::Int, Type::Int]) => Ok(Type::Str),
            (Builtin::Replace | Builtin::Slice, [Type::Str, Type::Str | Type::Int, _]) => Err(2),
            (
                Builtin::Split
                | Builtin::Contains
                | Builtin::StartsWith
                | Builtin::EndsWith
                | Builtin::IndexOf
                | Builtin::Replace
                | Builtin::Slice,
                [Type::Str, ..],
            ) => Err(1),
            (Builtin::Join, [Type::List(element), _]) if **element == Type::Str => Err(1),
            _ => Err(0),
        }
    }

    /// Runs the function with its arguments.
    pub fn call(self, args: &[Value]) -> Result<Value, SomoniError> {
        match (self, args) {
            (Builtin::Print, [arg]) => {
                println!("{}", arg);
                Ok(Value::Void)
            }
            (Builtin::Int, [Value::Float(f)]) => float_to_int(*f),
            (Builtin::Int, [Value::Str(s)]) => Ok(parse_int(s.as_str())),
            (Builtin::Float, [Value::Int(n)]) => Ok(Value::Float(*n as f64)),
            (Builtin::Float, [Value::Str(s)]) => Ok(parse_float(s.as_str())),
            (Builtin::Str, [arg]) => Ok(string_value(arg.to_string())),
            (Builtin::Len, [Value::Str(s)]) => Ok(Value::Int(s.char_count() as i64)),
            (Builtin::Len, [Value::List(items)]) => Ok(Value::Int(items.len() as i64)),
            (Builtin::Len, [Value::Map(map)]) => Ok(Value::Int(map.len() as i64)),
            (Builtin::Has, [Value::Map(map), key]) => Ok(Value::Bool(map.contains(&map_key(key)?))),
            (Builtin::Keys, [Value::Map(map)]) => {
                let keys = map.entries().map(|(key, _)| key.to_value()).collect();
                Ok(Value::List(Rc::new(keys)))
            }
            (Builtin::Ok, [arg]) => Ok(Value::Ok(Rc::new(arg.clone()))),
            (Builtin::Err, [arg]) => Ok(Value::Err(Rc::new(arg.clone()))),
            (Builtin::IsOk, [arg]) => Ok(Value::Bool(matches!(arg, Value::Ok(_)))),
            (Builtin::IsErr, [arg]) => Ok(Value::Bool(matches!(arg, Value::Err(_)))),
            (Builtin::Upper, [Value::Str(s)]) => Ok(string_value(s.as_str().to_uppercase())),
            (Builtin::Lower, [Value::Str(s)]) => Ok(string_value(s.as_str().to_lowercase())),
            (Builtin::Trim, [Value::Str(s)]) => Ok(string_value(s.as_str().trim().to_string())),
            (Builtin::Split, [Value::Str(s), Value::Str(sep)]) => {
                Ok(split(s.as_str(), sep.as_str()))
            }
            (Builtin::Join, [Value::List(items), Value::Str(sep)]) => join(items, sep.as_str()),
            (Builtin::Contains, [Value::Str(s), Value::Str(part)]) => {
                Ok(Value::Bool(s.as_str().contains(part.as_str())))
            }
            (Builtin::StartsWith, [Value::Str(s), Value::Str(part)]) => {
                Ok(Value::Bool(s.as_str().starts_with(part.as_str())))
            }
            (Builtin::EndsWith, [Value::Str(s), Value::Str(part)]) => {
                Ok(Value::Bool(s.as_str().ends_with(part.as_str())))
            }
            (Builtin::Replace, [Value::Str(s), Value::Str(old), Value::Str(new)]) => {
                Ok(replace(s, old.as_str(), new.as_str()))
            }
            (Builtin::Slice, [Value::Str(s), Value::Int(start), Value::Int(end)]) => {
                slice_text(s, *start, *end)
            }
            (Builtin::IndexOf, [Value::Str(s), Value::Str(part)]) => {
                Ok(index_of(s.as_str(), part.as_str()))
            }
            (builtin, _) => Err(SomoniError::internal(format!(
                "'{}' cannot take these arguments",
                builtin.name()
            ))),
        }
    }
}

/// Returns a string value.
fn string_value(s: String) -> Value {
    Value::Str(Rc::new(Text::new(s)))
}

/// Splits `s` at every `sep`, or into its characters if `sep` is empty.
fn split(s: &str, sep: &str) -> Value {
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
fn join(items: &[Value], sep: &str) -> Result<Value, SomoniError> {
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
fn replace(s: &Rc<Text>, old: &str, new: &str) -> Value {
    if old.is_empty() {
        return Value::Str(Rc::clone(s));
    }
    string_value(s.as_str().replace(old, new))
}

/// Returns the characters of `s` from `start` up to `end`, or R009 if that is not inside `s`.
fn slice_text(s: &Text, start: i64, end: i64) -> Result<Value, SomoniError> {
    let length = s.char_count();
    let (from, to) = slice_range(start, end, length)?;
    let part: String = s.as_str().chars().skip(from).take(to - from).collect();
    Ok(string_value(part))
}

/// Returns `start..end` as positions inside `length` values, or R009 if they are outside or backwards.
fn slice_range(start: i64, end: i64, length: usize) -> Result<(usize, usize), SomoniError> {
    let from = usize::try_from(start).ok();
    let to = usize::try_from(end).ok().filter(|&to| to <= length);
    match (from, to) {
        (Some(from), Some(to)) if from <= to => Ok((from, to)),
        _ => Err(ErrorCode::SliceOutOfRange { start, end, length }.into()),
    }
}

/// Returns the place in characters of the first `part` in `s`, or `null`.
fn index_of(s: &str, part: &str) -> Value {
    match s.find(part) {
        Some(byte) => Value::Int(s[..byte].chars().count() as i64),
        None => Value::Null,
    }
}

/// The first float that is too big for an int: 2 to the power of 63.
const INT_LIMIT: f64 = 9_223_372_036_854_775_808.0;

/// Cuts a float to an int (`int(-3.9)` is -3), or returns R002 for NaN and too big values.
fn float_to_int(f: f64) -> Result<Value, SomoniError> {
    if f.is_nan() || f >= INT_LIMIT || f < -INT_LIMIT {
        return Err(ErrorCode::IntegerOverflow.into());
    }
    Ok(Value::Int(f.trunc() as i64))
}

/// Returns `ok(n)` for a string like `"42"`, or an `err` with the reason.
fn parse_int(text: &str) -> Value {
    match text.parse::<i64>() {
        Ok(n) => Value::Ok(Rc::new(Value::Int(n))),
        Err(_) => cannot_convert(text, Type::Int),
    }
}

/// Returns `ok(f)` for a string like `"1.5"`, or an `err` with the reason.
fn parse_float(text: &str) -> Value {
    match text.parse::<f64>() {
        Ok(f) if f.is_finite() => Value::Ok(Rc::new(Value::Float(f))),
        _ => cannot_convert(text, Type::Float),
    }
}

/// The type of `int(string)` or `float(string)`: the number, or the reason why there is none.
fn conversion_result(target: Type) -> Type {
    Type::Result(Box::new(target), Box::new(Type::Str))
}

/// The `err` of a conversion that is not possible, e.g. `int("abc")`.
fn cannot_convert(text: &str, target: Type) -> Value {
    let message = format!("cannot convert '{}' to {}", text, target);
    Value::Err(Rc::new(string_value(message)))
}
