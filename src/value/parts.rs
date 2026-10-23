//! Reading and changing parts of values: list elements, map entries and fields.

use std::rc::Rc;

use crate::error::{ErrorCode, SomoniError};
use crate::map::Key;
use crate::span::Span;

use super::{Object, Value, map_key, show_key};

/// One step from a variable to a part of it: a list index or map key, or a field.
pub enum Step {
    Index { index: Value, span: Span },
    Field(usize),
}

/// The steps from a variable to a part of it, and whether one of them goes into an object.
pub struct Way {
    pub steps: Vec<Step>,
    pub through_object: bool,
}

/// Reads `list[index]` or `map[key]`, or returns R005 for an index outside the list and R006 for a missing key.
pub fn index(container: &Value, index: &Value) -> Result<Value, SomoniError> {
    index_ref(container, index).cloned()
}

/// Reads field `position` of a struct or an object.
pub fn field(value: &Value, position: usize) -> Result<Value, SomoniError> {
    let found = match value {
        Value::Struct(value) => value.fields.get(position).cloned(),
        Value::Object(object) => object.0.borrow().fields.get(position).cloned(),
        _ => None,
    };
    found.ok_or_else(|| SomoniError::internal("field of a non-struct"))
}

/// Changes the part of `target` that `way` leads to (like `l[0].x`) with `change`; `may_add_key` lets the last step add a map key.
pub fn change_part(
    target: &mut Value,
    mut way: Way,
    may_add_key: bool,
    change: impl FnOnce(&mut Value) -> Result<(), SomoniError>,
) -> Result<(), SomoniError> {
    let found = if way.through_object {
        last_object(target, &way.steps)?
    } else {
        None
    };
    match found {
        None => change_inside(target, way.steps, may_add_key, change),
        Some((object, at)) => {
            change_in_object(&object, way.steps.split_off(at), may_add_key, change)
        }
    }
}

/// Changes a part inside a field of `object`, so only this object is opened for writing; `steps` start with that field.
fn change_in_object(
    object: &Object,
    mut steps: Vec<Step>,
    may_add_key: bool,
    change: impl FnOnce(&mut Value) -> Result<(), SomoniError>,
) -> Result<(), SomoniError> {
    let rest = steps.split_off(1);
    let Some(Step::Field(position)) = steps.pop() else {
        return Err(SomoniError::internal("object without a field step"));
    };
    let mut object = object.0.borrow_mut();
    let place = object.fields.get_mut(position).ok_or_else(unknown_field)?;
    change_inside(place, rest, may_add_key, change)
}

/// Returns the last object on the way along `steps` and the number of the step into its field, without copies up to the first object.
fn last_object(target: &Value, steps: &[Step]) -> Result<Option<(Object, usize)>, SomoniError> {
    let mut place = target;
    for (i, step) in steps.iter().enumerate() {
        if let Value::Object(object) = place {
            return last_object_from(object.clone(), i, steps);
        }
        if i + 1 == steps.len() {
            break;
        }
        place = read_step(place, step)?;
    }
    Ok(None)
}

/// Like `last_object`, from the object at step `start` on, copying each part (only `Rc`s) because objects lend their fields only briefly.
fn last_object_from(
    object: Object,
    start: usize,
    steps: &[Step],
) -> Result<Option<(Object, usize)>, SomoniError> {
    let mut found = None;
    let mut current = Value::Object(object);
    for (i, step) in steps.iter().enumerate().skip(start) {
        if let (Value::Object(object), Step::Field(_)) = (&current, step) {
            found = Some((object.clone(), i));
        }
        if i + 1 == steps.len() {
            break;
        }
        current = match step {
            Step::Index { index: key, span } => index(&current, key).map_err(at_step(*span))?,
            Step::Field(position) => field(&current, *position)?,
        };
    }
    Ok(found)
}

/// Returns the part of a list, map or struct that one step leads to, without changing it.
fn read_step<'a>(place: &'a Value, step: &Step) -> Result<&'a Value, SomoniError> {
    match (step, place) {
        (Step::Index { index, span }, container) => {
            index_ref(container, index).map_err(at_step(*span))
        }
        (Step::Field(position), Value::Struct(value)) => {
            value.fields.get(*position).ok_or_else(unknown_field)
        }
        _ => Err(SomoniError::internal("step does not fit the value")),
    }
}

/// Changes a part of `target` on a way without objects, copying a shared list, map or struct first.
fn change_inside(
    target: &mut Value,
    steps: Vec<Step>,
    may_add_key: bool,
    change: impl FnOnce(&mut Value) -> Result<(), SomoniError>,
) -> Result<(), SomoniError> {
    let mut place = target;
    let count = steps.len();
    for (i, step) in steps.into_iter().enumerate() {
        place = step_into(place, step, may_add_key && i + 1 == count)?;
    }
    change(place)
}

/// Goes one step into a list, map or struct to change it; with `add_key` a missing map key is added.
fn step_into(place: &mut Value, step: Step, add_key: bool) -> Result<&mut Value, SomoniError> {
    match (step, place) {
        (
            Step::Index {
                index: Value::Int(index),
                span,
            },
            Value::List(items),
        ) => {
            let items = Rc::make_mut(items);
            let position = list_position(index, items.len()).map_err(at_step(span))?;
            Ok(&mut items[position])
        }
        (Step::Index { index, span }, Value::Map(map)) => {
            let key = map_key(&index)?;
            let map = Rc::make_mut(map);
            if add_key {
                return Ok(map.entry(key));
            }
            map.get_mut(&key)
                .ok_or_else(|| key_not_found(&key).with_span_if_missing(Some(span)))
        }
        (Step::Field(position), Value::Struct(fields)) => Rc::make_mut(fields)
            .fields
            .get_mut(position)
            .ok_or_else(unknown_field),
        _ => Err(SomoniError::internal("step does not fit the value")),
    }
}

/// Like `index`, but returns the element itself instead of a copy.
fn index_ref<'a>(container: &'a Value, index: &Value) -> Result<&'a Value, SomoniError> {
    match (container, index) {
        (Value::List(items), Value::Int(i)) => Ok(&items[list_position(*i, items.len())?]),
        (Value::Map(map), key) => {
            let key = map_key(key)?;
            map.get(&key).ok_or_else(|| key_not_found(&key))
        }
        _ => Err(SomoniError::internal("index on a non-list")),
    }
}

/// Returns the position of `index` in a list with `length` elements, or R005 if it is outside the list.
fn list_position(index: i64, length: usize) -> Result<usize, SomoniError> {
    usize::try_from(index)
        .ok()
        .filter(|&i| i < length)
        .ok_or_else(|| ErrorCode::IndexOutOfRange { index, length }.into())
}

/// Returns the error R006 for a key that is not in the map.
fn key_not_found(key: &Key) -> SomoniError {
    ErrorCode::KeyNotFound { key: show_key(key) }.into()
}

/// Returns the internal error for a field position that the value does not have.
fn unknown_field() -> SomoniError {
    SomoniError::internal("unknown field")
}

/// Returns a function that gives an error without a place the place of a step.
fn at_step(span: Span) -> impl FnOnce(SomoniError) -> SomoniError {
    move |error| error.with_span_if_missing(Some(span))
}
