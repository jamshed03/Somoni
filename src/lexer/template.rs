//! Reading a template `name.template.smn`: HTML text with `{{ }}`, `{!! !!}`, `{{-- --}}` and directives like
//! `@if`, turned into the tokens of normal Somoni (`write(...);`, `if`, `for` and `{ }`), so the parser sees no template.

use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use super::Lexer;

use directive::Directive;

mod directive;

/// The ending of a template file, like `.blade.php` in Laravel.
const TEMPLATE_ENDING: &str = ".template.smn";

/// What starts at a place in the HTML text of a template.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mark {
    /// `{{ value }}`, written with its HTML escaped.
    Echo,
    /// `{!! value !!}`, written as it is.
    Raw,
    /// `{{-- note --}}`, which writes nothing.
    Comment,
    /// `@if (...)`, `@endforeach` and the others.
    Directive(Directive),
}

impl Mark {
    /// Returns true for a mark that takes its whole line with it when it stands alone on it.
    fn takes_its_line(self) -> bool {
        matches!(self, Mark::Comment | Mark::Directive(_))
    }
}

/// The marks written with symbols, `{{--` before `{{` so that a comment is not read as `{{`.
const MARKS: &[(&str, Mark)] = &[
    ("{{--", Mark::Comment),
    ("{!!", Mark::Raw),
    ("{{", Mark::Echo),
];

/// What the lexer knows while it reads a template.
struct TemplateState {
    tokens: Vec<SpannedToken>,
    /// The `@if` and `@foreach` that still wait for their end, with their places.
    open: Vec<(Directive, Span)>,
    /// True until the first text, value or directive: only here `@import` and `@props` may stand.
    in_header: bool,
    /// True after `@props`, whose function is closed at the end of the template.
    is_function: bool,
}

/// Returns true for a file that is read as a template.
pub fn is_template(name: &str) -> bool {
    name.ends_with(TEMPLATE_ENDING)
}

impl Lexer {
    /// Reads the whole template in the file `name` and returns the tokens of the statements it stands for,
    /// the last one always `Eof`.
    pub fn tokenize_template(&mut self, name: &str) -> Result<Vec<SpannedToken>, SomoniError> {
        let mut state = TemplateState {
            tokens: Vec::new(),
            open: Vec::new(),
            in_header: true,
            is_function: false,
        };
        loop {
            let (line, col) = (self.line, self.col());
            let mut text = self.read_html();
            let text_span = self.span_from(line, col);
            let blank_header = state.in_header && text.trim().is_empty();
            let Some((width, mark)) = self.mark_here() else {
                if !blank_header {
                    push_text(&mut state.tokens, text, text_span);
                }
                return self.finish_template(state);
            };
            let alone_before = mark.takes_its_line() && self.blank_before();
            let start = Span::in_line(self.file, self.line, self.col(), width);
            self.pos += width;
            let mut mark_tokens = Vec::new();
            match mark {
                Mark::Directive(directive) if directive.is_header() => {
                    if !blank_header {
                        let code = ErrorCode::TemplateHeaderLate {
                            directive: directive.shown(),
                        };
                        return Err(SomoniError::at(start, code));
                    }
                    self.read_header(directive, name, start, &mut mark_tokens, &mut state)?;
                }
                Mark::Comment => {
                    state.in_header = blank_header;
                    self.skip_template_comment(start)?;
                }
                Mark::Echo | Mark::Raw | Mark::Directive(_) => {
                    state.in_header = false;
                    self.read_body_mark(mark, start, &mut mark_tokens, &mut state.open)?;
                }
            }
            if alone_before && self.skip_blank_line_end() {
                let kept = text.trim_end_matches([' ', '\t']).len();
                text.truncate(kept);
            }
            if !blank_header {
                push_text(&mut state.tokens, text, text_span);
            }
            state.tokens.extend(mark_tokens);
        }
    }

    /// Reads `{{ }}`, `{!! !!}` or a directive like `@if` after its start.
    fn read_body_mark(
        &mut self,
        mark: Mark,
        start: Span,
        tokens: &mut Vec<SpannedToken>,
        open: &mut Vec<(Directive, Span)>,
    ) -> Result<(), SomoniError> {
        match mark {
            Mark::Echo => self.read_echo(tokens, "}}", true, start),
            Mark::Raw => self.read_echo(tokens, "!!}", false, start),
            Mark::Directive(directive) => self.read_directive(directive, start, tokens, open),
            Mark::Comment => self.skip_template_comment(start),
        }
    }

    /// Reads `@import ...` as `import ...;` or `@props(...)` as `export fn name(...): void {`, named after the file `name`.
    fn read_header(
        &mut self,
        directive: Directive,
        name: &str,
        start: Span,
        tokens: &mut Vec<SpannedToken>,
        state: &mut TemplateState,
    ) -> Result<(), SomoniError> {
        if directive == Directive::Import {
            push(tokens, [Token::Import], start);
            self.read_rest_of_line(tokens)?;
            push(tokens, [Token::Semicolon], start);
            return Ok(());
        }
        let Some(function) = function_name(name) else {
            let code = ErrorCode::TemplateName {
                file: name.to_string(),
            };
            return Err(SomoniError::at(start, code));
        };
        push(
            tokens,
            [Token::Export, Token::Fn, Token::Ident(function)],
            start,
        );
        self.read_condition(directive, start, tokens)?;
        let void = Token::Ident("void".to_string());
        push(tokens, [Token::Colon, void, Token::LBrace], start);
        state.in_header = false;
        state.is_function = true;
        Ok(())
    }

