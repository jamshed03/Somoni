use std::fmt;

// In which station of the pipeline the error happened.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorKind {
    Lex,
    Parse,
    Type,
    Runtime,
}

// One error type for the whole pipeline: kind, line and text.
#[derive(Debug, Clone, PartialEq)]
pub struct SomoniError {
    pub kind: ErrorKind,
    // None: no line known (runtime errors, for now)
    pub line: Option<usize>,
    pub message: String,
}

impl SomoniError {
    pub fn lex(line: usize, message: impl Into<String>) -> SomoniError {
        SomoniError::new(ErrorKind::Lex, Some(line), message)
    }

    pub fn parse(line: usize, message: impl Into<String>) -> SomoniError {
        SomoniError::new(ErrorKind::Parse, Some(line), message)
    }

    pub fn type_error(line: usize, message: impl Into<String>) -> SomoniError {
        SomoniError::new(ErrorKind::Type, Some(line), message)
    }

    pub fn runtime(message: impl Into<String>) -> SomoniError {
        SomoniError::new(ErrorKind::Runtime, None, message)
    }

    pub fn with_line_if_missing(mut self, line: Option<usize>) -> SomoniError {
        if self.line.is_none() {
            self.line = line;
        }
        self
    }

    fn new(kind: ErrorKind, line: Option<usize>, message: impl Into<String>) -> SomoniError {
        SomoniError {
            kind,
            line,
            message: message.into(),
        }
    }
}

// How the error is shown to the user, e.g. "Type error: Line 3: unknown variable 'x'"
impl fmt::Display for SomoniError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let kind = match self.kind {
            ErrorKind::Lex => "Lex error",
            ErrorKind::Parse => "Parse error",
            ErrorKind::Type => "Type error",
            ErrorKind::Runtime => "Runtime error",
        };
        match self.line {
            Some(line) => write!(f, "{}: Line {}: {}", kind, line, self.message),
            None => write!(f, "{}: {}", kind, self.message),
        }
    }
}
