//! Reading expressions; each `parse_*` level is one level of operator precedence.

use std::rc::Rc;

use crate::ast::{
    BinaryOp, Call, Expr, ExprKind, FieldInit, MethodCall, New, SuperCall, THIS, UnaryOp,
};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::text::Text;
use crate::token::Token;

use super::Parser;

impl Parser {
    /// Reads a call `name(a, b)`. The current token is the name.
    pub(super) fn parse_call(
        &mut self,
        name: String,
        name_span: Span,
    ) -> Result<Expr, SomoniError> {
        self.advance();
        self.advance();
        let args = self.parse_args()?;
        Ok(Expr {
            kind: ExprKind::Call(Call {
                name,
                name_span,
                args,
                target: None,
            }),
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
    pub(super) fn parse_postfix(&mut self) -> Result<Expr, SomoniError> {
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
                    if self.eat(Token::LParen) {
                        let args = self.parse_args()?;
                        ExprKind::MethodCall(MethodCall {
                            target: Box::new(expr),
                            name,
                            name_span,
                            args,
                            call: None,
                        })
                    } else {
                        ExprKind::Field {
                            target: Box::new(expr),
                            name,
                            name_span,
                            index: None,
                        }
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

    /// Reads one `key: value` in a map literal.
    fn parse_map_entry(&mut self) -> Result<(Expr, Expr), SomoniError> {
        let key = self.parse_expr()?;
        self.expect(Token::Colon, "':' after map key")?;
        let value = self.parse_expr()?;
        Ok((key, value))
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

    /// Reads the arguments of a call up to `)`; the `(` is already read.
    fn parse_args(&mut self) -> Result<Vec<Expr>, SomoniError> {
        self.parse_comma_list(Token::RParen, "')'", Self::parse_expr)
    }

    /// Reads `new Name(args)`.
    fn parse_new(&mut self, span: Span) -> Result<Expr, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("class name after 'new'")?;
        self.expect(Token::LParen, "'(' after the class name")?;
        let args = self.parse_args()?;
        let kind = ExprKind::New(New {
            name,
            name_span,
            args,
            class: None,
            target: None,
        });
        Ok(Expr {
            kind,
            span: span.to(self.last_span()),
        })
    }

    /// Reads `super(args)` or `super.name(args)`.
    fn parse_super(&mut self, span: Span) -> Result<Expr, SomoniError> {
        self.advance();
        let kind = if self.eat(Token::LParen) {
            let args = self.parse_args()?;
            ExprKind::SuperInit { args, target: None }
        } else {
            self.expect(Token::Dot, "'(' or '.' after 'super'")?;
            let (name, name_span) = self.expect_ident("method name after 'super.'")?;
            self.expect(Token::LParen, "'(' after the method name")?;
            let args = self.parse_args()?;
            ExprKind::SuperCall(SuperCall {
                name,
                name_span,
                args,
                target: None,
            })
        };
        Ok(Expr {
            kind,
            span: span.to(self.last_span()),
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
            Token::Null => ExprKind::Null,
            Token::New => return self.parse_new(span),
            Token::Ident(name) if *self.peek_next() == Token::LParen => {
                return self.parse_call(name, span);
            }
            Token::Ident(name) if *self.peek_next() == Token::LBrace => {
                return self.parse_struct_literal(name, span);
            }
            Token::Ident(name) => ExprKind::Ident { name, slot: None },
            Token::Super => return self.parse_super(span),
            Token::This => ExprKind::Ident {
                name: THIS.to_string(),
                slot: None,
            },
            Token::LBracket => {
                self.advance();
                let elements = self.parse_comma_list(Token::RBracket, "']'", Self::parse_expr)?;
                return Ok(Expr {
                    kind: ExprKind::List(elements),
                    span: span.to(self.last_span()),
                });
            }
            Token::LBrace => {
                self.advance();
                let entries = self.parse_comma_list(Token::RBrace, "'}'", Self::parse_map_entry)?;
                return Ok(Expr {
                    kind: ExprKind::Map(entries),
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
