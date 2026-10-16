use std::fmt;

use crate::span::Span;
use crate::token::Token;
use crate::types::Type;

// In which station of the pipeline the error happened.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorKind {
    Lex,
    Parse,
    Type,
    Runtime,
}

// The catalog of all errors Somoni knows, each with an id and a message.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCode {
    // lexer
    UnexpectedCharacter {
        found: char,
    },
    UnterminatedString,
    UnterminatedComment,
    IntegerTooLarge {
        text: String,
    },
    InvalidFloat {
        text: String,
    },

    // parser
    Expected {
        what: String,
    },
    UnexpectedToken {
        found: Token,
    },
    BlockNotClosed,
    ForStartConst,

    // type checker
    UnknownType {
        name: String,
    },
    UnknownVariable {
        name: String,
    },
    CannotAssign {
        found: Type,
        expected: Type,
    },
    AlreadyDeclared {
        name: String,
    },
    AssignToConst {
        name: String,
    },
    OperandTypesDiffer {
        left: Type,
        right: Type,
    },
    OperatorNotAllowed {
        found: Type,
    },
    NotNeedsBool {
        found: Type,
    },
    NegNeedsNumber {
        found: Type,
    },
    ConditionNotBool {
        found: Type,
    },
    OutsideLoop {
        keyword: &'static str,
    },
    UnknownFunction {
        name: String,
    },
    WrongArgCount {
        name: &'static str,
        expected: usize,
        found: usize,
    },
    WrongArgType {
        name: &'static str,
        found: Type,
    },

    // runtime
    Internal {
        message: String,
    },
    DivisionByZero,
    IntegerOverflow,
    InvalidConversion {
        text: String,
        target: Type,
    },
}

