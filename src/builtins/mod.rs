//! The functions and methods built into Somoni; the methods of strings and the conversions have files of their own.

use std::rc::Rc;

use crate::error::{ErrorCode, SomoniError};
use crate::text::Text;
use crate::types::Type;
use crate::value::{Order, Value, map_key};

use convert::{conversion_result, float_to_int, parse_float, parse_int};
use strings::{Side, format_float, index_of, join, pad, repeat, replace, slice_text, split};

mod collections;
mod convert;
mod math;
mod strings;

/// The functions built into Somoni.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Builtin {
    Print,
    Int,
    Float,
    Str,
    Len,
    Has,
    Keys,
    Ok,
    Err,
    IsOk,
    IsErr,
    Upper,
    Lower,
    Trim,
    Split,
    Join,
    Contains,
    StartsWith,
    EndsWith,
    Replace,
    Slice,
    IndexOf,
    Abs,
    Min,
    Max,
    Sqrt,
    Pow,
    Floor,
    Ceil,
    Round,
    Reverse,
    Sort,
    SortDesc,
    Remove,
    Values,
    Map,
    Filter,
    Reduce,
    Any,
    All,
    SortBy,
    SortByDesc,
    Get,
    Format,
    PadLeft,
    PadRight,
    Repeat,
}

/// Every built-in function and method, so that a name only has to be written in `name`.
const ALL: &[Builtin] = &[
    Builtin::Print,
    Builtin::Int,
    Builtin::Float,
    Builtin::Str,
    Builtin::Len,
    Builtin::Has,
    Builtin::Keys,
    Builtin::Ok,
    Builtin::Err,
    Builtin::IsOk,
    Builtin::IsErr,
    Builtin::Upper,
    Builtin::Lower,
    Builtin::Trim,
    Builtin::Split,
    Builtin::Join,
    Builtin::Contains,
    Builtin::StartsWith,
    Builtin::EndsWith,
    Builtin::Replace,
    Builtin::Slice,
    Builtin::IndexOf,
    Builtin::Abs,
    Builtin::Min,
    Builtin::Max,
    Builtin::Sqrt,
    Builtin::Pow,
    Builtin::Floor,
    Builtin::Ceil,
    Builtin::Round,
    Builtin::Reverse,
    Builtin::Sort,
    Builtin::SortDesc,
    Builtin::Remove,
    Builtin::Values,
    Builtin::Map,
    Builtin::Filter,
    Builtin::Reduce,
    Builtin::Any,
    Builtin::All,
    Builtin::SortBy,
    Builtin::SortByDesc,
    Builtin::Get,
    Builtin::Format,
    Builtin::PadLeft,
    Builtin::PadRight,
    Builtin::Repeat,
];

impl Builtin {
    /// Returns the built-in function with this name, if there is one; methods like `upper` are no functions.
    pub fn from_name(name: &str) -> Option<Builtin> {
        Builtin::named(name).filter(|builtin| !builtin.is_method())
    }

    /// Returns the built-in method with this name, e.g. `upper` for `s.upper()`.
    pub fn method_named(name: &str) -> Option<Builtin> {
        Builtin::named(name).filter(|builtin| builtin.is_method())
    }

    /// Returns the function or method with this name.
    fn named(name: &str) -> Option<Builtin> {
        ALL.iter().copied().find(|builtin| builtin.name() == name)
    }

    /// Returns true for a method, which is called on a value (`s.upper()`) and gets that value as its first argument.
    pub fn is_method(self) -> bool {
        match self {
            Builtin::Upper
            | Builtin::Lower
            | Builtin::Trim
            | Builtin::Split
            | Builtin::Join
            | Builtin::Contains
            | Builtin::StartsWith
            | Builtin::EndsWith
            | Builtin::Replace
            | Builtin::Slice
            | Builtin::IndexOf
            | Builtin::Reverse
            | Builtin::Sort
            | Builtin::SortDesc
            | Builtin::Remove
            | Builtin::Values
            | Builtin::Map
            | Builtin::Filter
            | Builtin::Reduce
            | Builtin::Any
            | Builtin::All
            | Builtin::SortBy
            | Builtin::SortByDesc
            | Builtin::Get
            | Builtin::Format
            | Builtin::PadLeft
            | Builtin::PadRight
            | Builtin::Repeat => true,
            Builtin::Print
            | Builtin::Int
            | Builtin::Float
            | Builtin::Str
            | Builtin::Len
            | Builtin::Has
            | Builtin::Keys
            | Builtin::Ok
            | Builtin::Err
            | Builtin::IsOk
            | Builtin::IsErr
            | Builtin::Abs
            | Builtin::Min
            | Builtin::Max
            | Builtin::Sqrt
            | Builtin::Pow
            | Builtin::Floor
            | Builtin::Ceil
            | Builtin::Round => false,
        }
    }

