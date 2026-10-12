#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
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

    pub fn name(&self) -> &'static str {
        match self {
            Type::Int => "int",
            Type::Float => "float",
            Type::Bool => "bool",
            Type::Str => "string",
        }
    }
}
