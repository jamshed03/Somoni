//! Reading statements: blocks, `if`, loops, `match`, variables and assignments.

use crate::ast::{BinaryOp, ExprKind, MatchArm, Pattern, Stmt};
use crate::builtins::Builtin;
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::Token;

use super::Parser;

impl Parser {
    /// Reads one statement inside a block or at the top level.
    pub(super) fn parse_stmt(&mut self) -> Result<Stmt, SomoniError> {
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
            Token::Enum => Err(SomoniError::at(start, ErrorCode::EnumNotTopLevel)),
            Token::Class => Err(SomoniError::at(start, ErrorCode::ClassNotTopLevel)),
            Token::Interface => Err(SomoniError::at(start, ErrorCode::InterfaceNotTopLevel)),
            Token::Import => Err(SomoniError::at(start, ErrorCode::ImportNotFirst)),
            Token::Export => Err(SomoniError::at(start, ErrorCode::ExportNotTopLevel)),
            Token::Match => self.parse_match(start),
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
            Token::Ident(_) | Token::This | Token::New | Token::Super => self.parse_assign(start),
            other => Err(SomoniError::at(
                start,
                ErrorCode::UnexpectedToken { found: other },
            )),
        }
    }

    /// Reads `match (subject) { Farbe.Rot => { ... } _ => { ... } }`.
    fn parse_match(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        self.advance();
        self.expect(Token::LParen, "'(' after 'match'")?;
        let subject = self.parse_expr()?;
        self.expect(Token::RParen, "')' after match value")?;
        let open = self.current_span();
        self.expect(Token::LBrace, "'{' after match value")?;
        let mut arms = Vec::new();
        while *self.peek() != Token::RBrace {
            if *self.peek() == Token::Eof {
                return Err(SomoniError::at(open, ErrorCode::BlockNotClosed));
            }
            let pattern = self.parse_pattern()?;
            self.expect(Token::FatArrow, "'=>'")?;
            let body = self.expect_block("after '=>'")?;
            arms.push(MatchArm { pattern, body });
        }
        self.advance();
        Ok(Stmt::Match {
            subject,
            arms,
            arm_of_variant: Vec::new(),
            unwraps: false,
            span: start,
        })
    }

    /// Reads what a branch of `match` stands for: `Farbe.Rot`, `ok(n: int)`, `err(e: string)` or `_`.
    fn parse_pattern(&mut self) -> Result<Pattern, SomoniError> {
        if let Token::Ident(name) = self.peek()
            && name == "_"
        {
            let span = self.current_span();
            self.advance();
            return Ok(Pattern::Rest(span));
        }
        if let Token::Ident(name) = self.peek()
            && let Some(builtin) = Builtin::result_builder(name)
            && *self.peek_next() == Token::LParen
        {
            let span = self.current_span();
            self.advance();
            self.advance();
            let var = self.parse_param("variable")?;
            self.expect(Token::RParen, "')' after the variable")?;
            return Ok(Pattern::Result {
                ok: builtin == Builtin::Ok,
                var,
                slot: None,
                span,
            });
        }
        let (enum_name, enum_span) =
            self.expect_ident("a branch like 'Farbe.Rot', 'ok(n: int)' or '_'")?;
        self.expect(Token::Dot, "'.' after enum name")?;
        let (name, name_span) = self.expect_ident("variant name")?;
        Ok(Pattern::Variant {
            enum_name,
            enum_span,
            name,
            name_span,
        })
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
    pub(super) fn expect_block_stmts(&mut self, what: &str) -> Result<Vec<Stmt>, SomoniError> {
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
        if let Token::Ident(_) = self.peek()
            && *self.peek_next() == Token::Colon
        {
            return self.parse_foreach(start);
        }

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

    /// Reads the rest of `for (name: string in namen) { ... }` or `for (i: int, name: string in ...)`.
    /// `for (` is already read.
    fn parse_foreach(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        let mut vars = vec![self.parse_param("loop variable")?];
        if *self.peek() == Token::Comma {
            self.advance();
            vars.push(self.parse_param("loop variable")?);
        }
        self.expect(Token::In, "'in'")?;
        let iterable = self.parse_expr()?;
        self.expect(Token::RParen, "')' after the list")?;
        let body = self.expect_block("after for header")?;
        Ok(Stmt::ForEach {
            vars,
            iterable,
            body: Box::new(body),
            slots: Vec::new(),
            seq_slot: None,
            over: None,
            span: start,
        })
    }

    /// Reads an assignment with `;`, e.g. `x = 5;`.
    fn parse_assign(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        let assign = self.parse_assignment(start)?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(assign)
    }

    /// Reads an assignment without `;`, e.g. `x = 5`, `l[i] = 5`, `u.name = "Ali"` or `x += 1`.
    /// A method call like `k.einzahlen(5)` or `new K()` is read here too, as a statement
    /// without `=`.
    fn parse_assignment(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        let target = self.parse_postfix()?;
        let is_assignment = matches!(
            self.peek(),
            Token::Equal
                | Token::PlusEqual
                | Token::MinusEqual
                | Token::StarEqual
                | Token::SlashEqual
                | Token::PercentEqual
        );
        if let ExprKind::MethodCall { .. }
        | ExprKind::New { .. }
        | ExprKind::SuperInit { .. }
        | ExprKind::SuperCall { .. } = target.kind
            && !is_assignment
        {
            return Ok(Stmt::Expr {
                expr: target,
                span: self.since(start),
            });
        }
        if target.root_variable().is_none() {
            return Err(SomoniError::at(target.span, ErrorCode::InvalidAssignTarget));
        }
        let op = match self.peek() {
            Token::Equal => None,
            Token::PlusEqual => Some(BinaryOp::Add),
            Token::MinusEqual => Some(BinaryOp::Sub),
            Token::StarEqual => Some(BinaryOp::Mul),
            Token::SlashEqual => Some(BinaryOp::Div),
            Token::PercentEqual => Some(BinaryOp::Mod),
            _ => return Err(self.expected("'='")),
        };
        let op = op.map(|op| (op, self.current_span()));
        self.advance();
        let value = self.parse_expr()?;
        Ok(Stmt::Assign {
            target,
            op,
            value,
            in_object: false,
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
}
