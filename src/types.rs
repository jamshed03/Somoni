use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::rc::Rc;

use crate::ast::{ClassLayout, EnumDecl, InterfaceLayout, StructDecl};

/// A declaration compared by identity, so two structs with the same name in two files are different types.
#[derive(Debug)]
pub struct Decl<T>(pub Rc<T>);

impl<T> Clone for Decl<T> {
    fn clone(&self) -> Decl<T> {
        Decl(Rc::clone(&self.0))
    }
}

/// Equal only if both point to the same declaration.
impl<T> PartialEq for Decl<T> {
    fn eq(&self, other: &Decl<T>) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<T> Eq for Decl<T> {}

/// Hashes the address of the declaration, so a `Decl` can be the key of a `HashMap`.
impl<T> Hash for Decl<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).hash(state);
    }
}

/// Lets `decl.name` read the field of the declaration directly.
impl<T> Deref for Decl<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

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
    /// A struct of the program, by its declaration.
    Struct(Decl<StructDecl>),
    /// `int?`: an int or null.
    Optional(Box<Type>),
    /// `map<K, V>` with the key type and the value type.
    Map(Box<Type>, Box<Type>),
    /// An enum of the program, by its declaration.
    Enum(Decl<EnumDecl>),
    /// An object of a class of the program, by the layout of its class.
    Class(Decl<ClassLayout>),
    /// An object of any class that implements this interface.
    Interface(Decl<InterfaceLayout>),
    /// `result<T, E>` with the type of `ok` and the type of `err`.
    Result(Box<Type>, Box<Type>),
    /// `fn(int, string): bool`, behind an `Rc` so that `Type` stays small.
    Function(Rc<FunctionType>),
    /// `(int, string)` with the type of each element.
    Tuple(Rc<[Type]>),
    /// `Animal & Shape`: an object that has all these types, which only narrowing with `instanceof` makes;
    /// at least two parts, the class (if any) first.
    Intersection(Rc<[Type]>),
    /// An open SQLite database from `db_open`.
    Database,
    /// One row of `db.fetch`, read with `get_int` and the other getters.
    Row,
}

/// What the place of a lambda tells about its type, e.g. the parameter type that `l.map(...)` gives it.
#[derive(Debug, Clone)]
pub struct FunctionHint {
    pub params: Vec<Type>,
    /// `None` where any result is fine, like for `map`.
    pub result: Option<Type>,
}

/// Shows a hint as a function type, with `...` for a result where any fits: `fn(int): bool`, `fn(int): ...`.
impl fmt::Display for FunctionHint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let params = Type::joined(&self.params, ", ");
        match &self.result {
            Some(result) => write!(f, "fn({}): {}", params, result),
            None => write!(f, "fn({}): ...", params),
        }
    }
}

/// The parameter types and the result type of a function type.
#[derive(Debug, PartialEq)]
pub struct FunctionType {
    pub params: Vec<Type>,
    pub result: Type,
}

impl Type {
    /// Returns the number of the file that declares the first struct, enum, class or interface in this type.
    pub fn origin_file(&self) -> Option<usize> {
        match self {
            Type::Struct(decl) => Some(decl.name_span.file as usize),
            Type::Enum(decl) => Some(decl.name_span.file as usize),
            Type::Class(layout) => Some(layout.file),
            Type::Interface(layout) => Some(layout.file),
            Type::List(inner) | Type::Optional(inner) => inner.origin_file(),
            Type::Map(first, second) | Type::Result(first, second) => {
                first.origin_file().or_else(|| second.origin_file())
            }
            Type::Tuple(elements) | Type::Intersection(elements) => {
                elements.iter().find_map(Type::origin_file)
            }
            Type::Function(function) => (function.params.iter())
                .chain([&function.result])
                .find_map(Type::origin_file),
            Type::Int
            | Type::Float
            | Type::Bool
            | Type::Str
            | Type::Void
            | Type::Database
            | Type::Row => None,
        }
    }

    /// Returns the parts of `Animal & Shape`, or the type itself as its only part.
    pub fn parts(&self) -> &[Type] {
        match self {
            Type::Intersection(parts) => parts,
            other => std::slice::from_ref(other),
        }
    }

    /// Returns the part that can have fields: the class of `Animal & Shape`, or the type itself.
    pub fn with_fields(&self) -> &Type {
        (self.parts().iter())
            .find(|part| matches!(part, Type::Class(_)))
            .unwrap_or(self)
    }

    /// Shows the types one after another with `separator` between them, like `int, string` or `Animal & Shape`.
    pub fn joined(types: &[Type], separator: &str) -> String {
        let shown: Vec<String> = types.iter().map(Type::to_string).collect();
        shown.join(separator)
    }

    /// Builds the type `fn(params): result`.
    pub fn function(params: Vec<Type>, result: Type) -> Type {
        Type::Function(Rc::new(FunctionType { params, result }))
    }
}

impl Type {
    /// Returns the type with this name: `int`, `float`, `bool` or `string`.
    pub fn from_name(name: &str) -> Option<Type> {
        match name {
            "int" => Some(Type::Int),
            "float" => Some(Type::Float),
            "bool" => Some(Type::Bool),
            "string" => Some(Type::Str),
            "database" => Some(Type::Database),
            "row" => Some(Type::Row),
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
            Type::Struct(decl) => write!(f, "{}", decl.name),
            Type::Optional(inner) => write!(f, "{}?", inner),
            Type::Map(key, value) => write!(f, "map<{}, {}>", key, value),
            Type::Enum(decl) => write!(f, "{}", decl.name),
            Type::Class(layout) => write!(f, "{}", layout.name),
            Type::Interface(layout) => write!(f, "{}", layout.name),
            Type::Result(value, error) => write!(f, "result<{}, {}>", value, error),
            Type::Tuple(elements) => write!(f, "({})", Type::joined(elements, ", ")),
            Type::Function(function) => write!(
                f,
                "fn({}): {}",
                Type::joined(&function.params, ", "),
                function.result
            ),
            Type::Intersection(parts) => write!(f, "{}", Type::joined(parts, " & ")),
            Type::Database => write!(f, "database"),
            Type::Row => write!(f, "row"),
        }
    }
}
