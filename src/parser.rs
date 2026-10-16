use crate::ast::{BinaryOp, Expr, ExprKind, Function, Param, Stmt, UnaryOp};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::{SpannedToken, Token};

pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Parser {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    fn peek_next(&self) -> &Token {
        let index = (self.pos + 1).min(self.tokens.len() - 1);
        &self.tokens[index].token
    }

    fn current_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn last_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1)].span
    }

    fn since(&self, start: Span) -> Span {
        start.to(self.last_span())
    }

    fn expected(&self, what: impl Into<String>) -> SomoniError {
        SomoniError::at(
            self.last_span().after(),
            ErrorCode::Expected { what: what.into() },
        )
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
    }

    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, SomoniError> {
        let mut stmts = Vec::new();
        while *self.peek() != Token::Eof {
            // fn is only allowed here, at the top level
            if *self.peek() == Token::Fn {
                stmts.push(self.parse_fn()?);
            } else {
                stmts.push(self.parse_stmt()?);
            }
        }
        Ok(stmts)
    }

    fn expect(&mut self, expected: Token, what: &str) -> Result<(), SomoniError> {
        if *self.peek() == expected {
            self.advance();
            Ok(())
        } else {
            Err(self.expected(what))
        }
    }

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
                self.advance(); // 'break'
                self.expect(Token::Semicolon, "';' after 'break'")?;
                Ok(Stmt::Break { span: start })
            }
            Token::Continue => {
                self.advance(); // 'continue'
                self.expect(Token::Semicolon, "';' after 'continue'")?;
                Ok(Stmt::Continue { span: start })
            }
            Token::Fn => Err(SomoniError::at(start, ErrorCode::FnNotTopLevel)),
            Token::Return => {
                self.advance(); // 'return'
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
            Token::Ident(name) => self.parse_assign(name, start),
            other => Err(SomoniError::at(
                start,
                ErrorCode::UnexpectedToken { found: other },
            )),
        }
    }

    // fn name(a: int, b: int): int { ... }
    fn parse_fn(&mut self) -> Result<Stmt, SomoniError> {
        self.advance(); // 'fn'
        let (name, name_span) = self.expect_ident("function name")?;
        self.expect(Token::LParen, "'(' after function name")?;
        let mut params = Vec::new();
        if *self.peek() == Token::RParen {
            self.advance();
        } else {
            loop {
                let (name, name_span) = self.expect_ident("parameter name")?;
                self.expect(Token::Colon, "':' after parameter name")?;
                let (type_name, type_span) = self.expect_ident("parameter type")?;
                params.push(Param {
                    name,
                    name_span,
                    type_name,
                    type_span,
                });
                if *self.peek() == Token::Comma {
                    self.advance();
                    continue;
                }
                self.expect(Token::RParen, "')'")?;
                break;
            }
        }
        self.expect(Token::Colon, "':' and a return type after ')'")?;
        let (return_type, return_type_span) = self.expect_ident("return type")?;
        let body = self.expect_block_stmts("after function header")?;
        Ok(Stmt::FnDecl(Function {
            name,
            name_span,
            params,
            return_type,
            return_type_span,
            body,
        }))
    }

    // { stmt* } – no ';' after the closing brace
    // `open` is the '{', an unclosed block points there
    fn parse_block(&mut self, open: Span) -> Result<Stmt, SomoniError> {
        Ok(Stmt::Block(self.parse_block_stmts(open)?))
    }

    fn parse_block_stmts(&mut self, open: Span) -> Result<Vec<Stmt>, SomoniError> {
        self.advance(); // '{'
        let mut stmts = Vec::new();
        while *self.peek() != Token::RBrace {
            if *self.peek() == Token::Eof {
                return Err(SomoniError::at(open, ErrorCode::BlockNotClosed));
            }
            stmts.push(self.parse_stmt()?);
        }
        self.advance(); // '}'
        Ok(stmts)
    }

    // a block where the language requires one, e.g. after `if (...)`
    fn expect_block(&mut self, what: &str) -> Result<Stmt, SomoniError> {
        Ok(Stmt::Block(self.expect_block_stmts(what)?))
    }

    fn expect_block_stmts(&mut self, what: &str) -> Result<Vec<Stmt>, SomoniError> {
        if *self.peek() != Token::LBrace {
            return Err(self.expected(format!("'{{' {}", what)));
        }
        let open = self.current_span();
        self.parse_block_stmts(open)
    }

    // if (cond) { ... } [else if (cond) { ... }]* [else { ... }]
    fn parse_if(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        self.advance(); // 'if'
        self.expect(Token::LParen, "'(' after 'if'")?;
        let condition = self.parse_expr()?;
        self.expect(Token::RParen, "')' after if condition")?;
        let then_branch = self.expect_block("after if condition")?;

        let else_branch = if *self.peek() == Token::Else {
            self.advance(); // 'else'
            let next = self.current_span();
            match self.peek() {
                // `else if` is an else whose only content is another if
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

    // while (cond) { ... }
    fn parse_while(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        self.advance(); // 'while'
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

    // for (var i: int = 0; i < 10; i = i + 1) { ... }
    fn parse_for(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        self.advance(); // 'for'
        self.expect(Token::LParen, "'(' after 'for'")?;

        // start: always a new var, parse_var_decl also reads the ';'
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

        // step: an assignment without ';'
        let step = match self.peek().clone() {
            Token::Ident(name) => self.parse_assignment(name, self.current_span())?,
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

    // x = value;
    fn parse_assign(&mut self, name: String, start: Span) -> Result<Stmt, SomoniError> {
        let assign = self.parse_assignment(name, start)?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(assign)
    }

    // x = value (without ';', also used as the step of a for loop)
    // `start` is the name
    fn parse_assignment(&mut self, name: String, start: Span) -> Result<Stmt, SomoniError> {
        self.advance(); // name
        self.expect(Token::Equal, "'='")?;
        let value = self.parse_expr()?;
        Ok(Stmt::Assign {
            name,
            name_span: start,
            value,
            span: self.since(start),
        })
    }

    fn parse_var_decl(&mut self, mutable: bool, start: Span) -> Result<Stmt, SomoniError> {
        self.advance(); // var or const
        let (name, name_span) = self.expect_ident("variable name")?;
        self.expect(Token::Colon, "':'")?;
        let (type_name, type_span) = self.expect_ident("type name")?;
        self.expect(Token::Equal, "'='")?;
        let value = self.parse_expr()?;
        let span = self.since(start);
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::VarDecl {
            name,
            name_span,
            type_name,
            type_span,
            value,
            mutable,
            span,
        })
    }

    fn parse_call(&mut self, name: String, name_span: Span) -> Result<Expr, SomoniError> {
        self.advance(); // name
        self.advance(); // '('
        let mut args = Vec::new();
        if *self.peek() == Token::RParen {
            self.advance();
        } else {
            loop {
                args.push(self.parse_expr()?);
                if *self.peek() == Token::Comma {
                    self.advance();
                    continue;
                }
                self.expect(Token::RParen, "')'")?;
                break;
            }
        }
        Ok(Expr {
            kind: ExprKind::Call {
                name,
                name_span,
                args,
            },
            span: self.since(name_span),
        })
    }

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
                    op: op.clone(),
                    op_span,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

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

    fn parse_unary(&mut self) -> Result<Expr, SomoniError> {
        let op = match self.peek() {
            Token::Bang => UnaryOp::Not,
            Token::Minus => UnaryOp::Neg,
            _ => return self.parse_primary(),
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

    fn parse_primary(&mut self) -> Result<Expr, SomoniError> {
        let span = self.current_span();
        let kind = match self.peek().clone() {
            Token::Int(n) => ExprKind::Int(n),
            Token::Float(f) => ExprKind::Float(f),
            Token::Str(s) => ExprKind::Str(s),
            Token::True => ExprKind::Bool(true),
            Token::False => ExprKind::Bool(false),
            Token::Ident(name) if *self.peek_next() == Token::LParen => {
                return self.parse_call(name, span);
            }
            Token::Ident(name) => ExprKind::Ident(name),
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
