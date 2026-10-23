//! Reading expressions, from the weakest operator `||` down to single values.

use std::rc::Rc;

use crate::ast::{
    BinaryOp, Call, Expr, ExprKind, FieldInit, MethodCall, New, SuperCall, THIS, UnaryOp,
};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::text::Text;
use crate::token::Token;

use super::Parser;

/// The binary operators by precedence level, from the weakest (`||`) to the strongest (`*`).
const PRECEDENCE: &[&[(Token, BinaryOp)]] = &[
    &[(Token::OrOr, BinaryOp::Or)],
    &[(Token::AndAnd, BinaryOp::And)],
    &[
        (Token::EqualEqual, BinaryOp::Eq),
        (Token::BangEqual, BinaryOp::NotEq),
    ],
    &[
        (Token::Less, BinaryOp::Less),
        (Token::Greater, BinaryOp::Greater),
        (Token::LessEqual, BinaryOp::LessEq),
        (Token::GreaterEqual, BinaryOp::GreaterEq),
    ],
    &[(Token::Plus, BinaryOp::Add), (Token::Minus, BinaryOp::Sub)],
    &[
        (Token::Star, BinaryOp::Mul),
        (Token::Slash, BinaryOp::Div),
        (Token::Percent, BinaryOp::Mod),
    ],
];

impl Parser {
    /// Reads an expression.
    pub fn parse_expr(&mut self) -> Result<Expr, SomoniError> {
        self.parse_level(0)
    }

    /// Reads operands of precedence level `level` joined by its operators, e.g. `a + b - c`.
    fn parse_level(&mut self, level: usize) -> Result<Expr, SomoniError> {
        let Some(ops) = PRECEDENCE.get(level) else {
            return self.parse_unary();
        };
        let mut left = self.parse_level(level + 1)?;
        while let Some((_, op)) = ops.iter().find(|(token, _)| token == self.peek()) {
            let op_span = self.current_span();
            self.advance();
            let right = self.parse_level(level + 1)?;
            let span = left.span.to(right.span);
            let kind = ExprKind::Binary {
                left: Box::new(left),
                op: *op,
                op_span,
                right: Box::new(right),
            };
            left = Expr { kind, span };
        }
        Ok(left)
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

    /// Reads an expression followed by any number of `[index]`, `.field` and `.method(args)`, e.g. `l[0].x`.
    pub(super) fn parse_postfix(&mut self) -> Result<Expr, SomoniError> {
        let mut expr = self.parse_primary()?;
        loop {
            let start = expr.span;
            let kind = match self.peek() {
                Token::LBracket => self.parse_index(expr)?,
                Token::Dot => self.parse_member(expr)?,
                _ => return Ok(expr),
            };
            expr = self.expr_since(kind, start);
        }
    }

    /// Reads `[index]` after `target`.
    fn parse_index(&mut self, target: Expr) -> Result<ExprKind, SomoniError> {
        self.advance();
        let index = self.parse_expr()?;
        self.expect(Token::RBracket, "']'")?;
        Ok(ExprKind::Index {
            target: Box::new(target),
            index: Box::new(index),
        })
    }

    /// Reads `.field` or `.method(args)` after `target`.
    fn parse_member(&mut self, target: Expr) -> Result<ExprKind, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("field name after '.'")?;
        if !self.eat(Token::LParen) {
            return Ok(ExprKind::Field {
                target: Box::new(target),
                name,
                name_span,
                index: None,
            });
        }
        let args = self.parse_args()?;
        Ok(ExprKind::MethodCall(MethodCall {
            target: Box::new(target),
            name,
            name_span,
            args,
            call: None,
        }))
    }

