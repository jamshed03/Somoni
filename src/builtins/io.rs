//! Talking to the outside of the program: `input()` reads a line that the user types, `read_file`, `write_file`,
//! `append_file` and `copy_file` work with whole files, `make_dir` makes folders, `now()` reads the clock. `print` and `write` are written by the VM (`vm/output.rs`).

use std::io::{BufRead, ErrorKind, Write};
use std::path::Path;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{ErrorCode, SomoniError};
use crate::text::Text;
use crate::value::Value;

use super::string_value;

/// Reads one line without its line break, or returns `null` when the input has ended (Ctrl+D or the end of a file).
pub(super) fn read_line() -> Result<Value, SomoniError> {
    let mut line = String::new();
    let read = std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|error| ErrorCode::InputFailed {
            reason: error.to_string(),
        })?;
    if read == 0 {
        return Ok(Value::Null);
    }
    let text = line.strip_suffix('\n').unwrap_or(&line);
    let text = text.strip_suffix('\r').unwrap_or(text);
    Ok(string_value(text.to_string()))
}

/// Reads the file `path`, which starts at the folder `dir`, as `ok(text)`, or `err(reason)` if that is not possible.
pub(super) fn read_file(dir: &str, path: &str) -> Value {
    match std::fs::read_to_string(Path::new(dir).join(path)) {
        Ok(text) => Value::Ok(Rc::new(string_value(text))),
        Err(error) => failed("read", path, error.kind()),
    }
}

/// Writes `text` into the file `path`, which starts at the folder `dir`, as `ok(characters)`, or `err(reason)`.
pub(super) fn write_file(dir: &str, path: &str, text: &Text) -> Value {
    match std::fs::write(Path::new(dir).join(path), text.as_str()) {
        Ok(()) => Value::Ok(Rc::new(Value::Int(text.char_count() as i64))),
        Err(error) => failed("write", path, error.kind()),
    }
}

/// Adds `text` to the end of the file `path`, which starts at the folder `dir` and is made if it is missing, as
/// `ok(characters)`, or `err(reason)`.
pub(super) fn append_file(dir: &str, path: &str, text: &Text) -> Value {
    let appended = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(Path::new(dir).join(path))
        .and_then(|mut file| file.write_all(text.as_str().as_bytes()));
    match appended {
        Ok(()) => Value::Ok(Rc::new(Value::Int(text.char_count() as i64))),
        Err(error) => failed("write", path, error.kind()),
    }
}

/// Makes the folder `path` and the folders above it that are missing, as `ok(true)`, `ok(false)` if it was there
/// already, or `err(reason)`.
pub(super) fn make_dir(dir: &str, path: &str) -> Value {
    let full = Path::new(dir).join(path);
    if full.is_dir() {
        return Value::Ok(Rc::new(Value::Bool(false)));
    }
    match std::fs::create_dir_all(full) {
        Ok(()) => Value::Ok(Rc::new(Value::Bool(true))),
        Err(error) => failed("make", path, error.kind()),
    }
}

/// Copies the file `from` to `to`, both starting at the folder `dir`, as `ok(bytes)`, or `err(reason)`.
/// Copies raw bytes rather than text, so it also works for files like `.png` that are not valid UTF-8.
pub(super) fn copy_file(dir: &str, from: &str, to: &str) -> Value {
    let bytes = match std::fs::read(Path::new(dir).join(from)) {
        Ok(bytes) => bytes,
        Err(error) => return failed("read", from, error.kind()),
    };
    match std::fs::write(Path::new(dir).join(to), &bytes) {
        Ok(()) => Value::Ok(Rc::new(Value::Int(bytes.len() as i64))),
        Err(error) => failed("write", to, error.kind()),
    }
}

/// Returns the milliseconds since 1970-01-01 (UTC), or 0 if the clock of the system stands before that.
pub(super) fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

/// Returns `err("cannot read 'path': not found")` and the like, with a reason that is the same on every system.
fn failed(action: &str, path: &str, kind: ErrorKind) -> Value {
    let reason = match kind {
        ErrorKind::NotFound => "not found",
        ErrorKind::PermissionDenied => "no permission",
        ErrorKind::InvalidData => "not UTF-8 text",
        ErrorKind::IsADirectory => "it is a folder",
        _ => "the system refused",
    };
    let message = format!("cannot {} '{}': {}", action, path, reason);
    Value::Err(Rc::new(string_value(message)))
}
