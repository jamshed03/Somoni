//! The functions and methods built into Somoni; the methods of strings, the conversions and the input have files of their own.

use std::rc::Rc;

use crate::error::{ErrorCode, SomoniError};
use crate::text::Text;
use crate::types::{FunctionHint, Type};
use crate::value::{Order, Value, map_key};

use convert::{conversion_result, float_to_int, parse_float, parse_int};
use strings::{
    Side, escape_html, format_float, index_of, join, pad, repeat, replace, slice_text, split,
};

pub(crate) use random::Random;
pub(crate) use strings::html_safe;

mod collections;
mod convert;
mod date;
mod io;
mod math;
mod random;
mod strings;

/// Whether a built-in is called as a function, `len(l)`, or as a method on a value, `s.upper()`.
#[derive(PartialEq)]
enum Kind {
    Function,
    Method,
}

/// Writes the enum `Builtin`, the list `ALL` and `name`, `arity` and `is_method` from one line per built-in:
/// `Variant => "name", arguments, Function or Method`, where a method counts the value before the dot as an argument.
macro_rules! builtins {
    ($($variant:ident => $name:literal, $arity:literal, $kind:ident;)*) => {
        /// The functions and methods built into Somoni.
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub enum Builtin {
            $($variant,)*
        }

        /// Every built-in function and method, so that a name only has to be written once.
        const ALL: &[Builtin] = &[$(Builtin::$variant,)*];

        impl Builtin {
            /// Returns the name of the function as written in Somoni.
            pub fn name(self) -> &'static str {
                match self {
                    $(Builtin::$variant => $name,)*
                }
            }

            /// Returns the number of arguments, for a method with the value before the dot.
            pub fn arity(self) -> usize {
                match self {
                    $(Builtin::$variant => $arity,)*
                }
            }

            /// Returns true for a method, which is called on a value (`s.upper()`) and gets that value as its first
            /// argument.
            pub fn is_method(self) -> bool {
                match self {
                    $(Builtin::$variant => Kind::$kind == Kind::Method,)*
                }
            }
        }
    };
}

