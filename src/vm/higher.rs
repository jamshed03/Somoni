//! The built-in methods that call a function for the elements of a list: `map`, `filter`, `reduce`, `any`, `all`, `sort_by`, `sort_by_desc`.

use std::rc::Rc;

use crate::builtins::Builtin;
use crate::bytecode::Program;
use crate::error::SomoniError;
use crate::value::{Order, Value};

use super::{Frame, Vm};

impl<'p> Vm<'p> {
    /// Takes the list and the other `count - 1` arguments from the stack, runs the method and pushes its result.
    #[inline(never)]
    pub(super) fn call_higher(
        &mut self,
        builtin: Builtin,
        count: usize,
        frame: &mut Frame<'p>,
        program: &'p Program,
    ) -> Result<(), SomoniError> {
        let args = self.stack.split_off(self.top_start(count));
        let result = match (builtin, args.as_slice()) {
            (Builtin::Map, [Value::List(items), f]) => self.map(items, f, frame, program)?,
            (Builtin::Filter, [Value::List(items), f]) => self.filter(items, f, frame, program)?,
            (Builtin::Reduce, [Value::List(items), start, f]) => {
                self.reduce(items, start, f, frame, program)?
            }
            (Builtin::Any, [Value::List(items), f]) => {
                Value::Bool(self.find(items, f, true, frame, program)?)
            }
            (Builtin::All, [Value::List(items), f]) => {
                Value::Bool(!self.find(items, f, false, frame, program)?)
            }
            (Builtin::SortBy, [Value::List(items), f]) => {
                self.sort_by(items, f, Order::Ascending, frame, program)?
            }
            (Builtin::SortByDesc, [Value::List(items), f]) => {
                self.sort_by(items, f, Order::Descending, frame, program)?
            }
            _ => {
                return Err(SomoniError::internal(
                    "method with a function on wrong values",
                ));
            }
        };
        self.stack.push(result);
        Ok(())
    }

    /// Returns `f(item)` for every item.
    fn map(
        &mut self,
        items: &[Value],
        f: &Value,
        frame: &mut Frame<'p>,
        program: &'p Program,
    ) -> Result<Value, SomoniError> {
        let mut results = Vec::with_capacity(items.len());
        for item in items {
            results.push(self.call_now(f, std::slice::from_ref(item), frame, program)?);
        }
        Ok(Value::List(Rc::new(results)))
    }

    /// Returns the items for which `f(item)` is true.
    fn filter(
        &mut self,
        items: &[Value],
        f: &Value,
        frame: &mut Frame<'p>,
        program: &'p Program,
    ) -> Result<Value, SomoniError> {
        let mut kept = Vec::new();
        for item in items {
            if self.test(f, item, frame, program)? {
                kept.push(item.clone());
            }
        }
        Ok(Value::List(Rc::new(kept)))
    }

    /// Returns `f(... f(f(start, item0), item1) ..., itemN)`.
    fn reduce(
        &mut self,
        items: &[Value],
        start: &Value,
        f: &Value,
        frame: &mut Frame<'p>,
        program: &'p Program,
    ) -> Result<Value, SomoniError> {
        let mut result = start.clone();
        for item in items {
            result = self.call_now(f, &[result, item.clone()], frame, program)?;
        }
        Ok(result)
    }

    /// Returns true as soon as `f(item)` is `wanted` for an item, without calling `f` for the rest.
    fn find(
        &mut self,
        items: &[Value],
        f: &Value,
        wanted: bool,
        frame: &mut Frame<'p>,
        program: &'p Program,
    ) -> Result<bool, SomoniError> {
        for item in items {
            if self.test(f, item, frame, program)? == wanted {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns the items sorted by the key `f(item)` in `order`; items with the same key keep their order.
    fn sort_by(
        &mut self,
        items: &[Value],
        f: &Value,
        order: Order,
        frame: &mut Frame<'p>,
        program: &'p Program,
    ) -> Result<Value, SomoniError> {
        let mut keyed = Vec::with_capacity(items.len());
        for item in items {
            let key = self.call_now(f, std::slice::from_ref(item), frame, program)?;
            keyed.push((key, item.clone()));
        }
        keyed.sort_by(|(a, _), (b, _)| order.compare(a, b));
        let sorted = keyed.into_iter().map(|(_, item)| item).collect();
        Ok(Value::List(Rc::new(sorted)))
    }

    /// Returns `f(item)`, which the type checker made sure is a bool.
    fn test(
        &mut self,
        f: &Value,
        item: &Value,
        frame: &mut Frame<'p>,
        program: &'p Program,
    ) -> Result<bool, SomoniError> {
        match self.call_now(f, std::slice::from_ref(item), frame, program)? {
            Value::Bool(b) => Ok(b),
            _ => Err(SomoniError::internal("a test function gave no bool")),
        }
    }
}
