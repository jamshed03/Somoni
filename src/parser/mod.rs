//! The parser: turns the tokens of the lexer into a tree of statements and expressions.
//! Its parts are spread over the files of this folder; here are the basics they share.

use crate::ast::{Field, Param, Stmt, TypeName};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::{SpannedToken, Token};

mod declarations;
mod expressions;
mod statements;

/// Turns the tokens of the lexer into a tree of statements and expressions.
pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Parser {
        Parser { tokens, pos: 0 }
    }

    /// Returns the current token.
    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    /// Returns the token after the current one.
    fn peek_next(&self) -> &Token {
        let index = (self.pos + 1).min(self.tokens.len() - 1);
        &self.tokens[index].token
    }

    /// Returns the place of the current token.
    fn current_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    /// Returns the place of the token that was read last.
    fn last_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1)].span
    }

    /// Returns the place from `start` to the token that was read last.
    fn since(&self, start: Span) -> Span {
        start.to(self.last_span())
    }

    /// The error "expected ...", placed right after the token that was read last.
    fn expected(&self, what: impl Into<String>) -> SomoniError {
        SomoniError::at(
            self.last_span().after(),
            ErrorCode::Expected { what: what.into() },
        )
    }

    /// Goes to the next token. It stays at the end of the input (Eof).
    fn advance(&mut self) {
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
    }

    /// Reads a whole file. `fn`, `struct`, `enum` and `export` are only allowed here,
    /// at the top level, and `import` only before all other statements.
    /// An `imported` file may contain nothing else (M003).
    pub fn parse_program(&mut self, imported: bool) -> Result<Vec<Stmt>, SomoniError> {
        let mut stmts = Vec::new();
        let mut only_imports = true;
        while *self.peek() != Token::Eof {
            let is_declaration = matches!(
                self.peek(),
                Token::Import
                    | Token::Export
                    | Token::Fn
                    | Token::Struct
                    | Token::Enum
                    | Token::Class
                    | Token::Interface
            );
            if imported && !is_declaration {
                return Err(SomoniError::at(
                    self.current_span(),
                    ErrorCode::OnlyDeclarations,
                ));
            }
            let stmt = match self.peek() {
                Token::Import if only_imports => self.parse_import()?,
                Token::Export => self.parse_export()?,
                Token::Fn => self.parse_fn(false)?,
                Token::Struct => self.parse_struct(false)?,
                Token::Enum => self.parse_enum(false)?,
                Token::Class => self.parse_class(false)?,
                Token::Interface => self.parse_interface(false)?,
                _ => self.parse_stmt()?,
            };
            only_imports &= matches!(stmt, Stmt::Import(_));
            stmts.push(stmt);
        }
        Ok(stmts)
    }

    /// Reads the token `expected`, or returns the error "expected `what`".
    fn expect(&mut self, expected: Token, what: &str) -> Result<(), SomoniError> {
        if *self.peek() == expected {
            self.advance();
            Ok(())
        } else {
            Err(self.expected(what))
        }
    }

    /// Reads a name that is a keyword only at this place, like `from` in an import.
    fn expect_word(&mut self, word: &str, what: &str) -> Result<(), SomoniError> {
        match self.peek() {
            Token::Ident(name) if name == word => {
                self.advance();
                Ok(())
            }
            _ => Err(self.expected(what)),
        }
    }

    /// Reads a name, or returns the error "expected `what`".
    fn expect_ident(&mut self, what: &str) -> Result<(String, Span), SomoniError> {
        let span = self.current_span();
        match self.peek().clone() {
            Token::Ident(name) => {
                self.advance();
                Ok((name, span))
            }
            _ => Err(self.expected(what)),
        }
    }

    /// Reads `a: int`, a parameter or the variable of a `for`-each loop.
    /// `what` names it in error messages.
    fn parse_param(&mut self, what: &str) -> Result<Param, SomoniError> {
        let (name, name_span) = self.expect_ident(&format!("{} name", what))?;
        self.expect(Token::Colon, &format!("':' after {} name", what))?;
        let type_name = self.parse_type(&format!("{} type", what))?;
        Ok(Param {
            name,
            name_span,
            type_name,
        })
    }

    /// Reads one field `x: int` of a struct declaration.
    fn parse_field(&mut self) -> Result<Field, SomoniError> {
        let (name, name_span) = self.expect_ident("field name")?;
        self.expect(Token::Colon, "':' after field name")?;
        let type_name = self.parse_type("field type")?;
        Ok(Field {
            name,
            name_span,
            type_name,
        })
    }

    /// Reads items separated by `,` up to the token `close`, e.g. the arguments `a, b)`.
    /// The opening bracket is already read.
    fn parse_comma_list<T>(
        &mut self,
        close: Token,
        close_text: &str,
        mut parse_item: impl FnMut(&mut Self) -> Result<T, SomoniError>,
    ) -> Result<Vec<T>, SomoniError> {
        let mut items = Vec::new();
        if *self.peek() == close {
            self.advance();
            return Ok(items);
        }
        loop {
            items.push(parse_item(self)?);
            if *self.peek() != Token::Comma {
                break;
            }
            self.advance();
        }
        self.expect(close, close_text)?;
        Ok(items)
    }

    /// Reads a type like `int`, `int?`, `list<int>` or `map<string, list<int>>`.
    fn parse_type(&mut self, what: &str) -> Result<TypeName, SomoniError> {
        let (name, start) = self.expect_ident(what)?;
        let mut args = Vec::new();
        if *self.peek() == Token::Less {
            self.advance();
            args = self.parse_comma_list(Token::Greater, "'>'", |this| this.parse_type("type"))?;
        }
        let optional = *self.peek() == Token::Question;
        if optional {
            self.advance();
        }
        Ok(TypeName {
            name,
            args,
            optional,
            span: self.since(start),
        })
    }
}
