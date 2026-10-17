use std::rc::Rc;

use crate::error::{ErrorCode, SomoniError};
use crate::interpreter::Value;
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
        }
    }

    /// Returns the number of arguments. All built-ins take one.
    pub fn arity(self) -> usize {
        1
    }

    /// Returns the result type for this argument type, or None if the argument is not allowed.
    /// Converting to the same type (`int(5)`) is not allowed.
    pub fn result_type(self, arg: &Type) -> Option<Type> {
        match (self, arg) {
            (Builtin::Print, Type::Void) => None,
            (Builtin::Print, _) => Some(Type::Void),
            (Builtin::Int, Type::Float | Type::Str) => Some(Type::Int),
            (Builtin::Float, Type::Int | Type::Str) => Some(Type::Float),
            (Builtin::Str, Type::Int | Type::Float | Type::Bool) => Some(Type::Str),
            (Builtin::Len, Type::Str | Type::List(_)) => Some(Type::Int),
            _ => None,
        }
    }

    /// Runs the function with its argument.
    pub fn call(self, arg: Value) -> Result<Value, SomoniError> {
        match (self, arg) {
            (Builtin::Print, arg) => {
                println!("{}", arg);
                Ok(Value::Void)
            }
            (Builtin::Int, Value::Float(f)) => float_to_int(f),
            (Builtin::Int, Value::Str(s)) => s
                .as_str()
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|_| cannot_convert(s.as_str(), Type::Int)),
            (Builtin::Float, Value::Int(n)) => Ok(Value::Float(n as f64)),
            (Builtin::Float, Value::Str(s)) => match s.as_str().parse::<f64>() {
                Ok(f) if f.is_finite() => Ok(Value::Float(f)),
                _ => Err(cannot_convert(s.as_str(), Type::Float)),
            },
            (Builtin::Str, arg) => Ok(Value::Str(Rc::new(Text::new(arg.to_string())))),
            (Builtin::Len, Value::Str(s)) => Ok(Value::Int(s.char_count() as i64)),
            (Builtin::Len, Value::List(items)) => Ok(Value::Int(items.len() as i64)),
            (builtin, arg) => Err(SomoniError::internal(format!(
                "'{}' cannot take {}",
                builtin.name(),
                arg
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