    /// Reads a value, a name, a call, a literal, `new`, `super` or `( ... )`.
    fn parse_primary(&mut self) -> Result<Expr, SomoniError> {
        let span = self.current_span();
        let kind = match self.peek().clone() {
            Token::Int(n) => ExprKind::Int(n),
            Token::Float(f) => ExprKind::Float(f),
            Token::Str(s) => ExprKind::Str(Rc::new(Text::new(s))),
            Token::True => ExprKind::Bool(true),
            Token::False => ExprKind::Bool(false),
            Token::Null => ExprKind::Null,
            Token::Ident(name) if *self.peek_next() == Token::LParen => {
                return self.parse_call(name, span);
            }
            Token::Ident(name) if *self.peek_next() == Token::LBrace => {
                return self.parse_struct_literal(name, span);
            }
            Token::Ident(name) => ExprKind::Ident { name, slot: None },
            Token::This => ExprKind::Ident {
                name: THIS.to_string(),
                slot: None,
            },
            Token::New => return self.parse_new(span),
            Token::Super => return self.parse_super(span),
            Token::LBracket => return self.parse_list(span),
            Token::LBrace => return self.parse_map(span),
            Token::LParen => return self.parse_parens(span),
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

    /// Reads a call `name(a, b)` whose name is the current token.
    pub(super) fn parse_call(
        &mut self,
        name: String,
        name_span: Span,
    ) -> Result<Expr, SomoniError> {
        self.advance();
        self.advance();
        let args = self.parse_args()?;
        let call = Call {
            name,
            name_span,
            args,
            target: None,
        };
        Ok(self.expr_since(ExprKind::Call(call), name_span))
    }

    /// Reads the arguments of a call up to `)`, after the `(`.
    fn parse_args(&mut self) -> Result<Vec<Expr>, SomoniError> {
        self.parse_comma_list(Token::RParen, "')'", Self::parse_expr)
    }

    /// Reads a struct literal `Point { x: 1, y: 2 }` whose name is the current token.
    fn parse_struct_literal(&mut self, name: String, name_span: Span) -> Result<Expr, SomoniError> {
        self.advance();
        self.advance();
        let fields = self.parse_comma_list(Token::RBrace, "'}'", Self::parse_field_init)?;
        let kind = ExprKind::StructLiteral {
            name,
            name_span,
            fields,
            index: None,
        };
        Ok(self.expr_since(kind, name_span))
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

    /// Reads a list literal `[a, b, c]`.
    fn parse_list(&mut self, start: Span) -> Result<Expr, SomoniError> {
        self.advance();
        let elements = self.parse_comma_list(Token::RBracket, "']'", Self::parse_expr)?;
        Ok(self.expr_since(ExprKind::List(elements), start))
    }

    /// Reads a map literal `{key: value, ...}`.
    fn parse_map(&mut self, start: Span) -> Result<Expr, SomoniError> {
        self.advance();
        let entries = self.parse_comma_list(Token::RBrace, "'}'", Self::parse_map_entry)?;
        Ok(self.expr_since(ExprKind::Map(entries), start))
    }

    /// Reads one `key: value` in a map literal.
    fn parse_map_entry(&mut self) -> Result<(Expr, Expr), SomoniError> {
        let key = self.parse_expr()?;
        self.expect(Token::Colon, "':' after map key")?;
        let value = self.parse_expr()?;
        Ok((key, value))
    }

    /// Reads `( expr )`, whose place then includes the brackets.
    fn parse_parens(&mut self, start: Span) -> Result<Expr, SomoniError> {
        self.advance();
        let inner = self.parse_expr()?;
        self.expect(Token::RParen, "')'")?;
        Ok(self.expr_since(inner.kind, start))
    }

    /// Reads `new Name(args)`.
    fn parse_new(&mut self, start: Span) -> Result<Expr, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("class name after 'new'")?;
        self.expect(Token::LParen, "'(' after the class name")?;
        let args = self.parse_args()?;
        let new = New {
            name,
            name_span,
            args,
            class: None,
            target: None,
        };
        Ok(self.expr_since(ExprKind::New(new), start))
    }

    /// Reads `super(args)` or `super.name(args)`.
    fn parse_super(&mut self, start: Span) -> Result<Expr, SomoniError> {
        self.advance();
        let kind = if self.eat(Token::LParen) {
            let args = self.parse_args()?;
            ExprKind::SuperInit { args, target: None }
        } else {
            ExprKind::SuperCall(self.parse_super_call()?)
        };
        Ok(self.expr_since(kind, start))
    }

    /// Reads the rest of `super.name(args)` after `super`.
    fn parse_super_call(&mut self) -> Result<SuperCall, SomoniError> {
        self.expect(Token::Dot, "'(' or '.' after 'super'")?;
        let (name, name_span) = self.expect_ident("method name after 'super.'")?;
        self.expect(Token::LParen, "'(' after the method name")?;
        let args = self.parse_args()?;
        Ok(SuperCall {
            name,
            name_span,
            args,
            target: None,
        })
    }
}