builtins! {
    Print         => "print",          1, Function;
    Write         => "write",          1, Function;
    Capture       => "capture",        1, Function;
    Input         => "input",          0, Function;
    ReadFile      => "read_file",      1, Function;
    WriteFile     => "write_file",     2, Function;
    CopyFile      => "copy_file",      2, Function;
    AppendFile    => "append_file",    2, Function;
    MakeDir       => "make_dir",       1, Function;
    Args          => "args",           0, Function;
    Now           => "now",            0, Function;
    FormatDate    => "format_date",    2, Function;
    Random        => "random",         2, Function;
    Query         => "query",          1, Function;
    RequestPath   => "request_path",   0, Function;
    Form          => "form",           1, Function;
    RequestMethod => "request_method", 0, Function;
    SetStatus     => "set_status",     1, Function;
    Redirect      => "redirect",       1, Function;
    Int           => "int",            1, Function;
    Float         => "float",          1, Function;
    Str           => "string",         1, Function;
    Len           => "len",            1, Function;
    Has           => "has",            2, Function;
    Keys          => "keys",           1, Function;
    Ok            => "ok",             1, Function;
    Err           => "err",            1, Function;
    IsOk          => "is_ok",          1, Function;
    IsErr         => "is_err",         1, Function;
    Upper         => "upper",          1, Method;
    Lower         => "lower",          1, Method;
    Trim          => "trim",           1, Method;
    Split         => "split",          2, Method;
    Join          => "join",           2, Method;
    Contains      => "contains",       2, Method;
    StartsWith    => "starts_with",    2, Method;
    EndsWith      => "ends_with",      2, Method;
    Replace       => "replace",        3, Method;
    Slice         => "slice",          3, Method;
    IndexOf       => "index_of",       2, Method;
    Abs           => "abs",            1, Function;
    Min           => "min",            2, Function;
    Max           => "max",            2, Function;
    Sqrt          => "sqrt",           1, Function;
    Pow           => "pow",            2, Function;
    Floor         => "floor",          1, Function;
    Ceil          => "ceil",           1, Function;
    Round         => "round",          1, Function;
    Reverse       => "reverse",        1, Method;
    Sort          => "sort",           1, Method;
    SortDesc      => "sort_desc",      1, Method;
    Remove        => "remove",         2, Method;
    Values        => "values",         1, Method;
    Map           => "map",            2, Method;
    Filter        => "filter",         2, Method;
    Reduce        => "reduce",         3, Method;
    Any           => "any",            2, Method;
    All           => "all",            2, Method;
    SortBy        => "sort_by",        2, Method;
    SortByDesc    => "sort_by_desc",   2, Method;
    Get           => "get",            3, Method;
    Format        => "format",         2, Method;
    PadLeft       => "pad_left",       2, Method;
    PadRight      => "pad_right",      2, Method;
    Repeat        => "repeat",         2, Method;
    EscapeHtml    => "escape_html",    1, Method;
}

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

    /// Returns true for a method like `l.map(f)` that calls a function, which only the VM can do (`CallHigher`).
    pub fn calls_functions(self) -> bool {
        matches!(
            self,
            Builtin::Capture
                | Builtin::Map
                | Builtin::Filter
                | Builtin::Reduce
                | Builtin::Any
                | Builtin::All
                | Builtin::SortBy
                | Builtin::SortByDesc
        )
    }

    /// Returns what a lambda as the next argument gets as its type, from the types of the value and the arguments before it.
    pub fn function_hint(self, before: &[Type]) -> Option<FunctionHint> {
        let hint = |params: Vec<Type>, result: Option<Type>| Some(FunctionHint { params, result });
        match (self, before) {
            (Builtin::Capture, []) => hint(Vec::new(), Some(Type::Void)),
            (Builtin::Map | Builtin::SortBy | Builtin::SortByDesc, [Type::List(element)]) => {
                hint(vec![(**element).clone()], None)
            }
            (Builtin::Filter | Builtin::Any | Builtin::All, [Type::List(element)]) => {
                hint(vec![(**element).clone()], Some(Type::Bool))
            }
            (Builtin::Reduce, [Type::List(element), start]) => hint(
                vec![start.clone(), (**element).clone()],
                Some(start.clone()),
            ),
            _ => None,
        }
    }

    /// Returns the text that `print(x)` or `write(x)` puts out, which the VM writes; None for other functions.
    pub fn output_text(self, args: &[Value]) -> Option<String> {
        match (self, args) {
            (Builtin::Print, [arg]) => Some(format!("{}\n", arg)),
            (Builtin::Write, [arg]) => Some(arg.to_string()),
            _ => None,
        }
    }

    /// Returns true for `read_file`, `write_file` and `copy_file`, whose paths start at the folder of the file
    /// with the call; the type checker puts that folder before the arguments.
    pub fn needs_dir(self) -> bool {
        matches!(
            self,
            Builtin::ReadFile
                | Builtin::WriteFile
                | Builtin::AppendFile
                | Builtin::CopyFile
                | Builtin::MakeDir
        )
    }

    /// Returns the type the next argument should have after the types `before`, so that `m.get("x", [])` or
    /// `l.contains(null)` know the type of `[]` and `null`; None where any type is fine.
    pub fn expected_arg(self, before: &[Type]) -> Option<Type> {
        match (self, before) {
            (Builtin::Get, [Type::Map(key, _)]) => Some((**key).clone()),
            (Builtin::Get, [Type::Map(_, value), _]) => Some((**value).clone()),
            (Builtin::Contains | Builtin::IndexOf, [Type::List(element)]) => {
                Some((**element).clone())
            }
            _ => None,
        }
    }

    /// Returns what the next argument after the types `before` should be, for a message: a function type for a lambda
    /// or the one type that fits; None where several types fit, like for `len`.
    pub fn expected_text(self, before: &[Type]) -> Option<String> {
        match (self, self.function_hint(before)) {
            (Builtin::SortBy | Builtin::SortByDesc, Some(hint)) => {
                Some(format!("fn({}): int, float or string", hint.params[0]))
            }
            (_, Some(hint)) => Some(hint.to_string()),
            (_, None) => self.expected_arg(before).map(|ty| ty.to_string()),
        }
    }

    /// Returns true for `ok` and `err`, which take their result type from where they are used.
    pub fn builds_result(self) -> bool {
        matches!(self, Builtin::Ok | Builtin::Err)
    }

    /// Returns `ok` or `err` if `name` is one of them.
    pub fn result_builder(name: &str) -> Option<Builtin> {
        Builtin::from_name(name).filter(|builtin| builtin.builds_result())
    }

    /// Returns the result type for these argument types, or `Err(i)` if argument `i` is not allowed.
    pub fn result_type(self, args: &[Type]) -> Result<Type, usize> {
        match (self, args) {
            (Builtin::Print | Builtin::Write, [Type::Void | Type::Function(_)]) => Err(0),
            (Builtin::Print | Builtin::Write, [_]) => Ok(Type::Void),
            (Builtin::Capture, [Type::Function(f)])
                if f.params.is_empty() && f.result == Type::Void =>
            {
                Ok(Type::Str)
            }
            (Builtin::Input, []) => Ok(Type::Optional(Box::new(Type::Str))),
            (Builtin::ReadFile, [Type::Str]) => {
                Ok(Type::Result(Box::new(Type::Str), Box::new(Type::Str)))
            }
            (Builtin::WriteFile, [Type::Str, Type::Str]) => {
                Ok(Type::Result(Box::new(Type::Int), Box::new(Type::Str)))
            }
            (Builtin::WriteFile, [Type::Str, _]) => Err(1),
            (Builtin::AppendFile, [Type::Str, Type::Str]) => {
                Ok(Type::Result(Box::new(Type::Int), Box::new(Type::Str)))
            }
            (Builtin::AppendFile, [Type::Str, _]) => Err(1),
            (Builtin::MakeDir, [Type::Str]) => {
                Ok(Type::Result(Box::new(Type::Bool), Box::new(Type::Str)))
            }
            (Builtin::CopyFile, [Type::Str, Type::Str]) => {
                Ok(Type::Result(Box::new(Type::Int), Box::new(Type::Str)))
            }
            (Builtin::CopyFile, [Type::Str, _]) => Err(1),
            (Builtin::Query | Builtin::Form, [Type::Str]) => {
                Ok(Type::Optional(Box::new(Type::Str)))
            }
            (Builtin::RequestPath | Builtin::RequestMethod, []) => Ok(Type::Str),
            (Builtin::Args, []) => Ok(Type::List(Box::new(Type::Str))),
            (Builtin::Now, []) => Ok(Type::Int),
            (Builtin::FormatDate, [Type::Int, Type::Str]) => Ok(Type::Str),
            (Builtin::FormatDate, [Type::Int, _]) => Err(1),
            (Builtin::Random, [Type::Int, Type::Int]) => Ok(Type::Int),
            (Builtin::Random, [Type::Int, _]) => Err(1),
            (Builtin::SetStatus, [Type::Int]) => Ok(Type::Void),
            (Builtin::Redirect, [Type::Str]) => Ok(Type::Void),
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
            (
                Builtin::Upper | Builtin::Lower | Builtin::Trim | Builtin::EscapeHtml,
                [Type::Str],
            ) => Ok(Type::Str),
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
            (Builtin::Input, []) => io::read_line(),
            (Builtin::Now, []) => Ok(Value::Int(io::now_millis())),
            (Builtin::FormatDate, [Value::Int(millis), Value::Str(pattern)]) => {
                Ok(string_value(date::format_date(*millis, pattern.as_str())))
            }
            (Builtin::ReadFile, [Value::Str(dir), Value::Str(path)]) => {
                Ok(io::read_file(dir.as_str(), path.as_str()))
            }
            (Builtin::WriteFile, [Value::Str(dir), Value::Str(path), Value::Str(text)]) => {
                Ok(io::write_file(dir.as_str(), path.as_str(), text))
            }
            (Builtin::MakeDir, [Value::Str(dir), Value::Str(path)]) => {
                Ok(io::make_dir(dir.as_str(), path.as_str()))
            }
            (Builtin::AppendFile, [Value::Str(dir), Value::Str(path), Value::Str(text)]) => {
                Ok(io::append_file(dir.as_str(), path.as_str(), text))
            }
            (Builtin::CopyFile, [Value::Str(dir), Value::Str(from), Value::Str(to)]) => {
                Ok(io::copy_file(dir.as_str(), from.as_str(), to.as_str()))
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
            (Builtin::EscapeHtml, [Value::Str(s)]) => Ok(escape_html(s)),
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
pub(crate) fn string_value(s: String) -> Value {
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
