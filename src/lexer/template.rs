//! Reading a template `name.template.smn`: HTML text with `{{ }}`, `{!! !!}`, `{{-- --}}` and directives like
//! `@if`, turned into the tokens of normal Somoni (`write(...);`, `if`, `for` and `{ }`), so the parser sees no template.

use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use super::Lexer;

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

/// A directive: a part of an `if` or a `for` loop, or `@import` and `@props` at the top of a template.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Directive {
    If,
    ElseIf,
    Else,
    EndIf,
    Foreach,
    EndForeach,
    Import,
    Props,
}

/// Every directive, so that a name only has to be written in `shown`.
const DIRECTIVES: &[Directive] = &[
    Directive::If,
    Directive::ElseIf,
    Directive::Else,
    Directive::EndIf,
    Directive::Foreach,
    Directive::EndForeach,
    Directive::Import,
    Directive::Props,
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

impl Directive {
    /// Returns the directive as it is written, like `@if`.
    fn shown(self) -> &'static str {
        match self {
            Directive::If => "@if",
            Directive::ElseIf => "@elseif",
            Directive::Else => "@else",
            Directive::EndIf => "@endif",
            Directive::Foreach => "@foreach",
            Directive::EndForeach => "@endforeach",
            Directive::Import => "@import",
            Directive::Props => "@props",
        }
    }

    /// Returns true for `@import` and `@props`, which may only stand at the top of a template.
    fn is_header(self) -> bool {
        matches!(self, Directive::Import | Directive::Props)
    }

    /// Returns an example of the `( ... )` after the directive, for L012.
    fn example(self) -> &'static str {
        match self {
            Directive::If | Directive::ElseIf => "(x > 0)",
            Directive::Foreach => "(item: string in items)",
            Directive::Props => "(title: string)",
            Directive::Else | Directive::EndIf | Directive::EndForeach | Directive::Import => "( )",
        }
    }

    /// Returns the word after the `@`.
    fn word(self) -> &'static str {
        self.shown().trim_start_matches('@')
    }

    /// Returns the directive written as `@word`, if there is one.
    fn from_word(word: &str) -> Option<Directive> {
        DIRECTIVES
            .iter()
            .copied()
            .find(|directive| directive.word() == word)
    }

    /// Returns the directive that must be open before this one, like `@if` before `@else`.
    fn needs_open(self) -> Option<Directive> {
        match self {
            Directive::ElseIf | Directive::Else | Directive::EndIf => Some(Directive::If),
            Directive::EndForeach => Some(Directive::Foreach),
            Directive::If | Directive::Foreach | Directive::Import | Directive::Props => None,
        }
    }

    /// Returns the directive that closes this one, like `@endif` for `@if`.
    fn closer(self) -> Directive {
        match self {
            Directive::Foreach | Directive::EndForeach => Directive::EndForeach,
            Directive::If | Directive::ElseIf | Directive::Else | Directive::EndIf => {
                Directive::EndIf
            }
            Directive::Import | Directive::Props => self,
        }
    }
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

    /// Reads a directive after its name as the tokens it stands for, like `} else if (...) {` for `@elseif (...)`;
    /// `open` holds the directives that still wait for their end.
    fn read_directive(
        &mut self,
        directive: Directive,
        start: Span,
        tokens: &mut Vec<SpannedToken>,
        open: &mut Vec<(Directive, Span)>,
    ) -> Result<(), SomoniError> {
        if let Some(needed) = directive.needs_open() {
            check_open(directive, needed, open.last(), start)?;
        }
        match directive {
            Directive::If | Directive::Foreach => {
                let keyword = if directive == Directive::If {
                    Token::If
                } else {
                    Token::For
                };
                push(tokens, [keyword], start);
                self.read_condition(directive, start, tokens)?;
                push(tokens, [Token::LBrace], start);
                open.push((directive, start));
            }
            Directive::ElseIf => {
                push(tokens, [Token::RBrace, Token::Else, Token::If], start);
                self.read_condition(directive, start, tokens)?;
                push(tokens, [Token::LBrace], start);
            }
            Directive::Else => push(tokens, [Token::RBrace, Token::Else, Token::LBrace], start),
            Directive::EndIf | Directive::EndForeach => {
                open.pop();
                push(tokens, [Token::RBrace], start);
            }
            Directive::Import | Directive::Props => {
                return Err(SomoniError::internal("header directive in the body"));
            }
        }
        Ok(())
    }

    /// Reads the `( ... )` after `@if`, `@elseif` or `@foreach` up to its matching `)`, or returns L012.
    fn read_condition(
        &mut self,
        directive: Directive,
        start: Span,
        tokens: &mut Vec<SpannedToken>,
    ) -> Result<(), SomoniError> {
        let missing = || {
            let code = ErrorCode::DirectiveCondition {
                directive: directive.shown(),
                example: directive.example(),
            };
            SomoniError::at(start, code)
        };
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.advance();
        }
        if self.peek() != Some('(') {
            return Err(missing());
        }
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            if self.skip_space_or_comment(c)? {
                continue;
            }
            let (line, col) = (self.line, self.col());
            let token = self.read_token(c)?;
            match token {
                Token::LParen => depth += 1,
                Token::RParen => depth = depth.saturating_sub(1),
                _ => {}
            }
            let span = self.span_from(line, col);
            tokens.push(SpannedToken { token, span });
            if depth == 0 {
                return Ok(());
            }
        }
        Err(missing())
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

/// Checks that `innermost`, the innermost open directive, is the `needed` one for `directive`, or returns L010 or L013.
fn check_open(
    directive: Directive,
    needed: Directive,
    innermost: Option<&(Directive, Span)>,
    start: Span,
) -> Result<(), SomoniError> {
    let code = match innermost {
        Some((open, _)) if *open == needed => return Ok(()),
        Some((open, open_span)) => ErrorCode::DirectiveWrongEnd {
            found: directive.shown(),
            open: open.shown(),
            line: open_span.line,
            close: open.closer().shown(),
        },
        None => ErrorCode::DirectiveWithoutOpen {
            found: directive.shown(),
            needs: needed.shown(),
        },
    };
    Err(SomoniError::at(start, code))
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
