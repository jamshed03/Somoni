//! The values of a running program, and how they are shown, computed and changed in parts.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::{ClassLayout, EnumDecl, StructDecl};
use crate::error::SomoniError;
use crate::map::{Key, MapValue};
use crate::text::Text;

mod operators;
mod parts;
mod show;

pub use operators::{append, binary, unary};
pub use parts::{Step, Way, change_part, field, index};
pub use show::Quoted;

/// A value while the program runs; strings, lists and structs are copied only when changed, objects are shared.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Rc<Text>),
    List(Rc<Vec<Value>>),
    Struct(Rc<StructValue>),
    Map(Rc<MapValue>),
    Enum(Rc<EnumValue>),
    Object(Object),
    /// `ok(x)` of a result, with the value inside.
    Ok(Rc<Value>),
    /// `err(x)` of a result, with the error inside.
    Err(Rc<Value>),
    Null,
    Void,
}

/// The value of a struct: its declaration and the field values in declaration order.
#[derive(Debug, Clone)]
pub struct StructValue {
    pub decl: Rc<StructDecl>,
    pub fields: Vec<Value>,
}

/// Two struct values are equal if they are the same struct and all fields are equal.
impl PartialEq for StructValue {
    fn eq(&self, other: &StructValue) -> bool {
        Rc::ptr_eq(&self.decl, &other.decl) && self.fields == other.fields
    }
}

/// A variant of an enum: its declaration (for the names in `print`) and its number.
#[derive(Debug, Clone)]
pub struct EnumValue {
    pub decl: Rc<EnumDecl>,
    pub variant: usize,
}

/// Two variants are equal if they belong to the same enum and are the same variant.
impl PartialEq for EnumValue {
    fn eq(&self, other: &EnumValue) -> bool {
        Rc::ptr_eq(&self.decl, &other.decl) && self.variant == other.variant
    }
}

/// An object of a class that all its copies share, like an object in TypeScript.
#[derive(Debug, Clone)]
pub struct Object(pub Rc<RefCell<ObjectValue>>);

/// The layout and number of the class of an object, and its field values in declaration order.
#[derive(Debug)]
pub struct ObjectValue {
    pub layout: Rc<ClassLayout>,
    pub class: usize,
    pub fields: Vec<Value>,
}

impl Object {
    /// A new object of class number `class` with the layout `layout`, its fields not set yet.
    pub fn new(layout: Rc<ClassLayout>, class: usize) -> Object {
        let fields = vec![Value::Void; layout.fields.len()];
        Object(Rc::new(RefCell::new(ObjectValue {
            layout,
            class,
            fields,
        })))
    }
}

/// Two objects are equal only if they are the same object (like `===`).
impl PartialEq for Object {
    fn eq(&self, other: &Object) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Turns a value into a map key, or returns an internal error if it is no int or string.
pub fn map_key(value: &Value) -> Result<Key, SomoniError> {
    Key::from_value(value).ok_or_else(|| SomoniError::internal("map key is no int or string"))
}

/// Shows a key in an error message, strings with quotes: `"Tom"`.
pub fn show_key(key: &Key) -> String {
    match key {
        Key::Int(n) => n.to_string(),
        Key::Str(s) => format!("\"{}\"", s),
    }
}