    /// Returns true for a method like `l.map(f)` that calls a function, which only the VM can do (`CallHigher`).
    pub fn calls_functions(self) -> bool {
        matches!(
            self,
            Builtin::Map
                | Builtin::Filter
                | Builtin::Reduce
                | Builtin::Any
                | Builtin::All
                | Builtin::SortBy
                | Builtin::SortByDesc
        )
    }

    /// Returns true for `ok` and `err`, which take their result type from where they are used.
    pub fn builds_result(self) -> bool {
        matches!(self, Builtin::Ok | Builtin::Err)
    }

    /// Returns `ok` or `err` if `name` is one of them.
    pub fn result_builder(name: &str) -> Option<Builtin> {
        Builtin::from_name(name).filter(|builtin| builtin.builds_result())
    }

    /// Returns the name of the function as written in Somoni.
    pub fn name(self) -> &'static str {
        match self {
            Builtin::Print => "print",
            Builtin::Int => "int",
            Builtin::Float => "float",
            Builtin::Str => "string",
            Builtin::Len => "len",
            Builtin::Has => "has",
            Builtin::Keys => "keys",
            Builtin::Ok => "ok",
            Builtin::Err => "err",
            Builtin::IsOk => "is_ok",
            Builtin::IsErr => "is_err",
            Builtin::Upper => "upper",
            Builtin::Lower => "lower",
            Builtin::Trim => "trim",
            Builtin::Split => "split",
            Builtin::Join => "join",
            Builtin::Contains => "contains",
            Builtin::StartsWith => "starts_with",
            Builtin::EndsWith => "ends_with",
            Builtin::Replace => "replace",
            Builtin::Slice => "slice",
            Builtin::IndexOf => "index_of",
            Builtin::Abs => "abs",
            Builtin::Min => "min",
            Builtin::Max => "max",
            Builtin::Sqrt => "sqrt",
            Builtin::Pow => "pow",
            Builtin::Floor => "floor",
            Builtin::Ceil => "ceil",
            Builtin::Round => "round",
            Builtin::Reverse => "reverse",
            Builtin::Sort => "sort",
            Builtin::SortDesc => "sort_desc",
            Builtin::Remove => "remove",
            Builtin::Values => "values",
            Builtin::Map => "map",
            Builtin::Filter => "filter",
            Builtin::Reduce => "reduce",
            Builtin::Any => "any",
            Builtin::All => "all",
            Builtin::SortBy => "sort_by",
            Builtin::SortByDesc => "sort_by_desc",
            Builtin::Get => "get",
            Builtin::Format => "format",
            Builtin::PadLeft => "pad_left",
            Builtin::PadRight => "pad_right",
            Builtin::Repeat => "repeat",
        }
    }

    /// Returns the number of arguments.
    pub fn arity(self) -> usize {
        match self {
            Builtin::Print
            | Builtin::Int
            | Builtin::Float
            | Builtin::Str
            | Builtin::Len
            | Builtin::Keys
            | Builtin::Ok
            | Builtin::Err
            | Builtin::IsOk
            | Builtin::IsErr
            | Builtin::Upper
            | Builtin::Lower
            | Builtin::Trim
            | Builtin::Abs
            | Builtin::Sqrt
            | Builtin::Floor
            | Builtin::Ceil
            | Builtin::Round
            | Builtin::Reverse
            | Builtin::Sort
            | Builtin::SortDesc
            | Builtin::Values => 1,
            Builtin::Has
            | Builtin::Split
            | Builtin::Join
            | Builtin::Contains
            | Builtin::StartsWith
            | Builtin::EndsWith
            | Builtin::IndexOf
            | Builtin::Min
            | Builtin::Max
            | Builtin::Pow
            | Builtin::Remove
            | Builtin::Map
            | Builtin::Filter
            | Builtin::Any
            | Builtin::All
            | Builtin::SortBy
            | Builtin::SortByDesc
            | Builtin::Format
            | Builtin::PadLeft
            | Builtin::PadRight
            | Builtin::Repeat => 2,
            Builtin::Replace | Builtin::Slice | Builtin::Reduce | Builtin::Get => 3,
        }
    }

    /// Returns the result type for these argument types, or `Err(i)` if argument `i` is not allowed.
    pub fn result_type(self, args: &[Type]) -> Result<Type, usize> {
        match (self, args) {
            (Builtin::Print, [Type::Void | Type::Function(_)]) => Err(0),
            (Builtin::Print, [_]) => Ok(Type::Void),
            (Builtin::Int, [Type::Float]) => Ok(Type::Int),
            (Builtin::Int, [Type::Str]) => Ok(conversion_result(Type::Int)),
            (Builtin::Float, [Type::Int]) => Ok(Type::Float),
            (Builtin::Float, [Type::Str]) => Ok(conversion_result(Type::Float)),
            (Builtin::Str, [Type::Int | Type::Float | Type::Bool]) => Ok(Type::Str),
            (Builtin::Len, [Type::Str | Type::List(_) | Type::Map(_, _)]) => Ok(Type::Int),
            (Builtin::Has, [Type::Map(key, _), found]) if **key == *found => Ok(Type::Bool),
            (Builtin::Has, [Type::Map(_, _), _]) => Err(1),
            (Builtin::Keys, [Type::Map(key, _)]) => Ok(Type::List(key.clone())),
            (Builtin::IsOk | Builtin::IsErr, [Type::Result(_, _)]) => Ok(Type::Bool),
            (Builtin::Upper | Builtin::Lower | Builtin::Trim, [Type::Str]) => Ok(Type::Str),
            (Builtin::Split, [Type::Str, Type::Str]) => Ok(Type::List(Box::new(Type::Str))),
            (Builtin::Join, [Type::List(element), Type::Str]) if **element == Type::Str => {
                Ok(Type::Str)
            }
            (
                Builtin::Contains | Builtin::StartsWith | Builtin::EndsWith,
                [Type::Str, Type::Str],
            ) => Ok(Type::Bool),
            (Builtin::IndexOf, [Type::Str, Type::Str]) => Ok(Type::Optional(Box::new(Type::Int))),
            (Builtin::Replace, [Type::Str, Type::Str, Type::Str]) => Ok(Type::Str),
            (Builtin::Slice, [Type::Str, Type::Int, Type::Int]) => Ok(Type::Str),
            (Builtin::PadLeft | Builtin::PadRight | Builtin::Repeat, [Type::Str, Type::Int]) => {
                Ok(Type::Str)
            }
            (Builtin::Replace | Builtin::Slice, [Type::Str, Type::Str | Type::Int, _]) => Err(2),
            (
                Builtin::Split
                | Builtin::Contains
                | Builtin::StartsWith
                | Builtin::EndsWith
                | Builtin::IndexOf
                | Builtin::Replace
                | Builtin::Slice
                | Builtin::PadLeft
                | Builtin::PadRight
                | Builtin::Repeat,
                [Type::Str, ..],
            ) => Err(1),
            (Builtin::Format, [Type::Float, Type::Int]) => Ok(Type::Str),
            (Builtin::Format, [Type::Float, _]) => Err(1),
            (Builtin::Join, [Type::List(element), _]) if **element == Type::Str => Err(1),
            (Builtin::Abs, [number @ (Type::Int | Type::Float)]) => Ok(number.clone()),
            (Builtin::Min | Builtin::Max | Builtin::Pow, [Type::Int, Type::Int]) => Ok(Type::Int),
            (Builtin::Min | Builtin::Max | Builtin::Pow, [Type::Float, Type::Float]) => {
                Ok(Type::Float)
            }
            (Builtin::Min | Builtin::Max | Builtin::Pow, [Type::Int | Type::Float, _]) => Err(1),
            (Builtin::Sqrt, [Type::Float]) => Ok(Type::Float),
            (Builtin::Floor | Builtin::Ceil | Builtin::Round, [Type::Float]) => Ok(Type::Int),
            (Builtin::Contains, [Type::List(element), x]) if **element == *x => Ok(Type::Bool),
            (Builtin::IndexOf, [Type::List(element), x]) if **element == *x => {
                Ok(Type::Optional(Box::new(Type::Int)))
            }
            (Builtin::Contains | Builtin::IndexOf, [Type::List(_), _]) => Err(1),
            (Builtin::Reverse, [list @ Type::List(_)]) => Ok(list.clone()),
            (Builtin::Sort | Builtin::SortDesc, [list @ Type::List(element)])
                if matches!(**element, Type::Int | Type::Float | Type::Str) =>
            {
                Ok(list.clone())
            }
            (Builtin::Slice, [list @ Type::List(_), Type::Int, Type::Int]) => Ok(list.clone()),
            (Builtin::Slice, [Type::List(_), Type::Int, _]) => Err(2),
            (Builtin::Slice, [Type::List(_), _, _]) => Err(1),
            (Builtin::Remove, [list @ Type::List(_), Type::Int]) => Ok(list.clone()),
            (Builtin::Remove, [map @ Type::Map(key, _), found]) if **key == *found => {
                Ok(map.clone())
            }
            (Builtin::Remove, [Type::List(_) | Type::Map(_, _), _]) => Err(1),
            (Builtin::Values, [Type::Map(_, value)]) => Ok(Type::List(value.clone())),
            (Builtin::Get, [Type::Map(key, value), found, default])
                if **key == *found && **value == *default =>
            {
                Ok((**value).clone())
            }
            (Builtin::Get, [Type::Map(key, _), found, _]) if **key == *found => Err(2),
            (Builtin::Get, [Type::Map(_, _), _, _]) => Err(1),
            (Builtin::Map, [Type::List(element), Type::Function(f)])
                if f.params == [(**element).clone()] && f.result != Type::Void =>
            {
                Ok(Type::List(Box::new(f.result.clone())))
            }
            (Builtin::Filter, [list @ Type::List(element), Type::Function(f)])
                if f.params == [(**element).clone()] && f.result == Type::Bool =>
            {
                Ok(list.clone())
            }
            (Builtin::Any | Builtin::All, [Type::List(element), Type::Function(f)])
                if f.params == [(**element).clone()] && f.result == Type::Bool =>
            {
                Ok(Type::Bool)
            }
            (
                Builtin::SortBy | Builtin::SortByDesc,
                [list @ Type::List(element), Type::Function(f)],
            ) if f.params == [(**element).clone()]
                && matches!(f.result, Type::Int | Type::Float | Type::Str) =>
            {
                Ok(list.clone())
            }
            (Builtin::Reduce, [Type::List(element), start, Type::Function(f)])
                if f.params == [start.clone(), (**element).clone()] && f.result == *start =>
            {
                Ok(start.clone())
            }
            (
                Builtin::Map
                | Builtin::Filter
                | Builtin::Any
                | Builtin::All
                | Builtin::SortBy
                | Builtin::SortByDesc,
                [Type::List(_), _],
            ) => Err(1),
            (Builtin::Reduce, [Type::List(_), _, _]) => Err(2),
            _ => Err(0),
        }
    }

    /// Runs the function with its arguments.
    pub fn call(self, args: &[Value]) -> Result<Value, SomoniError> {
        match (self, args) {
            (Builtin::Print, [arg]) => {
                println!("{}", arg);
                Ok(Value::Void)
            }
            (Builtin::Int, [Value::Float(f)]) => float_to_int(*f),
            (Builtin::Int, [Value::Str(s)]) => Ok(parse_int(s.as_str())),
            (Builtin::Float, [Value::Int(n)]) => Ok(Value::Float(*n as f64)),
            (Builtin::Float, [Value::Str(s)]) => Ok(parse_float(s.as_str())),
            (Builtin::Str, [arg]) => Ok(string_value(arg.to_string())),
            (Builtin::Len, [Value::Str(s)]) => Ok(Value::Int(s.char_count() as i64)),
            (Builtin::Len, [Value::List(items)]) => Ok(Value::Int(items.len() as i64)),
            (Builtin::Len, [Value::Map(map)]) => Ok(Value::Int(map.len() as i64)),
            (Builtin::Has, [Value::Map(map), key]) => Ok(Value::Bool(map.contains(&map_key(key)?))),
            (Builtin::Keys, [Value::Map(map)]) => {
                let keys = map.entries().map(|(key, _)| key.to_value()).collect();
                Ok(Value::List(Rc::new(keys)))
            }
            (Builtin::Ok, [arg]) => Ok(Value::Ok(Rc::new(arg.clone()))),
            (Builtin::Err, [arg]) => Ok(Value::Err(Rc::new(arg.clone()))),
            (Builtin::IsOk, [arg]) => Ok(Value::Bool(matches!(arg, Value::Ok(_)))),
            (Builtin::IsErr, [arg]) => Ok(Value::Bool(matches!(arg, Value::Err(_)))),
            (Builtin::Upper, [Value::Str(s)]) => Ok(string_value(s.as_str().to_uppercase())),
            (Builtin::Lower, [Value::Str(s)]) => Ok(string_value(s.as_str().to_lowercase())),
            (Builtin::Trim, [Value::Str(s)]) => Ok(string_value(s.as_str().trim().to_string())),
            (Builtin::Split, [Value::Str(s), Value::Str(sep)]) => {
                Ok(split(s.as_str(), sep.as_str()))
            }
            (Builtin::Join, [Value::List(items), Value::Str(sep)]) => join(items, sep.as_str()),
            (Builtin::Contains, [Value::Str(s), Value::Str(part)]) => {
                Ok(Value::Bool(s.as_str().contains(part.as_str())))
            }
            (Builtin::StartsWith, [Value::Str(s), Value::Str(part)]) => {
                Ok(Value::Bool(s.as_str().starts_with(part.as_str())))
            }
            (Builtin::EndsWith, [Value::Str(s), Value::Str(part)]) => {
                Ok(Value::Bool(s.as_str().ends_with(part.as_str())))
            }
            (Builtin::Replace, [Value::Str(s), Value::Str(old), Value::Str(new)]) => {
                Ok(replace(s, old.as_str(), new.as_str()))
            }
            (Builtin::Slice, [Value::Str(s), Value::Int(start), Value::Int(end)]) => {
                slice_text(s, *start, *end)
            }
            (Builtin::IndexOf, [Value::Str(s), Value::Str(part)]) => {
                Ok(index_of(s.as_str(), part.as_str()))
            }
            (Builtin::Format, [Value::Float(f), Value::Int(places)]) => format_float(*f, *places),
            (Builtin::PadLeft, [Value::Str(s), Value::Int(width)]) => {
                Ok(pad(s, *width, Side::Left))
            }
            (Builtin::PadRight, [Value::Str(s), Value::Int(width)]) => {
                Ok(pad(s, *width, Side::Right))
            }
            (Builtin::Repeat, [Value::Str(s), Value::Int(times)]) => repeat(s.as_str(), *times),
            (Builtin::Abs, [Value::Int(n)]) => math::abs_int(*n),
            (Builtin::Abs, [Value::Float(f)]) => Ok(Value::Float(f.abs())),
            (Builtin::Min, [Value::Int(a), Value::Int(b)]) => Ok(Value::Int(*a.min(b))),
            (Builtin::Max, [Value::Int(a), Value::Int(b)]) => Ok(Value::Int(*a.max(b))),
            (Builtin::Min, [Value::Float(a), Value::Float(b)]) => Ok(Value::Float(a.min(*b))),
            (Builtin::Max, [Value::Float(a), Value::Float(b)]) => Ok(Value::Float(a.max(*b))),
            (Builtin::Sqrt, [Value::Float(f)]) => math::sqrt(*f),
            (Builtin::Pow, [Value::Int(a), Value::Int(b)]) => math::pow_int(*a, *b),
            (Builtin::Pow, [Value::Float(a), Value::Float(b)]) => math::pow_float(*a, *b),
            (Builtin::Floor, [Value::Float(f)]) => math::to_int(*f, f64::floor),
            (Builtin::Ceil, [Value::Float(f)]) => math::to_int(*f, f64::ceil),
            (Builtin::Round, [Value::Float(f)]) => math::to_int(*f, f64::round),
            (Builtin::Contains, [Value::List(items), x]) => Ok(Value::Bool(items.contains(x))),
            (Builtin::IndexOf, [Value::List(items), x]) => Ok(collections::index_of(items, x)),
            (Builtin::Reverse, [Value::List(items)]) => Ok(collections::reversed(items)),
            (Builtin::Slice, [Value::List(items), Value::Int(start), Value::Int(end)]) => {
                collections::slice_list(items, *start, *end)
            }
            (Builtin::Sort, [Value::List(items)]) => {
                Ok(collections::sorted(items, Order::Ascending))
            }
            (Builtin::SortDesc, [Value::List(items)]) => {
                Ok(collections::sorted(items, Order::Descending))
            }
            (Builtin::Remove, [Value::List(items), Value::Int(index)]) => {
                collections::without_index(items, *index)
            }
            (Builtin::Remove, [Value::Map(map), key]) => collections::without_key(map, key),
            (Builtin::Values, [Value::Map(map)]) => Ok(collections::values(map)),
            (Builtin::Get, [Value::Map(map), key, default]) => {
                collections::get_or(map, key, default)
            }
            (builtin, _) => Err(SomoniError::internal(format!(
                "'{}' cannot take these arguments",
                builtin.name()
            ))),
        }
    }
}

/// Returns a string value.
pub(super) fn string_value(s: String) -> Value {
    Value::Str(Rc::new(Text::new(s)))
}

/// Returns `start..end` as positions inside `length` values, or R009 if they are outside or backwards.
pub(super) fn slice_range(
    start: i64,
    end: i64,
    length: usize,
) -> Result<(usize, usize), SomoniError> {
    let from = usize::try_from(start).ok();
    let to = usize::try_from(end).ok().filter(|&to| to <= length);
    match (from, to) {
        (Some(from), Some(to)) if from <= to => Ok((from, to)),
        _ => Err(ErrorCode::SliceOutOfRange { start, end, length }.into()),
    }
}
