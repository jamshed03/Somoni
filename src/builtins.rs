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
}

impl Builtin {
    /// Returns the built-in function with this name, if there is one.
    pub fn from_name(name: &str) -> Option<Builtin> {
        match name {
            "print" => Some(Builtin::Print),
            "int" => Some(Builtin::Int),
            "float" => Some(Builtin::Float),
            "string" => Some(Builtin::Str),
            "len" => Some(Builtin::Len),
            "has" => Some(Builtin::Has),
            "keys" => Some(Builtin::Keys),
            "ok" => Some(Builtin::Ok),
            "err" => Some(Builtin::Err),
            _ => None,
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
            | Builtin::Err => 1,
            Builtin::Has => 2,
        }
    }

    /// Returns the result type for these argument types, or `Err(i)` if argument `i` is not allowed.
    pub fn result_type(self, args: &[Type]) -> Result<Type, usize> {
        match (self, args) {
            (Builtin::Print, [Type::Void]) => Err(0),
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
            (Builtin::Str, [arg]) => Ok(Value::Str(Rc::new(Text::new(arg.to_string())))),
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
            (builtin, _) => Err(SomoniError::internal(format!(
                "'{}' cannot take these arguments",
                builtin.name()
            ))),
        }
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
    Value::Err(Rc::new(Value::Str(Rc::new(Text::new(message)))))
}
