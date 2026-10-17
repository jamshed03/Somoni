use std::rc::Rc;

use crate::ast::{
    BinaryOp, Expr, ExprKind, Field, FieldInit, Function, Param, Stmt, StructDecl, TypeName,
    UnaryOp,
};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::text::Text;
use crate::token::{SpannedToken, Token};

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

    /// Reads the whole program. `fn` and `struct` are only allowed here, at the top level.
    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, SomoniError> {
        let mut stmts = Vec::new();
        while *self.peek() != Token::Eof {
            let stmt = match self.peek() {
                Token::Fn => self.parse_fn()?,
                Token::Struct => self.parse_struct()?,
                _ => self.parse_stmt()?,
            };
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

    /// Reads one statement inside a block or at the top level.
    fn parse_stmt(&mut self) -> Result<Stmt, SomoniError> {
        let start = self.current_span();
        match self.peek().clone() {
            Token::Var => self.parse_var_decl(true, start),
            Token::Const => self.parse_var_decl(false, start),
            Token::LBrace => self.parse_block(start),
            Token::If => self.parse_if(start),
            Token::While => self.parse_while(start),
            Token::For => self.parse_for(start),
            Token::Break => {
                self.advance();
                self.expect(Token::Semicolon, "';' after 'break'")?;
                Ok(Stmt::Break { span: start })
            }
            Token::Continue => {
                self.advance();
                self.expect(Token::Semicolon, "';' after 'continue'")?;
                Ok(Stmt::Continue { span: start })
            }
            Token::Fn => Err(SomoniError::at(start, ErrorCode::FnNotTopLevel)),
            Token::Struct => Err(SomoniError::at(start, ErrorCode::StructNotTopLevel)),
            Token::Return => {
                self.advance();
                let value = if *self.peek() == Token::Semicolon {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                let span = self.since(start);
                self.expect(Token::Semicolon, "';'")?;
                Ok(Stmt::Return { value, span })
            }
            Token::Ident(name) if *self.peek_next() == Token::LParen => {
                let expr = self.parse_call(name, start)?;
                let span = self.since(start);
                self.expect(Token::Semicolon, "';'")?;
                Ok(Stmt::Expr { expr, span })
            }
            Token::Ident(_) => self.parse_assign(start),
            other => Err(SomoniError::at(
                start,
                ErrorCode::UnexpectedToken { found: other },
            )),
        }
    }

    /// Reads `fn name(a: int, b: int): int { ... }`.
    fn parse_fn(&mut self) -> Result<Stmt, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("function name")?;
        self.expect(Token::LParen, "'(' after function name")?;
        let params = self.parse_comma_list(Token::RParen, "')'", Self::parse_param)?;
        self.expect(Token::Colon, "':' and a return type after ')'")?;
        let return_type = self.parse_type("return type")?;
        let body = self.expect_block_stmts("after function header")?;
        Ok(Stmt::FnDecl(Function {
            name,
            name_span,
            params,
            return_type,
            body,
        }))
    }

    /// Reads one parameter `a: int`.
    fn parse_param(&mut self) -> Result<Param, SomoniError> {
        let (name, name_span) = self.expect_ident("parameter name")?;
        self.expect(Token::Colon, "':' after parameter name")?;
        let type_name = self.parse_type("parameter type")?;
        Ok(Param {
            name,
            name_span,
            type_name,
        })
    }

    /// Reads `struct Point { x: int, y: int }`.
    fn parse_struct(&mut self) -> Result<Stmt, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("struct name")?;
        self.expect(Token::LBrace, "'{' after struct name")?;
        let fields = self.parse_comma_list(Token::RBrace, "'}'", Self::parse_field)?;
        Ok(Stmt::StructDecl(Rc::new(StructDecl {
            name,
            name_span,
            fields,
        })))
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

    /// Reads a block `{ ... }`. `open` is the place of `{`, for the error if `}` is missing.
    fn parse_block(&mut self, open: Span) -> Result<Stmt, SomoniError> {
        Ok(Stmt::Block(self.parse_block_stmts(open)?))
    }

    /// Reads the statements of a block `{ ... }`.
    fn parse_block_stmts(&mut self, open: Span) -> Result<Vec<Stmt>, SomoniError> {
        self.advance();
        let mut stmts = Vec::new();
        while *self.peek() != Token::RBrace {
            if *self.peek() == Token::Eof {
                return Err(SomoniError::at(open, ErrorCode::BlockNotClosed));
            }
            stmts.push(self.parse_stmt()?);
        }
        self.advance();
        Ok(stmts)
    }

    /// Reads a block that must come here, e.g. after `if (...)`.
    fn expect_block(&mut self, what: &str) -> Result<Stmt, SomoniError> {
        Ok(Stmt::Block(self.expect_block_stmts(what)?))
    }

    /// Like `expect_block`, but returns the statements.
    fn expect_block_stmts(&mut self, what: &str) -> Result<Vec<Stmt>, SomoniError> {
        if *self.peek() != Token::LBrace {
            return Err(self.expected(format!("'{{' {}", what)));
        }
        let open = self.current_span();
        self.parse_block_stmts(open)
    }

    /// Reads `if (cond) { ... }` with any number of `else if` and an optional `else`.
    fn parse_if(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        self.advance();
        self.expect(Token::LParen, "'(' after 'if'")?;
        let condition = self.parse_expr()?;
        self.expect(Token::RParen, "')' after if condition")?;
        let then_branch = self.expect_block("after if condition")?;

        let else_branch = if *self.peek() == Token::Else {
            self.advance();
            let next = self.current_span();
            match self.peek() {
                Token::If => Some(Box::new(self.parse_if(next)?)),
                Token::LBrace => Some(Box::new(self.parse_block(next)?)),
                _ => {
                    return Err(self.expected("'{' or 'if' after 'else'"));
                }
            }
        } else {
            None
        };

        Ok(Stmt::If {
            condition,
            then_branch: Box::new(then_branch),
            else_branch,
            span: start,
        })
    }

    /// Reads `while (cond) { ... }`.
    fn parse_while(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        self.advance();
        self.expect(Token::LParen, "'(' after 'while'")?;
        let condition = self.parse_expr()?;
        self.expect(Token::RParen, "')' after while condition")?;
        let body = self.expect_block("after while condition")?;
        Ok(Stmt::While {
            condition,
            body: Box::new(body),
            span: start,
        })
    }

    /// Reads `for (var i: int = 0; i < 10; i = i + 1) { ... }`.
    fn parse_for(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        self.advance();
        self.expect(Token::LParen, "'(' after 'for'")?;

        let init = match self.peek() {
            Token::Var => self.parse_var_decl(true, self.current_span())?,
            Token::Const => {
                return Err(SomoniError::at(
                    self.current_span(),
                    ErrorCode::ForStartConst,
                ));
            }
            _ => {
                return Err(self.expected("'var' after 'for ('"));
            }
        };

        let condition = self.parse_expr()?;
        self.expect(Token::Semicolon, "';' after for condition")?;

        let step = match self.peek().clone() {
            Token::Ident(_) => self.parse_assignment(self.current_span())?,
            _ => {
                return Err(SomoniError::at(
                    self.current_span(),
                    ErrorCode::Expected {
                        what: "assignment as for step".to_string(),
                    },
                ));
            }
        };
        self.expect(Token::RParen, "')' after for step")?;

        let body = self.expect_block("after for header")?;
        Ok(Stmt::For {
            init: Box::new(init),
            condition,
            step: Box::new(step),
            body: Box::new(body),
            span: start,
        })
    }

    /// Reads an assignment with `;`, e.g. `x = 5;`.
    fn parse_assign(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        let assign = self.parse_assignment(start)?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(assign)
    }

    /// Reads an assignment without `;`, e.g. `x = 5`, `l[i] = 5` or `u.name = "Ali"`.
    fn parse_assignment(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        let target = self.parse_postfix()?;
        if target.root_variable().is_none() {
            return Err(SomoniError::at(target.span, ErrorCode::InvalidAssignTarget));
        }
        self.expect(Token::Equal, "'='")?;
        let value = self.parse_expr()?;
        Ok(Stmt::Assign {
            target,
            value,
            span: self.since(start),
        })
    }

    /// Reads `var name: type = value;` or `const name: type = value;`.
    fn parse_var_decl(&mut self, mutable: bool, start: Span) -> Result<Stmt, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("variable name")?;
        self.expect(Token::Colon, "':'")?;
        let type_name = self.parse_type("type name")?;
        self.expect(Token::Equal, "'='")?;
        let value = self.parse_expr()?;
        let span = self.since(start);
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::VarDecl {
            name,
            name_span,
            type_name,
            value,
            mutable,
            slot: None,
            span,
        })
    }

    /// Reads a call `name(a, b)`. The current token is the name.
    fn parse_call(&mut self, name: String, name_span: Span) -> Result<Expr, SomoniError> {
        self.advance();
        self.advance();
        let args = self.parse_comma_list(Token::RParen, "')'", Self::parse_expr)?;
        Ok(Expr {
            kind: ExprKind::Call {
                name,
                name_span,
                args,
                target: None,
            },
            span: self.since(name_span),
        })
    }

    /// Reads operands with operators of one level, e.g. `a + b - c`.
    /// `next` reads an operand, `ops` are the operators of this level.
    fn parse_binary(
        &mut self,
        next: fn(&mut Self) -> Result<Expr, SomoniError>,
        ops: &[(Token, BinaryOp)],
    ) -> Result<Expr, SomoniError> {
        let mut left = next(self)?;

        while let Some((_, op)) = ops.iter().find(|(token, _)| token == self.peek()) {
            let op_span = self.current_span();
            self.advance();
            let right = next(self)?;
            let span = left.span.to(right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op: *op,
                    op_span,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Reads an expression. Each `parse_*` below is one level of operator precedence.
    pub fn parse_expr(&mut self) -> Result<Expr, SomoniError> {
        self.parse_binary(Self::parse_and, &[(Token::OrOr, BinaryOp::Or)])
    }

    fn parse_and(&mut self) -> Result<Expr, SomoniError> {
        self.parse_binary(Self::parse_equality, &[(Token::AndAnd, BinaryOp::And)])
    }

    fn parse_equality(&mut self) -> Result<Expr, SomoniError> {
        self.parse_binary(
            Self::parse_comparison,
            &[
                (Token::EqualEqual, BinaryOp::Eq),
                (Token::BangEqual, BinaryOp::NotEq),
            ],
        )
    }

    fn parse_comparison(&mut self) -> Result<Expr, SomoniError> {
        self.parse_binary(
            Self::parse_additive,
            &[
                (Token::Less, BinaryOp::Less),
                (Token::Greater, BinaryOp::Greater),
                (Token::LessEqual, BinaryOp::LessEq),
                (Token::GreaterEqual, BinaryOp::GreaterEq),
            ],
        )
    }

    fn parse_additive(&mut self) -> Result<Expr, SomoniError> {
        self.parse_binary(
            Self::parse_term,
            &[(Token::Plus, BinaryOp::Add), (Token::Minus, BinaryOp::Sub)],
        )
    }

    fn parse_term(&mut self) -> Result<Expr, SomoniError> {
        self.parse_binary(
            Self::parse_unary,
            &[
                (Token::Star, BinaryOp::Mul),
                (Token::Slash, BinaryOp::Div),
                (Token::Percent, BinaryOp::Mod),
            ],
        )
    }

    /// Reads `!x`, `-x` or an expression without such an operator.
    fn parse_unary(&mut self) -> Result<Expr, SomoniError> {
        let op = match self.peek() {
            Token::Bang => UnaryOp::Not,
            Token::Minus => UnaryOp::Neg,
            _ => return self.parse_postfix(),
        };
        let start = self.current_span();
        self.advance();
        let operand = self.parse_unary()?;
        let span = start.to(operand.span);
        Ok(Expr {
            kind: ExprKind::Unary {
                op,
                operand: Box::new(operand),
            },
            span,
        })
    }

    /// Reads an expression followed by any number of `[index]` and `.field`, e.g. `l[0].x`.
    fn parse_postfix(&mut self) -> Result<Expr, SomoniError> {
        let mut expr = self.parse_primary()?;
        loop {
            let start = expr.span;
            let kind = match self.peek() {
                Token::LBracket => {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(Token::RBracket, "']'")?;
                    ExprKind::Index {
                        target: Box::new(expr),
                        index: Box::new(index),
                    }
                }
                Token::Dot => {
                    self.advance();
                    let (name, name_span) = self.expect_ident("field name after '.'")?;
                    ExprKind::Field {
                        target: Box::new(expr),
                        name,
                        name_span,
                        index: None,
                    }
                }
                _ => return Ok(expr),
            };
            expr = Expr {
                kind,
                span: start.to(self.last_span()),
            };
        }
    }

    /// Reads a struct literal `Point { x: 1, y: 2 }`. The current token is the name.
    fn parse_struct_literal(&mut self, name: String, name_span: Span) -> Result<Expr, SomoniError> {
        self.advance();
        self.advance();
        let fields = self.parse_comma_list(Token::RBrace, "'}'", Self::parse_field_init)?;
        Ok(Expr {
            kind: ExprKind::StructLiteral {
                name,
                name_span,
                fields,
                index: None,
            },
            span: self.since(name_span),
        })
    }

    /// Reads one `x: 1` in a struct literal.
    fn parse_field_init(&mut self) -> Result<FieldInit, SomoniError> {
        let (name, name_span) = self.expect_ident("field name")?;
        self.expect(Token::Colon, "':' after field name")?;
        let value = self.parse_expr()?;
        Ok(FieldInit {
            name,
            name_span,
            value,
            index: None,
        })
    }

    /// Reads a type like `int`, `list<int>` or `list<list<string>>`.
    fn parse_type(&mut self, what: &str) -> Result<TypeName, SomoniError> {
        let (name, start) = self.expect_ident(what)?;
        let mut args = Vec::new();
        if *self.peek() == Token::Less {
            self.advance();
            args.push(self.parse_type("type")?);
            self.expect(Token::Greater, "'>'")?;
        }
        Ok(TypeName {
            name,
            args,
            span: self.since(start),
        })
    }

    /// Reads a value, a name, a call, a struct literal, a list literal or `( ... )`.
    fn parse_primary(&mut self) -> Result<Expr, SomoniError> {
        let span = self.current_span();
        let kind = match self.peek().clone() {
            Token::Int(n) => ExprKind::Int(n),
            Token::Float(f) => ExprKind::Float(f),
            Token::Str(s) => ExprKind::Str(Rc::new(Text::new(s))),
            Token::True => ExprKind::Bool(true),
            Token::False => ExprKind::Bool(false),
            Token::Ident(name) if *self.peek_next() == Token::LParen => {
                return self.parse_call(name, span);
            }
            Token::Ident(name) if *self.peek_next() == Token::LBrace => {
                return self.parse_struct_literal(name, span);
            }
            Token::Ident(name) => ExprKind::Ident { name, slot: None },
            Token::LBracket => {
                self.advance();
                let elements = self.parse_comma_list(Token::RBracket, "']'", Self::parse_expr)?;
                return Ok(Expr {
                    kind: ExprKind::List(elements),
                    span: span.to(self.last_span()),
                });
            }
            Token::LParen => {
                self.advance();
                let inner = self.parse_expr()?;
                if *self.peek() != Token::RParen {
                    return Err(self.expected("')'"));
                }
                self.advance();
                return Ok(Expr {
                    kind: inner.kind,
                    span: span.to(self.last_span()),
                });
            }
            other => {
                return Err(SomoniError::at(
                    span,
                    ErrorCode::UnexpectedToken { found: other },
                ));
            }
        };
        self.advance();
        Ok(Expr { kind, span })
    }
}
