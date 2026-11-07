//! Where `print` and `write` go: into the innermost running `capture(...)`, or else to the terminal.

use std::io::Write;

use crate::error::{ErrorCode, SomoniError};

/// The output of a running program.
#[derive(Default)]
pub(super) struct Output {
    /// The texts of the `capture(...)` calls that are running, the innermost last.
    captures: Vec<String>,
}

impl Output {
    /// Adds `text` to the innermost capture, or writes it to the terminal at once, or returns R012 if that fails.
    pub(super) fn write(&mut self, text: &str) -> Result<(), SomoniError> {
        if let Some(capture) = self.captures.last_mut() {
            capture.push_str(text);
            return Ok(());
        }
        let mut out = std::io::stdout().lock();
        out.write_all(text.as_bytes())
            .and_then(|()| out.flush())
            .map_err(|error| {
                ErrorCode::OutputFailed {
                    reason: error.to_string(),
                }
                .into()
            })
    }

    /// Starts a capture, which keeps everything written until `end_capture`.
    pub(super) fn start_capture(&mut self) {
        self.captures.push(String::new());
    }

    /// Ends the innermost capture and returns what was written into it.
    pub(super) fn end_capture(&mut self) -> String {
        self.captures.pop().unwrap_or_default()
    }
}