    /// Reads code up to the end of the line, like `{ Page } from "pages.smn"` after `@import`.
    fn read_rest_of_line(&mut self, tokens: &mut Vec<SpannedToken>) -> Result<(), SomoniError> {
        loop {
            while matches!(self.peek(), Some(' ' | '\t')) {
                self.advance();
            }
            let Some(c) = self.peek().filter(|&c| c != '\n') else {
                return Ok(());
            };
            let (line, col) = (self.line, self.col());
            let token = self.read_token(c)?;
            let span = self.span_from(line, col);
            tokens.push(SpannedToken { token, span });
        }
    }

    /// Reads HTML text up to the next mark or the end of the template; `@@` stands for `@` and `@{{` for `{{`.
    fn read_html(&mut self) -> String {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if self.at("@@") {
                text.push('@');
                self.pos += 2;
                continue;
            }
            if self.at("@{{") {
                text.push_str("{{");
                self.pos += 3;
                continue;
            }
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

    /// Returns the mark that starts at the current character and how many characters it has, if there is one.
    fn mark_here(&self) -> Option<(usize, Mark)> {
        if let Some((text, mark)) = MARKS.iter().copied().find(|(text, _)| self.at(text)) {
            return Some((text.len(), mark));
        }
        if self.peek() != Some('@') {
            return None;
        }
        let word: String = self.chars[self.pos + 1..]
            .iter()
            .take_while(|c| c.is_alphanumeric() || **c == '_')
            .collect();
        let directive = Directive::from_word(&word)?;
        Some((1 + word.chars().count(), Mark::Directive(directive)))
    }

    /// Returns true if only spaces and tabs stand between the start of the line and the current character.
    fn blank_before(&self) -> bool {
        self.chars[self.line_start..self.pos]
            .iter()
            .all(|c| matches!(c, ' ' | '\t'))
    }

    /// Skips the rest of the line with its line break if only spaces and tabs are left in it, and returns true if it did.
    fn skip_blank_line_end(&mut self) -> bool {
        let rest = self.chars[self.pos..]
            .iter()
            .take_while(|c| matches!(c, ' ' | '\t'))
            .count();
        match self.chars.get(self.pos + rest) {
            Some('\n') => {
                self.pos += rest;
                self.newline();
                true
            }
            None => {
                self.pos += rest;
                true
            }
            Some(_) => false,
        }
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

    /// Checks that every directive is closed, or returns L011, closes the function of `@props` and adds the `Eof` token.
    fn finish_template(&self, mut state: TemplateState) -> Result<Vec<SpannedToken>, SomoniError> {
        if let Some((directive, span)) = state.open.last() {
            let code = ErrorCode::DirectiveNotClosed {
                open: directive.shown(),
                close: directive.closer().shown(),
            };
            return Err(SomoniError::at(*span, code));
        }
        if state.is_function {
            let end = self.span_from(self.line, self.col());
            push(&mut state.tokens, [Token::RBrace], end);
        }
        self.finish(state.tokens)
    }
}

/// Returns the name of the function of a template, `card` for `site/card.template.smn`, if it is a valid name.
fn function_name(file: &str) -> Option<String> {
    let base = file
        .rsplit(['/', '\\'])
        .next()?
        .strip_suffix(TEMPLATE_ENDING)?;
    let mut chars = base.chars();
    let starts_well = chars.next().is_some_and(|c| c.is_alphabetic() || c == '_');
    let rest_fits = chars.all(|c| c.is_alphanumeric() || c == '_');
    (starts_well && rest_fits).then(|| base.to_string())
}

/// Adds `write("text");` for a piece of HTML text, or nothing if it is empty.
fn push_text(tokens: &mut Vec<SpannedToken>, text: String, span: Span) {
    if !text.is_empty() {
        let call = write_start().chain([Token::Str(text)]).chain(write_end());
        push(tokens, call, span);
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
        let tokens = Lexer::new(source, 0)
            .tokenize_template("test.template.smn")
            .unwrap();
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
    fn directives_become_if_and_for_and_their_lines_disappear() {
        assert_eq!(
            template_tokens("@if (a)\nx\n@else\ny\n@endif\n"),
            "If LParen Ident(\"a\") RParen LBrace \
             Ident(\"write\") LParen Str(\"x\\n\") RParen Semicolon \
             RBrace Else LBrace \
             Ident(\"write\") LParen Str(\"y\\n\") RParen Semicolon \
             RBrace"
        );
    }

    #[test]
    fn at_sign_before_an_unknown_word_is_text() {
        assert_eq!(
            template_tokens("a@b @@if"),
            "Ident(\"write\") LParen Str(\"a@b @if\") RParen Semicolon"
        );
    }

    #[test]
    fn props_make_an_exported_function_named_after_the_file() {
        let tokens = Lexer::new("@props(a: int)\n{{ a }}", 0)
            .tokenize_template("site/card.template.smn")
            .unwrap();
        let shown: Vec<String> = tokens.iter().map(|t| format!("{:?}", t.token)).collect();
        assert_eq!(
            shown[..9].join(" "),
            "Export Fn Ident(\"card\") LParen Ident(\"a\") Colon Ident(\"int\") RParen Colon"
        );
        assert_eq!(shown[shown.len() - 2..].join(" "), "RBrace Eof");
    }

    #[test]
    fn function_names_come_from_valid_file_names() {
        assert_eq!(
            function_name("a/b/card.template.smn"),
            Some("card".to_string())
        );
        assert_eq!(function_name("bad-name.template.smn"), None);
        assert_eq!(function_name("1st.template.smn"), None);
    }

    #[test]
    fn only_files_ending_in_template_smn_are_templates() {
        assert!(is_template("site/page.template.smn"));
        assert!(!is_template("site/page.smn"));
    }
}
