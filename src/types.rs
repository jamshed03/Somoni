use std::fmt;

/// A type in the type checker.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    /// No value: only the result of a function like `print`, never the type of a variable.
    Void,
    List(Box<Type>),
    /// A struct of the program, by its name.
    Struct(String),
    /// `int?`: an int or null.
    Optional(Box<Type>),
}

impl Type {
    /// Returns the type with this name: `int`, `float`, `bool` or `string`.
    pub fn from_name(name: &str) -> Option<Type> {
        match name {
            "int" => Some(Type::Int),
            "float" => Some(Type::Float),
            "bool" => Some(Type::Bool),
            "string" => Some(Type::Str),
            _ => None,
        }
    }

    /// Returns true for the names of built-in types; a struct may not use them.
    pub fn is_builtin_name(name: &str) -> bool {
        name == "list" || Type::from_return_name(name).is_some()
    }

    /// Like `from_name`, but also knows `void` (for return types).
    pub fn from_return_name(name: &str) -> Option<Type> {
        match name {
            "void" => Some(Type::Void),
            _ => Type::from_name(name),
        }
    }
}

/// The type as written in Somoni, e.g. `list<list<int>>`.
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Bool => write!(f, "bool"),
            Type::Str => write!(f, "string"),
            Type::Void => write!(f, "void"),
            Type::List(element) => write!(f, "list<{}>", element),
            Type::Struct(name) => write!(f, "{}", name),
            Type::Optional(inner) => write!(f, "{}?", inner),
        }
    }
}
