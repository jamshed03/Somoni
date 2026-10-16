#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    Void,
}

impl Type {
    pub fn from_name(name: &str) -> Option<Type> {
        match name {
            "int" => Some(Type::Int),
            "float" => Some(Type::Float),
            "bool" => Some(Type::Bool),
            "string" => Some(Type::Str),
            _ => None,
        }
    }

    pub fn from_return_name(name: &str) -> Option<Type> {
        match name {
            "void" => Some(Type::Void),
            _ => Type::from_name(name),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Type::Int => "int",
            Type::Float => "float",
            Type::Bool => "bool",
            Type::Str => "string",
            Type::Void => "void",
        }
    }
}
