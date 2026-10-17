use std::rc::Rc;

use crate::error::{ErrorCode, SomoniError};
use crate::interpreter::Value;
use crate::map::Key;
use crate::text::Text;
use crate::types::Type;

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
            _ => None,
        }
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
            | Builtin::Keys => 1,
            Builtin::Has => 2,
        }
    }

    /// Returns the result type for these argument types,
    /// or `Err(i)` if argument `i` is not allowed (counted from 0).
    /// Converting to the same type (`int(5)`) is not allowed.
    pub fn result_type(self, args: &[Type]) -> Result<Type, usize> {
        match (self, args) {
            (Builtin::Print, [Type::Void]) => Err(0),
            (Builtin::Print, [_]) => Ok(Type::Void),
            (Builtin::Int, [Type::Float | Type::Str]) => Ok(Type::Int),
            (Builtin::Float, [Type::Int | Type::Str]) => Ok(Type::Float),
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
            (Builtin::Int, [Value::Str(s)]) => s
                .as_str()
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|_| cannot_convert(s.as_str(), Type::Int)),
            (Builtin::Float, [Value::Int(n)]) => Ok(Value::Float(*n as f64)),
            (Builtin::Float, [Value::Str(s)]) => match s.as_str().parse::<f64>() {
                Ok(f) if f.is_finite() => Ok(Value::Float(f)),
                _ => Err(cannot_convert(s.as_str(), Type::Float)),
            },
            (Builtin::Str, [arg]) => Ok(Value::Str(Rc::new(Text::new(arg.to_string())))),
            (Builtin::Len, [Value::Str(s)]) => Ok(Value::Int(s.char_count() as i64)),
            (Builtin::Len, [Value::List(items)]) => Ok(Value::Int(items.len() as i64)),
            (Builtin::Len, [Value::Map(map)]) => Ok(Value::Int(map.len() as i64)),
            (Builtin::Has, [Value::Map(map), key]) => {
                let key = Key::from_value(key)
                    .ok_or_else(|| SomoniError::internal("map key is no int or string"))?;
                Ok(Value::Bool(map.contains(&key)))
            }
            (Builtin::Keys, [Value::Map(map)]) => {
                let keys = map.entries().map(|(key, _)| key.to_value()).collect();
                Ok(Value::List(Rc::new(keys)))
            }
            (builtin, _) => Err(SomoniError::internal(format!(
                "'{}' cannot take these arguments",
                builtin.name()
            ))),
        }
    }
}

/// Turns a float into an int by cutting off: `int(3.9)` is 3, `int(-3.9)` is -3.
/// Too big values and NaN are R002.
fn float_to_int(f: f64) -> Result<Value, SomoniError> {
    let limit = 9_223_372_036_854_775_808.0;
    if f.is_nan() || f >= limit || f < -limit {
        return Err(ErrorCode::IntegerOverflow.into());
    }
    Ok(Value::Int(f.trunc() as i64))
}

/// The error R003 for a conversion that is not possible, e.g. `int("abc")`.
fn cannot_convert(text: &str, target: Type) -> SomoniError {
    ErrorCode::InvalidConversion {
        text: text.to_string(),
        target,
    }
    .into()
}
