use crate::error::SomoniError;
use crate::interpreter::Value;
use crate::types::Type;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Builtin {
    Print,
}

impl Builtin {
    pub fn from_name(name: &str) -> Option<Builtin> {
        match name {
            "print" => Some(Builtin::Print),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Builtin::Print => "print",
        }
    }

    // all built-ins take exactly one argument (for now)
    pub fn arity(self) -> usize {
        1
    }

    // type checker: the result type for this argument type, None if the argument is not allowed
    pub fn result_type(self, arg: Type) -> Option<Type> {
        match (self, arg) {
            (Builtin::Print, Type::Void) => None,
            (Builtin::Print, _) => Some(Type::Void),
        }
    }

    // interpreter: run it, the type checker already checked the argument
    pub fn call(self, arg: Value) -> Result<Value, SomoniError> {
        match self {
            Builtin::Print => {
                println!("{}", arg);
                Ok(Value::Void)
            }
        }
    }
}