impl ErrorCode {
    pub fn code(&self) -> &'static str {
        match self {
            ErrorCode::UnexpectedCharacter { .. } => "L001",
            ErrorCode::UnterminatedString => "L002",
            ErrorCode::UnterminatedComment => "L003",
            ErrorCode::IntegerTooLarge { .. } => "L004",
            ErrorCode::InvalidFloat { .. } => "L005",

            ErrorCode::Expected { .. } => "P001",
            ErrorCode::UnexpectedToken { .. } => "P002",
            ErrorCode::BlockNotClosed => "P003",
            ErrorCode::ForStartConst => "P004",

            ErrorCode::UnknownType { .. } => "T001",
            ErrorCode::UnknownVariable { .. } => "T002",
            ErrorCode::CannotAssign { .. } => "T003",
            ErrorCode::AlreadyDeclared { .. } => "T004",
            ErrorCode::AssignToConst { .. } => "T005",
            ErrorCode::OperandTypesDiffer { .. } => "T006",
            ErrorCode::OperatorNotAllowed { .. } => "T007",
            ErrorCode::NotNeedsBool { .. } => "T008",
            ErrorCode::NegNeedsNumber { .. } => "T009",
            ErrorCode::ConditionNotBool { .. } => "T010",
            ErrorCode::OutsideLoop { .. } => "T011",
            ErrorCode::UnknownFunction { .. } => "T012",
            ErrorCode::WrongArgCount { .. } => "T013",
            ErrorCode::WrongArgType { .. } => "T014",

            // should never happen: the type checker rejects such programs (a bug in Somoni)
            ErrorCode::Internal { .. } => "R000",
            ErrorCode::DivisionByZero => "R001",
            ErrorCode::IntegerOverflow => "R002",
            ErrorCode::InvalidConversion { .. } => "R003",
        }
    }

    pub fn kind(&self) -> ErrorKind {
        match self {
            ErrorCode::UnexpectedCharacter { .. }
            | ErrorCode::UnterminatedString
            | ErrorCode::UnterminatedComment
            | ErrorCode::IntegerTooLarge { .. }
            | ErrorCode::InvalidFloat { .. } => ErrorKind::Lex,

            ErrorCode::Expected { .. }
            | ErrorCode::UnexpectedToken { .. }
            | ErrorCode::BlockNotClosed
            | ErrorCode::ForStartConst => ErrorKind::Parse,

            ErrorCode::UnknownType { .. }
            | ErrorCode::UnknownVariable { .. }
            | ErrorCode::CannotAssign { .. }
            | ErrorCode::AlreadyDeclared { .. }
            | ErrorCode::AssignToConst { .. }
            | ErrorCode::OperandTypesDiffer { .. }
            | ErrorCode::OperatorNotAllowed { .. }
            | ErrorCode::NotNeedsBool { .. }
            | ErrorCode::NegNeedsNumber { .. }
            | ErrorCode::ConditionNotBool { .. }
            | ErrorCode::OutsideLoop { .. }
            | ErrorCode::UnknownFunction { .. }
            | ErrorCode::WrongArgCount { .. }
            | ErrorCode::WrongArgType { .. } => ErrorKind::Type,

            ErrorCode::Internal { .. }
            | ErrorCode::DivisionByZero
            | ErrorCode::IntegerOverflow
            | ErrorCode::InvalidConversion { .. } => ErrorKind::Runtime,
        }
    }

    // lowercase, like rustc: the message follows "Line N: "
    pub fn message(&self) -> String {
        match self {
            ErrorCode::UnexpectedCharacter { found } => format!("unexpected character '{}'", found),
            ErrorCode::UnterminatedString => "unterminated string".to_string(),
            ErrorCode::UnterminatedComment => "unterminated comment".to_string(),
            ErrorCode::IntegerTooLarge { text } => format!("integer too large '{}'", text),
            ErrorCode::InvalidFloat { text } => format!("invalid float '{}'", text),

            ErrorCode::Expected { what } => format!("expected {}", what),
            ErrorCode::UnexpectedToken { found } => format!("unexpected token {:?}", found),
            ErrorCode::BlockNotClosed => "block is not closed, expected '}'".to_string(),
            ErrorCode::ForStartConst => "for loop start must be 'var', not 'const'".to_string(),

            ErrorCode::UnknownType { name } => format!("unknown type '{}'", name),
            ErrorCode::UnknownVariable { name } => format!("unknown variable '{}'", name),
            ErrorCode::CannotAssign { found, expected } => {
                format!("cannot assign {} to {}", found.name(), expected.name())
            }
            ErrorCode::AlreadyDeclared { name } => {
                format!("'{}' is already declared in this block", name)
            }
            ErrorCode::AssignToConst { name } => format!("cannot assign to const '{}'", name),
            ErrorCode::OperandTypesDiffer { left, right } => {
                format!(
                    "cannot use operator on {} and {}",
                    left.name(),
                    right.name()
                )
            }
            ErrorCode::OperatorNotAllowed { found } => {
                format!("operator not allowed for {}", found.name())
            }
            ErrorCode::NotNeedsBool { found } => format!("'!' needs bool, got {}", found.name()),
            ErrorCode::NegNeedsNumber { found } => {
                format!("'-' needs int or float, got {}", found.name())
            }
            ErrorCode::ConditionNotBool { found } => {
                format!("condition must be bool, got {}", found.name())
            }
            ErrorCode::OutsideLoop { keyword } => format!("'{}' outside of a loop", keyword),
            ErrorCode::UnknownFunction { name } => format!("unknown function '{}'", name),
            ErrorCode::WrongArgCount {
                name,
                expected,
                found,
            } => {
                let word = if *expected == 1 {
                    "argument"
                } else {
                    "arguments"
                };
                format!("'{}' expects {} {}, got {}", name, expected, word, found)
            }
            ErrorCode::WrongArgType { name, found } => {
                format!("cannot use {} as argument of '{}'", found.name(), name)
            }

            ErrorCode::Internal { message } => format!("internal error: {}", message),
            ErrorCode::DivisionByZero => "division by zero".to_string(),
            ErrorCode::IntegerOverflow => "integer overflow".to_string(),
            ErrorCode::InvalidConversion { text, target } => {
                format!("cannot convert '{}' to {}", text, target.name())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SomoniError {
    pub code: ErrorCode,
    pub span: Option<Span>,
}

impl SomoniError {
    pub fn at(span: Span, code: ErrorCode) -> SomoniError {
        SomoniError {
            code,
            span: Some(span),
        }
    }

    // R000: a check the type checker should have made failed (a bug in Somoni)
    pub fn internal(message: impl Into<String>) -> SomoniError {
        ErrorCode::Internal {
            message: message.into(),
        }
        .into()
    }

    pub fn render(&self, source: &str) -> String {
        let header = self.to_string();
        let Some(span) = self.span else {
            return header;
        };
        let Some(text) = source.lines().nth(span.line - 1) else {
            return header;
        };

        let end_col = if span.end_line == span.line {
            span.end_col
        } else {
            text.chars().count() + 1
        };
        let width = end_col.saturating_sub(span.col).max(1);
        let indent: String = text
            .chars()
            .take(span.col - 1)
            .map(|c| if c == '\t' { '\t' } else { ' ' })
            .collect();

        let number = span.line.to_string();
        let gutter = " ".repeat(number.len());
        format!(
            "{}\n  {} | {}\n  {} | {}{}",
            header,
            number,
            text,
            gutter,
            indent,
            "^".repeat(width)
        )
    }

    pub fn with_span_if_missing(mut self, span: Option<Span>) -> SomoniError {
        if self.span.is_none() {
            self.span = span;
        }
        self
    }
}

// ErrorCode -> SomoniError without a place, used for runtime errors
impl From<ErrorCode> for SomoniError {
    fn from(code: ErrorCode) -> SomoniError {
        SomoniError { code, span: None }
    }
}

// e.g. "Type error [T002]: Line 4: unknown variable 'y'"
impl fmt::Display for SomoniError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let kind = match self.code.kind() {
            ErrorKind::Lex => "Lex error",
            ErrorKind::Parse => "Parse error",
            ErrorKind::Type => "Type error",
            ErrorKind::Runtime => "Runtime error",
        };
        let code = self.code.code();
        let message = self.code.message();
        match self.span {
            Some(span) => write!(f, "{} [{}]: Line {}: {}", kind, code, span.line, message),
            None => write!(f, "{} [{}]: {}", kind, code, message),
        }
    }
}
