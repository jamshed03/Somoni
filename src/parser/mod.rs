//! The parser, which turns the tokens of the lexer into a tree; its parts are spread over this folder.

use crate::ast::{Expr, ExprKind, Field, Param, Stmt, TypeName};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::{SpannedToken, Token};

mod classes;
mod declarations;
mod expressions;
mod statements;

/// Turns the tokens of the lexer into a tree of statements and expressions.
pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}

impl Parser {
    /// Starts at the first token.
    pub fn new(tokens: Vec<SpannedToken>) -> Parser {
        Parser { tokens, pos: 0 }
    }

    /// Reads a whole file, which may hold only declarations if it is `imported` (M003).
    pub fn parse_program(&mut self, imported: bool) -> Result<Vec<Stmt>, SomoniError> {
        let mut stmts = Vec::new();
        let mut only_imports = true;
        while *self.peek() != Token::Eof {
            if imported && !starts_declaration(self.peek()) {
                return Err(SomoniError::at(
                    self.current_span(),
                    ErrorCode::OnlyDeclarations,
                ));
            }
            let stmt = self.parse_top_level(only_imports)?;
            only_imports &= matches!(stmt, Stmt::Import(_));
            stmts.push(stmt);
        }
        Ok(stmts)
    }

    /// Reads one statement at the top level, where declarations are allowed and `import` only before all others.
    fn parse_top_level(&mut self, only_imports: bool) -> Result<Stmt, SomoniError> {
        match self.peek() {
            Token::Import if only_imports => self.parse_import(),
            Token::Export => self.parse_export(),
            Token::Fn => self.parse_fn(false),
            Token::Struct => self.parse_struct(false),
            Token::Enum => self.parse_enum(false),
            Token::Class => self.parse_class(false),
            Token::Interface => self.parse_interface(false),
            _ => self.parse_stmt(),
        }
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

    /// Returns an expression of `kind` from `start` to the token that was read last.
    fn expr_since(&self, kind: ExprKind, start: Span) -> Expr {
        Expr {
            kind,
            span: self.since(start),
        }
    }

    /// The error "expected ...", placed right after the token that was read last.
    fn expected(&self, what: impl Into<String>) -> SomoniError {
        SomoniError::at(
            self.last_span().after(),
            ErrorCode::Expected { what: what.into() },
        )
    }

    /// Goes to the next token, but stays at the end of the input (Eof).
    fn advance(&mut self) {
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
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
        if !self.at_word(word) {
            return Err(self.expected(what));
        }
        self.advance();
        Ok(())
    }

    /// Returns true if the next token is the name `word`, like `constructor` in a class.
    fn at_word(&self, word: &str) -> bool {
        matches!(self.peek(), Token::Ident(name) if name == word)
    }

    /// Reads `token` if it comes next and returns whether it did.
    fn eat(&mut self, token: Token) -> bool {
        let found = *self.peek() == token;
        if found {
            self.advance();
        }
        found
    }

    /// Reads names separated by `,`, like `A, B` after `implements`.
    fn parse_names(&mut self, what: &str) -> Result<Vec<(String, Span)>, SomoniError> {
        let mut names = vec![self.expect_ident(what)?];
        while self.eat(Token::Comma) {
            names.push(self.expect_ident(what)?);
        }
        Ok(names)
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

    /// Reads `a: int`, where `what` names it in error messages, e.g. `parameter`.
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

    /// Reads the parameters of a function, method or constructor up to `)`, after the `(`.
    fn parse_params(&mut self) -> Result<Vec<Param>, SomoniError> {
        self.parse_comma_list(Token::RParen, "')'", |this| this.parse_param("parameter"))
    }

    /// Reads `: type` after the parameters of a function or method.
    fn parse_return_type(&mut self) -> Result<TypeName, SomoniError> {
        self.expect(Token::Colon, "':' and a return type after ')'")?;
        self.parse_type("return type")
    }

    /// Reads one field `x: int` of a struct or class.
    /// It stays apart from `parse_param` because fields and parameters change for different reasons.
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

    /// Reads items separated by `,` up to `close`, after the opening bracket, e.g. the arguments `a, b)`.
    fn parse_comma_list<T>(
        &mut self,
        close: Token,
        close_text: &str,
        mut parse_item: impl FnMut(&mut Self) -> Result<T, SomoniError>,
    ) -> Result<Vec<T>, SomoniError> {
        let mut items = Vec::new();
        if self.eat(close.clone()) {
            return Ok(items);
        }
        loop {
            items.push(parse_item(self)?);
            if !self.eat(Token::Comma) {
                break;
            }
        }
        self.expect(close, close_text)?;
        Ok(items)
    }

    /// Reads items up to the `}` of the block opened at `open`, or returns P003 if it is not closed.
    fn parse_until_brace<T>(
        &mut self,
        open: Span,
        mut parse_item: impl FnMut(&mut Self) -> Result<T, SomoniError>,
    ) -> Result<Vec<T>, SomoniError> {
        let mut items = Vec::new();
        while !self.eat(Token::RBrace) {
            if *self.peek() == Token::Eof {
                return Err(SomoniError::at(open, ErrorCode::BlockNotClosed));
            }
            items.push(parse_item(self)?);
        }
        Ok(items)
    }

    /// Reads a type like `int`, `int?`, `list<int>` or `map<string, list<int>>`.
    fn parse_type(&mut self, what: &str) -> Result<TypeName, SomoniError> {
        let (name, start) = self.expect_ident(what)?;
        let mut args = Vec::new();
        if self.eat(Token::Less) {
            args = self.parse_comma_list(Token::Greater, "'>'", |this| this.parse_type("type"))?;
        }
        let optional = self.eat(Token::Question);
        Ok(TypeName {
            name,
            args,
            optional,
            span: self.since(start),
        })
    }
}

/// Returns true for the tokens that start a declaration, the only statements an imported file may have.
fn starts_declaration(token: &Token) -> bool {
    matches!(
        token,
        Token::Import
            | Token::Export
            | Token::Fn
            | Token::Struct
            | Token::Enum
            | Token::Class
            | Token::Interface
    )
}
