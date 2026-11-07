//! Reading from outside the program: `input()` reads a line that the user types.

use std::io::BufRead;

use crate::error::{ErrorCode, SomoniError};
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
