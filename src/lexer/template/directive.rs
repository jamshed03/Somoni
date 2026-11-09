//! The directives of a template, like `@if (...)` and `@endforeach`, and the tokens they stand for.

use crate::error::{ErrorCode, SomoniError};
use crate::lexer::Lexer;
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use super::push;

/// A directive: a part of an `if` or a `for` loop, or `@import` and `@props` at the top of a template.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Directive {
    If,
    ElseIf,
    Else,
    EndIf,
    Foreach,
    EndForeach,
    Include,
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
    Directive::Include,
    Directive::Import,
    Directive::Props,
];

impl Directive {
    /// Returns the directive as it is written, like `@if`.
    pub(super) fn shown(self) -> &'static str {
        match self {
            Directive::If => "@if",
            Directive::ElseIf => "@elseif",
            Directive::Else => "@else",
            Directive::EndIf => "@endif",
            Directive::Foreach => "@foreach",
            Directive::EndForeach => "@endforeach",
            Directive::Include => "@include",
            Directive::Import => "@import",
            Directive::Props => "@props",
        }
    }

    /// Returns true for `@import` and `@props`, which may only stand at the top of a template.
    pub(super) fn is_header(self) -> bool {
        matches!(self, Directive::Import | Directive::Props)
    }

    /// Returns an example of the `( ... )` after the directive, for L012.
    fn example(self) -> &'static str {
        match self {
            Directive::If | Directive::ElseIf => "(x > 0)",
            Directive::Foreach => "(item: string in items)",
            Directive::Props => "(title: string)",
            Directive::Include => "(card(page))",
            Directive::Else | Directive::EndIf | Directive::EndForeach | Directive::Import => "( )",
        }
    }

    /// Returns the word after the `@`.
    fn word(self) -> &'static str {
        self.shown().trim_start_matches('@')
    }

    /// Returns the directive written as `@word`, if there is one.
    pub(super) fn from_word(word: &str) -> Option<Directive> {
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
            Directive::If
            | Directive::Foreach
            | Directive::Include
            | Directive::Import
            | Directive::Props => None,
        }
    }

    /// Returns the directive that closes this one, like `@endif` for `@if`.
    pub(super) fn closer(self) -> Directive {
        match self {
            Directive::Foreach | Directive::EndForeach => Directive::EndForeach,
            Directive::If | Directive::ElseIf | Directive::Else | Directive::EndIf => {
                Directive::EndIf
            }
            Directive::Include | Directive::Import | Directive::Props => self,
        }
    }
}

impl Lexer {
    /// Reads a directive after its name as the tokens it stands for, like `} else if (...) {` for `@elseif (...)`;
    /// `open` holds the directives that still wait for their end.
    pub(super) fn read_directive(
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
            Directive::Include => self.read_include(start, tokens)?,
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

    /// Reads `@include(card(page))` after its name as the statement `card(page);`, which calls another template;
    /// it must start like a call (L016), the parser checks the rest.
    fn read_include(
        &mut self,
        start: Span,
        tokens: &mut Vec<SpannedToken>,
    ) -> Result<(), SomoniError> {
        let mut parens = Vec::new();
        self.read_condition(Directive::Include, start, &mut parens)?;
        let inner = parens
            .get(1..parens.len().saturating_sub(1))
            .unwrap_or_default();
        if !starts_call(inner) {
            return Err(SomoniError::at(start, ErrorCode::IncludeNotCall));
        }
        tokens.extend_from_slice(inner);
        push(tokens, [Token::Semicolon], start);
        Ok(())
    }

    /// Reads the `( ... )` after `@if`, `@elseif`, `@foreach`, `@include` or `@props` up to its matching `)`, or returns L012.
    pub(super) fn read_condition(
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
            line: open_span.line as usize,
            close: open.closer().shown(),
        },
        None => ErrorCode::DirectiveWithoutOpen {
            found: directive.shown(),
            needs: needed.shown(),
        },
    };
    Err(SomoniError::at(start, code))
}

/// Returns true if `tokens` start like a call: a name, then `(` or `.` like in `card(page)` or `parts.card(page)`.
fn starts_call(tokens: &[SpannedToken]) -> bool {
    let kinds: Vec<&Token> = tokens.iter().take(2).map(|t| &t.token).collect();
    matches!(
        kinds.as_slice(),
        [Token::Ident(_), Token::LParen | Token::Dot]
    )
}
