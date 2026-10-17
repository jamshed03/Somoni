use std::fmt;
use std::rc::Rc;

use crate::ast::EnumDecl;

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
    /// `map<K, V>` with the key type and the value type.
    Map(Box<Type>, Box<Type>),
    /// An enum of the program, by its declaration.
    Enum(Rc<EnumDecl>),
    /// `result<T, E>` with the type of `ok` and the type of `err`.
    Result(Box<Type>, Box<Type>),
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
        Type::arg_count(name).is_some() || Type::from_return_name(name).is_some()
    }

    /// Returns how many type arguments `list`, `map` and `result` take, None for all other names.
    pub fn arg_count(name: &str) -> Option<usize> {
        match name {
            "list" => Some(1),
            "map" | "result" => Some(2),
            _ => None,
        }
    }

    /// Builds `list<T>`, `map<K, V>` or `result<T, E>` from the name and the type arguments.
    pub fn with_args(name: &str, args: Vec<Type>) -> Option<Type> {
        let mut args = args.into_iter().map(Box::new);
        match (name, args.next(), args.next(), args.next()) {
            ("list", Some(element), None, None) => Some(Type::List(element)),
            ("map", Some(key), Some(value), None) => Some(Type::Map(key, value)),
            ("result", Some(value), Some(error), None) => Some(Type::Result(value, error)),
            _ => None,
        }
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
            Type::Map(key, value) => write!(f, "map<{}, {}>", key, value),
            Type::Enum(decl) => write!(f, "{}", decl.name),
            Type::Result(value, error) => write!(f, "result<{}, {}>", value, error),
        }
    }
}
