use std::collections::HashMap;
use std::rc::Rc;

use crate::interpreter::Value;
use crate::text::Text;

/// A key of a map: an int or a string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Int(i64),
    Str(Rc<Text>),
}

impl Key {
    /// Turns a value into a key, or None if the value cannot be a key.
    pub fn from_value(value: &Value) -> Option<Key> {
        match value {
            Value::Int(n) => Some(Key::Int(*n)),
            Value::Str(s) => Some(Key::Str(Rc::clone(s))),
            _ => None,
        }
    }

    /// Turns the key back into a value.
    pub fn to_value(&self) -> Value {
        match self {
            Key::Int(n) => Value::Int(*n),
            Key::Str(s) => Value::Str(Rc::clone(s)),
        }
    }
}

/// The value of a map. It keeps the entries in the order they were added
/// and finds the entry of a key without searching through all of them.
#[derive(Debug, Clone, Default)]
pub struct MapValue {
    entries: Vec<(Key, Value)>,
    positions: HashMap<Key, usize>,
}

impl MapValue {
    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the map has this key.
    pub fn contains(&self, key: &Key) -> bool {
        self.positions.contains_key(key)
    }

    /// Returns the value for `key`, if the map has it.
    pub fn get(&self, key: &Key) -> Option<&Value> {
        self.positions.get(key).map(|&i| &self.entries[i].1)
    }

    /// Returns the value for `key` to change it, if the map has it.
    pub fn get_mut(&mut self, key: &Key) -> Option<&mut Value> {
        self.positions.get(key).map(|&i| &mut self.entries[i].1)
    }

    /// Returns the value for `key` to change it. A missing key is added first, with `void`
    /// as value until it is overwritten.
    pub fn entry(&mut self, key: Key) -> &mut Value {
        let i = match self.positions.get(&key) {
            Some(&i) => i,
            None => {
                let i = self.entries.len();
                self.positions.insert(key.clone(), i);
                self.entries.push((key, Value::Void));
                i
            }
        };
        &mut self.entries[i].1
    }

    /// Adds an entry, or replaces the value if the key is already there.
    pub fn insert(&mut self, key: Key, value: Value) {
        *self.entry(key) = value;
    }

    /// Returns all entries in the order they were added.
    pub fn entries(&self) -> impl Iterator<Item = &(Key, Value)> {
        self.entries.iter()
    }
}

/// Two maps are equal if they have the same entries, in any order.
impl PartialEq for MapValue {
    fn eq(&self, other: &MapValue) -> bool {
        self.len() == other.len()
            && self
                .entries()
                .all(|(key, value)| other.get(key) == Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Key {
        Key::Str(Rc::new(Text::new(text.to_string())))
    }

    #[test]
    fn insert_keeps_the_order_and_replaces() {
        let mut map = MapValue::default();
        map.insert(key("b"), Value::Int(1));
        map.insert(key("a"), Value::Int(2));
        map.insert(key("b"), Value::Int(3));
        let keys: Vec<&Key> = map.entries().map(|(key, _)| key).collect();
        assert_eq!(keys, vec![&key("b"), &key("a")]);
        assert_eq!(map.get(&key("b")), Some(&Value::Int(3)));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn equal_in_any_order() {
        let mut first = MapValue::default();
        first.insert(Key::Int(1), Value::Bool(true));
        first.insert(Key::Int(2), Value::Bool(false));
        let mut second = MapValue::default();
        second.insert(Key::Int(2), Value::Bool(false));
        second.insert(Key::Int(1), Value::Bool(true));
        assert_eq!(first, second);
    }
}
