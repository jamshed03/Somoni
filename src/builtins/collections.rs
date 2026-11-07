//! The built-in methods of lists and maps, like `l.sort()` and `m.values()`.

use std::rc::Rc;

use crate::error::{ErrorCode, SomoniError};
use crate::map::MapValue;
use crate::value::{Order, Value, map_key};

use super::slice_range;

/// Returns the place of the first `x` in `items`, or `null`.
pub(super) fn index_of(items: &[Value], x: &Value) -> Value {
    match items.iter().position(|item| item == x) {
        Some(place) => Value::Int(place as i64),
        None => Value::Null,
    }
}

/// Returns the items in the other order.
pub(super) fn reversed(items: &[Value]) -> Value {
    list(items.iter().rev().cloned().collect())
}

/// Returns the items from `start` up to `end`, or R009 if that is not inside the list.
pub(super) fn slice_list(items: &[Value], start: i64, end: i64) -> Result<Value, SomoniError> {
    let (from, to) = slice_range(start, end, items.len())?;
    Ok(list(items[from..to].to_vec()))
}

/// Returns the items sorted in `order`; the type checker allows only ints, floats and strings.
pub(super) fn sorted(items: &[Value], order: Order) -> Value {
    let mut items = items.to_vec();
    items.sort_by(|a, b| order.compare(a, b));
    list(items)
}

/// Returns the items without the one at place `index`, or R005 if there is none.
pub(super) fn without_index(items: &[Value], index: i64) -> Result<Value, SomoniError> {
    let length = items.len();
    let place = usize::try_from(index)
        .ok()
        .filter(|&place| place < length)
        .ok_or(ErrorCode::IndexOutOfRange { index, length })?;
    let mut items = items.to_vec();
    items.remove(place);
    Ok(list(items))
}

/// Returns the map without `key`, or the same map if it does not have the key.
pub(super) fn without_key(map: &Rc<MapValue>, key: &Value) -> Result<Value, SomoniError> {
    let key = map_key(key)?;
    if !map.contains(&key) {
        return Ok(Value::Map(Rc::clone(map)));
    }
    let mut rest = MapValue::default();
    for (other, value) in map.entries().filter(|(other, _)| *other != key) {
        rest.insert(other.clone(), value.clone());
    }
    Ok(Value::Map(Rc::new(rest)))
}

/// Returns the value of `key`, or `default` if the map does not have the key.
pub(super) fn get_or(map: &MapValue, key: &Value, default: &Value) -> Result<Value, SomoniError> {
    let key = map_key(key)?;
    Ok(map.get(&key).unwrap_or(default).clone())
}

/// Returns the values of the map in the order they were added.
pub(super) fn values(map: &MapValue) -> Value {
    list(map.entries().map(|(_, value)| value.clone()).collect())
}

/// Returns a list value.
fn list(items: Vec<Value>) -> Value {
    Value::List(Rc::new(items))
}
