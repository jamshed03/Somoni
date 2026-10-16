use std::fmt;

// Not Copy: list<int> holds another type inside (a Box), so a Type is cloned where needed.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    // "no value": only the result of a function like print, never a variable type
    Void,
    List(Box<Type>),
}

impl Type {
    // the types without type arguments; list<T> is built by the type checker
    pub fn from_name(name: &str) -> Option<Type> {
        match name {
            "int" => Some(Type::Int),
            "float" => Some(Type::Float),
            "bool" => Some(Type::Bool),
            "string" => Some(Type::Str),
            // "void" is missing on purpose: `var x: void` makes no sense
            _ => None,
        }
    }

    // like from_name, but also "void": a function may return nothing
    pub fn from_return_name(name: &str) -> Option<Type> {
        match name {
            "void" => Some(Type::Void),
            _ => Type::from_name(name),
        }
    }
}

// how a type is written in Somoni, e.g. "list<list<int>>"
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Bool => write!(f, "bool"),
            Type::Str => write!(f, "string"),
            Type::Void => write!(f, "void"),
            Type::List(element) => write!(f, "list<{}>", element),
        }
    }
}
