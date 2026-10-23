//! How `print` shows a value.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use crate::map::MapValue;

use super::{Object, ObjectValue, Value};

/// The text `print` shows, e.g. `[1, 2]` or `Point { x: 1, y: 2 }`.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Writer::new(f).show(self, false)
    }
}

/// A value shown like inside a list or struct, where a string gets quotes: `"1"`.
pub struct Quoted<'a>(pub &'a Value);

impl fmt::Display for Quoted<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Writer::new(f).show(self.0, true)
    }
}

/// Writes values as `print` shows them and remembers the objects it is inside, so an object in itself is `Name { ... }`.
struct Writer<'a, 'b> {
    f: &'a mut fmt::Formatter<'b>,
    open: Vec<*const RefCell<ObjectValue>>,
}

impl<'a, 'b> Writer<'a, 'b> {
    /// Starts writing into `f`, inside no object yet.
    fn new(f: &'a mut fmt::Formatter<'b>) -> Writer<'a, 'b> {
        Writer {
            f,
            open: Vec::new(),
        }
    }

    /// Writes one value; with `quoted`, a string gets quotes.
    fn show(&mut self, value: &Value, quoted: bool) -> fmt::Result {
        match value {
            Value::Int(n) => write!(self.f, "{}", n),
            Value::Float(x) => write!(self.f, "{}", x),
            Value::Bool(b) => write!(self.f, "{}", b),
            Value::Str(s) if quoted => write!(self.f, "\"{}\"", s),
            Value::Str(s) => write!(self.f, "{}", s),
            Value::List(items) => self.show_list(items),
            Value::Struct(value) => {
                let names = value.decl.fields.iter().map(|field| field.name.as_str());
                self.show_fields(&value.decl.name, names, &value.fields)
            }
            Value::Object(object) => self.show_object(object),
            Value::Map(map) => self.show_map(map),
            Value::Enum(value) => {
                let variant = &value.decl.variants[value.variant].0;
                write!(self.f, "{}.{}", value.decl.name, variant)
            }
            Value::Ok(inner) => self.show_wrapped("ok", inner),
            Value::Err(inner) => self.show_wrapped("err", inner),
            Value::Null => write!(self.f, "null"),
            Value::Void => write!(self.f, "void"),
        }
    }

    /// Writes a list as `[1, 2]`.
    fn show_list(&mut self, items: &[Value]) -> fmt::Result {
        write!(self.f, "[")?;
        for (i, item) in items.iter().enumerate() {
            self.separator(i)?;
            self.show(item, true)?;
        }
        write!(self.f, "]")
    }

    /// Writes a map as `{"a": 1, "b": 2}`.
    fn show_map(&mut self, map: &MapValue) -> fmt::Result {
        write!(self.f, "{{")?;
        for (i, (key, value)) in map.entries().enumerate() {
            self.separator(i)?;
            self.show(&key.to_value(), true)?;
            write!(self.f, ": ")?;
            self.show(value, true)?;
        }
        write!(self.f, "}}")
    }

    /// Writes an object with its fields, or `Name { ... }` if it is already being written.
    fn show_object(&mut self, object: &Object) -> fmt::Result {
        let address = Rc::as_ptr(&object.0);
        let object = object.0.borrow();
        if self.open.contains(&address) {
            return write!(self.f, "{} {{ ... }}", object.layout.name);
        }
        self.open.push(address);
        let names = object.layout.fields.iter().map(String::as_str);
        let result = self.show_fields(&object.layout.name, names, &object.fields);
        self.open.pop();
        result
    }

    /// Writes a struct or object as `Name { a: 1, b: 2 }`, or `Name {}` without fields.
    fn show_fields<'n>(
        &mut self,
        name: &str,
        names: impl Iterator<Item = &'n str>,
        values: &[Value],
    ) -> fmt::Result {
        write!(self.f, "{} {{", name)?;
        for (i, (field, item)) in names.zip(values).enumerate() {
            let comma = if i > 0 { "," } else { "" };
            write!(self.f, "{} {}: ", comma, field)?;
            self.show(item, true)?;
        }
        if values.is_empty() {
            write!(self.f, "}}")
        } else {
            write!(self.f, " }}")
        }
    }

    /// Writes `ok(...)` or `err(...)` around the value inside.
    fn show_wrapped(&mut self, word: &str, inner: &Value) -> fmt::Result {
        write!(self.f, "{}(", word)?;
        self.show(inner, true)?;
        write!(self.f, ")")
    }

    /// Writes `, ` before every item but the first.
    fn separator(&mut self, i: usize) -> fmt::Result {
        if i > 0 {
            write!(self.f, ", ")?;
        }
        Ok(())
    }
}
