//! Raw strings `"""..."""`: no escapes and no `{...}`, with the indentation of the closing `"""` taken off every line.

use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::Token;

use super::Lexer;

/// The quotes that open and close a raw string.
pub(super) const RAW_QUOTES: &str = "\"\"\"";

impl Lexer {
    /// Reads a raw string `"""..."""` as a normal string token; its lines are shaped by `shape_raw`.
    pub(super) fn read_raw_string(&mut self) -> Result<Token, SomoniError> {
        let start = Span::in_line(self.file, self.line, self.col(), RAW_QUOTES.len());
        self.pos += RAW_QUOTES.len();
        let mut text = String::new();
        while !self.at(RAW_QUOTES) {
            match self.peek() {
                None => return Err(SomoniError::at(start, ErrorCode::UnterminatedString)),
                Some('\n') => {
                    text.push('\n');
                    self.newline();
                }
                Some(c) => {
                    text.push(c);
                    self.advance();
                }
            }
        }
        self.pos += RAW_QUOTES.len();
        shape_raw(&text).map(Token::Str).map_err(|offset| {
            let line = start.line as usize + offset;
            let place = Span::in_line(self.file, line, 1, 1);
            SomoniError::at(place, ErrorCode::RawStringIndent)
        })
    }
}

/// Returns the text of a raw string: written over several lines, the line break after the opening `"""` and the one
/// before the closing `"""` go, and the indentation of the closing line is taken off every line;
/// `Err(n)` is the line `n` after the opening one that has less indentation.
fn shape_raw(text: &str) -> Result<String, usize> {
    let Some((first, body)) = text.split_once('\n') else {
        return Ok(text.to_string());
    };
    if !is_blank(first) {
        return Ok(text.to_string());
    }
    let (content, closing) = body.rsplit_once('\n').unwrap_or(("", body));
    if !is_blank(closing) {
        return Ok(body.to_string());
    }
    let lines = content
        .split('\n')
        .enumerate()
        .map(|(place, line)| match line.strip_prefix(closing) {
            Some(rest) => Ok(rest),
            None if is_blank(line) => Ok(""),
            None => Err(place + 1),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(lines.join("\n"))
}

/// Returns true if `line` holds only spaces and tabs.
fn is_blank(line: &str) -> bool {
    line.chars().all(|c| c == ' ' || c == '\t')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indentation_of_the_closing_quotes_goes() {
        let text = "\n        a(\"x\");\n          b\n\n        c\n        ";
        assert_eq!(shape_raw(text), Ok("a(\"x\");\n  b\n\nc".to_string()));
    }

    #[test]
    fn one_line_stays_as_it_is() {
        assert_eq!(shape_raw(" a \"b\" {c} "), Ok(" a \"b\" {c} ".to_string()));
    }

    #[test]
    fn closing_quotes_after_text_keep_the_lines() {
        assert_eq!(shape_raw("\n  a\n  b"), Ok("  a\n  b".to_string()));
    }

    #[test]
    fn a_line_with_less_indentation_is_an_error() {
        assert_eq!(shape_raw("\n    a\n  b\n    "), Err(2));
    }

    #[test]
    fn empty_raw_string_over_two_lines() {
        assert_eq!(shape_raw("\n    "), Ok(String::new()));
    }
}
