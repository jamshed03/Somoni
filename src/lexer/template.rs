//! Reading a template `name.template.smn`: HTML text with `{{ }}`, `{!! !!}` and `{{-- --}}`,
//! turned into the tokens of `write(...);` statements, so that the parser sees normal Somoni.

use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use super::Lexer;

/// The ending of a template file, like `.blade.php` in Laravel.
const TEMPLATE_ENDING: &str = ".template.smn";

/// What starts at a place in the HTML text of a template.
#[derive(Debug, Clone, Copy)]
enum Mark {
    /// `{{ value }}`, written with its HTML escaped.
    Echo,
    /// `{!! value !!}`, written as it is.
    Raw,
    /// `{{-- note --}}`, which writes nothing.
    Comment,
}

/// The marks with their text, `{{--` before `{{` so that a comment is not read as `{{`.
const MARKS: &[(&str, Mark)] = &[
    ("{{--", Mark::Comment),
    ("{!!", Mark::Raw),
    ("{{", Mark::Echo),
];

/// Returns true for a file that is read as a template.
pub fn is_template(name: &str) -> bool {
    name.ends_with(TEMPLATE_ENDING)
}

impl Lexer {
    /// Reads a whole template and returns the tokens of the statements it stands for, the last one always `Eof`.
    pub fn tokenize_template(&mut self) -> Result<Vec<SpannedToken>, SomoniError> {
        let mut tokens = Vec::new();
        loop {
            let (line, col) = (self.line, self.col());
            let text = self.read_html();
            if !text.is_empty() {
                let span = self.span_from(line, col);
                let call = write_start().chain([Token::Str(text)]).chain(write_end());
                push(&mut tokens, call, span);
            }
            let Some((mark_text, mark)) = self.mark_here() else {
                return self.finish(tokens);
            };
            let start = Span::in_line(self.file, self.line, self.col(), mark_text.len());
            self.pos += mark_text.len();
            match mark {
                Mark::Comment => self.skip_template_comment(start)?,
                Mark::Echo => self.read_echo(&mut tokens, "}}", true, start)?,
                Mark::Raw => self.read_echo(&mut tokens, "!!}", false, start)?,
            }
        }
    }

    /// Reads HTML text up to the next mark or the end of the template.
    fn read_html(&mut self) -> String {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if self.mark_here().is_some() {
                break;
            }
            text.push(c);
            if c == '\n' {
                self.newline();
            } else {
                self.advance();
            }
        }
        text
    }

    /// Returns the mark that starts at the current character, if there is one.
    fn mark_here(&self) -> Option<(&'static str, Mark)> {
        MARKS.iter().copied().find(|(text, _)| self.at(text))
    }

    /// Reads the value of `{{ value }}` or `{!! value !!}` up to `close`, as the tokens of `write("{value}".escape_html());`.
    fn read_echo(
        &mut self,
        tokens: &mut Vec<SpannedToken>,
        close: &'static str,
        escaped: bool,
        start: Span,
    ) -> Result<(), SomoniError> {
        push(
            tokens,
            write_start().chain([Token::StrStart(String::new())]),
            start,
        );
        let before = tokens.len();
        if !self.read_code(tokens, Some(close))? {
            return Err(SomoniError::at(
                start,
                ErrorCode::TemplateNotClosed { close },
            ));
        }
        if tokens.len() == before {
            return Err(SomoniError::at(start, ErrorCode::TemplateEmpty));
        }
        let mut end = vec![Token::StrEnd(String::new())];
        if escaped {
            end.extend([
                Token::Dot,
                Token::Ident("escape_html".to_string()),
                Token::LParen,
                Token::RParen,
            ]);
        }
        push(tokens, end.into_iter().chain(write_end()), start);
        Ok(())
    }

    /// Skips a comment `{{-- ... --}}` after its start, or returns L003 if it is not closed.
    fn skip_template_comment(&mut self, start: Span) -> Result<(), SomoniError> {
        while !self.at("--}}") {
            match self.peek() {
                None => return Err(SomoniError::at(start, ErrorCode::UnterminatedComment)),
                Some('\n') => self.newline(),
                Some(_) => self.advance(),
            }
        }
        self.pos += "--}}".len();
        Ok(())
    }
}

/// Returns the tokens `write(` that start every statement of a template.
fn write_start() -> impl Iterator<Item = Token> {
    [Token::Ident("write".to_string()), Token::LParen].into_iter()
}

/// Returns the tokens `);` that end every statement of a template.
fn write_end() -> impl Iterator<Item = Token> {
    [Token::RParen, Token::Semicolon].into_iter()
}

/// Adds `add` to `tokens`, all with the place `span`.
fn push(tokens: &mut Vec<SpannedToken>, add: impl IntoIterator<Item = Token>, span: Span) {
    tokens.extend(add.into_iter().map(|token| SpannedToken { token, span }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns the tokens of the template `source` as text, without `Eof`.
    fn template_tokens(source: &str) -> String {
        let tokens = Lexer::new(source, 0).tokenize_template().unwrap();
        let shown: Vec<String> = tokens
            .iter()
            .map(|t| format!("{:?}", t.token))
            .filter(|token| token != "Eof")
            .collect();
        shown.join(" ")
    }

    #[test]
    fn text_and_echo_become_write_calls() {
        assert_eq!(
            template_tokens("<b>{{ x }}</b>"),
            "Ident(\"write\") LParen Str(\"<b>\") RParen Semicolon \
             Ident(\"write\") LParen StrStart(\"\") Ident(\"x\") StrEnd(\"\") \
             Dot Ident(\"escape_html\") LParen RParen RParen Semicolon \
             Ident(\"write\") LParen Str(\"</b>\") RParen Semicolon"
        );
    }

    #[test]
    fn raw_is_not_escaped_and_comment_writes_nothing() {
        assert_eq!(
            template_tokens("{{-- note --}}{!! x !!}"),
            "Ident(\"write\") LParen StrStart(\"\") Ident(\"x\") StrEnd(\"\") RParen Semicolon"
        );
    }

    #[test]
    fn only_files_ending_in_template_smn_are_templates() {
        assert!(is_template("site/page.template.smn"));
        assert!(!is_template("site/page.smn"));
    }
}
