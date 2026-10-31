//! Reading statements: blocks, `if`, loops, `match`, variables and assignments.

use crate::ast::{
    Assign, BinaryOp, Destructure, Expr, ExprKind, ForEach, Match, MatchArm, Pattern, Stmt,
    TUPLE_TYPE, TypeName, VarDecl,
};
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
            Token::Var | Token::Const if *self.peek_next() == Token::LParen => {
                let mutable = *self.peek() == Token::Var;
                self.parse_destructure(mutable, start)
            }
            Token::Var => Ok(Stmt::VarDecl(self.parse_var_decl(true, start)?)),
            Token::Const => Ok(Stmt::VarDecl(self.parse_var_decl(false, start)?)),
            Token::LBrace => self.parse_block(start),
            Token::If => self.parse_if(start),
            Token::While => self.parse_while(start),
            Token::For => self.parse_for(start),
            Token::Break => self.parse_jump(Stmt::Break { span: start }, "';' after 'break'"),
            Token::Continue => {
                self.parse_jump(Stmt::Continue { span: start }, "';' after 'continue'")
            }
            Token::Match => self.parse_match(start),
            Token::Return => self.parse_return(start),
            Token::Ident(_) | Token::This | Token::New | Token::Super => self.parse_assign(start),
            other => Err(SomoniError::at(start, misplaced(other))),
        }
    }

    /// Reads `break;` or `continue;` as the statement `stmt`.
    fn parse_jump(&mut self, stmt: Stmt, semicolon: &str) -> Result<Stmt, SomoniError> {
        self.advance();
        self.expect(Token::Semicolon, semicolon)?;
        Ok(stmt)
    }

    /// Reads `return;` or `return value;`.
    fn parse_return(&mut self, start: Span) -> Result<Stmt, SomoniError> {
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

    /// Reads `match (subject) { Farbe.Rot => { ... } _ => { ... } }`.
    fn parse_match(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        self.advance();
        self.expect(Token::LParen, "'(' after 'match'")?;
        let subject = self.parse_expr()?;
        self.expect(Token::RParen, "')' after match value")?;
        let open = self.current_span();
        self.expect(Token::LBrace, "'{' after match value")?;
        let arms = self.parse_until_brace(open, Self::parse_arm)?;
        Ok(Stmt::Match(Match {
            subject,
            arms,
            arm_of_variant: Vec::new(),
            unwraps: false,
            span: start,
        }))
    }

    /// Reads one branch `pattern => { ... }` of a `match`.
    fn parse_arm(&mut self) -> Result<MatchArm, SomoniError> {
        let pattern = self.parse_pattern()?;
        self.expect(Token::FatArrow, "'=>'")?;
        let body = self.expect_block("after '=>'")?;
        Ok(MatchArm { pattern, body })
    }

    /// Reads what a branch of `match` stands for: `Farbe.Rot`, `ok(n: int)`, `err(e: string)` or `_`.
    fn parse_pattern(&mut self) -> Result<Pattern, SomoniError> {
        if self.at_word("_") {
            let span = self.current_span();
            self.advance();
            return Ok(Pattern::Rest(span));
        }
        if let Token::Ident(name) = self.peek()
            && let Some(builtin) = Builtin::result_builder(name)
            && *self.peek_next() == Token::LParen
        {
            return self.parse_result_pattern(builtin);
        }
        self.parse_variant_pattern()
    }

    /// Reads `ok(n: int)` or `err(e: string)` in a branch of `match`.
    fn parse_result_pattern(&mut self, builtin: Builtin) -> Result<Pattern, SomoniError> {
        let span = self.current_span();
        self.advance();
        self.advance();
        let var = self.parse_param("variable")?;
        self.expect(Token::RParen, "')' after the variable")?;
        Ok(Pattern::Result {
            ok: builtin == Builtin::Ok,
            var,
            slot: None,
            span,
        })
    }

    /// Reads a variant like `Farbe.Rot` in a branch of `match`.
    fn parse_variant_pattern(&mut self) -> Result<Pattern, SomoniError> {
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

    /// Reads a block `{ ... }` whose `{` is at `open`.
    fn parse_block(&mut self, open: Span) -> Result<Stmt, SomoniError> {
        Ok(Stmt::Block(self.parse_block_stmts(open)?))
    }

    /// Reads the statements of a block `{ ... }` whose `{` is at `open`.
    fn parse_block_stmts(&mut self, open: Span) -> Result<Vec<Stmt>, SomoniError> {
        self.advance();
        self.parse_until_brace(open, Self::parse_stmt)
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
        let else_branch = self.parse_else()?;
        Ok(Stmt::If {
            condition,
            then_branch: Box::new(then_branch),
            else_branch,
            span: start,
        })
    }

    /// Reads `else if ...` or `else { ... }` after an `if`, if there is one.
    fn parse_else(&mut self) -> Result<Option<Box<Stmt>>, SomoniError> {
        if !self.eat(Token::Else) {
            return Ok(None);
        }
        let next = self.current_span();
        let branch = match self.peek() {
            Token::If => self.parse_if(next)?,
            Token::LBrace => self.parse_block(next)?,
            _ => return Err(self.expected("'{' or 'if' after 'else'")),
        };
        Ok(Some(Box::new(branch)))
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

    /// Reads `for (var i: int = 0; i < 10; i = i + 1) { ... }` or a `for`-each loop.
    fn parse_for(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        self.advance();
        self.expect(Token::LParen, "'(' after 'for'")?;
        if let Token::Ident(_) = self.peek()
            && *self.peek_next() == Token::Colon
        {
            return self.parse_foreach(start);
        }
        let init = self.parse_for_init()?;
        let condition = self.parse_expr()?;
        self.expect(Token::Semicolon, "';' after for condition")?;
        let step = self.parse_for_step()?;
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

    /// Reads the start `var i: int = 0;` of a `for` loop, or returns P004 for `const`.
    fn parse_for_init(&mut self) -> Result<Stmt, SomoniError> {
        let span = self.current_span();
        match self.peek() {
            Token::Var => Ok(Stmt::VarDecl(self.parse_var_decl(true, span)?)),
            Token::Const => Err(SomoniError::at(span, ErrorCode::ForStartConst)),
            _ => Err(self.expected("'var' after 'for ('")),
        }
    }

    /// Reads the step `i = i + 1` of a `for` loop.
    fn parse_for_step(&mut self) -> Result<Stmt, SomoniError> {
        let span = self.current_span();
        match self.peek() {
            Token::Ident(_) => self.parse_assignment(span),
            _ => Err(SomoniError::at(
                span,
                ErrorCode::Expected {
                    what: "assignment as for step".to_string(),
                },
            )),
        }
    }

    /// Reads the rest of `for (name: string in namen) { ... }` after `for (`.
    fn parse_foreach(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        let mut vars = vec![self.parse_param("loop variable")?];
        if self.eat(Token::Comma) {
            vars.push(self.parse_param("loop variable")?);
        }
        self.expect(Token::In, "'in'")?;
        let iterable = self.parse_expr()?;
        self.expect(Token::RParen, "')' after the list")?;
        let body = self.expect_block("after for header")?;
        Ok(Stmt::ForEach(ForEach {
            vars,
            iterable,
            body: Box::new(body),
            slots: Vec::new(),
            seq_slot: None,
            over: None,
            span: start,
        }))
    }

    /// Reads an assignment with `;`, e.g. `x = 5;`.
    fn parse_assign(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        let assign = self.parse_assignment(start)?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(assign)
    }

    /// Reads an assignment without `;` like `x += 1` or `x++`, or a call statement like `k.einzahlen(5)`.
    fn parse_assignment(&mut self, start: Span) -> Result<Stmt, SomoniError> {
        let target = self.parse_postfix()?;
        if let Some(op) = self.step_op() {
            return self.finish_step(target, op, start);
        }
        let is_assignment = *self.peek() == Token::Equal || compound_op(self.peek()).is_some();
        if stands_alone(&target) && !is_assignment {
            return Ok(Stmt::Expr {
                expr: target,
                span: self.since(start),
            });
        }
        self.finish_assignment(target, start)
    }

    /// Returns `+` for `++` and `-` for `--` after an assignment target: two signs with no space between them.
    /// Nothing else may follow a target there, so `--x` in an expression stays two minus signs.
    fn step_op(&self) -> Option<BinaryOp> {
        let op = match (self.peek(), self.peek_next()) {
            (Token::Plus, Token::Plus) => BinaryOp::Add,
            (Token::Minus, Token::Minus) => BinaryOp::Sub,
            _ => return None,
        };
        let first = self.tokens[self.pos].span;
        let second = self.tokens[self.pos + 1].span;
        let touching = second.line == first.end_line && second.col == first.end_col;
        touching.then_some(op)
    }

    /// Reads `++` or `--` after `target` as `target += 1` or `target -= 1`, or returns P007 if the target is no variable or part of one.
    fn finish_step(
        &mut self,
        target: Expr,
        op: BinaryOp,
        start: Span,
    ) -> Result<Stmt, SomoniError> {
        if target.assign_root().is_none() {
            return Err(SomoniError::at(target.span, ErrorCode::InvalidAssignTarget));
        }
        let op_span = self.current_span().to(self.tokens[self.pos + 1].span);
        self.advance();
        self.advance();
        Ok(Stmt::Assign(Assign {
            target,
            op: Some((op, op_span)),
            value: Expr {
                kind: ExprKind::Int(1),
                span: op_span,
            },
            in_object: false,
            span: self.since(start),
        }))
    }

    /// Reads `= value` or `+= value` after `target`, or returns P007 if the target is no variable or part of one.
    fn finish_assignment(&mut self, target: Expr, start: Span) -> Result<Stmt, SomoniError> {
        if target.assign_root().is_none() {
            return Err(SomoniError::at(target.span, ErrorCode::InvalidAssignTarget));
        }
        let op = compound_op(self.peek()).map(|op| (op, self.current_span()));
        if op.is_none() && *self.peek() != Token::Equal {
            return Err(self.expected("'='"));
        }
        self.advance();
        let value = self.parse_expr()?;
        Ok(Stmt::Assign(Assign {
            target,
            op,
            value,
            in_object: false,
            span: self.since(start),
        }))
    }

    /// Reads `var (q: int, r: int) = value;`: a hidden variable holds the tuple, every name reads one element of it.
    fn parse_destructure(&mut self, mutable: bool, start: Span) -> Result<Stmt, SomoniError> {
        self.advance();
        self.advance();
        let names =
            self.parse_comma_list(Token::RParen, "')'", |this| this.parse_param("variable"))?;
        self.expect(Token::Equal, "'='")?;
        let value = self.parse_expr()?;
        let span = self.since(start);
        self.expect(Token::Semicolon, "';'")?;
        let hidden = format!("<tuple {}:{}>", start.line, start.col);
        let tuple_type = TypeName {
            name: TUPLE_TYPE.to_string(),
            args: names.iter().map(|name| name.type_name.clone()).collect(),
            optional: false,
            span,
        };
        let tuple = VarDecl {
            exported: false,
            name: hidden.clone(),
            name_span: start,
            type_name: tuple_type,
            value,
            mutable: true,
            slot: None,
            global: None,
            span,
        };
        let parts = names
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let read = ExprKind::TupleIndex {
                    target: Box::new(Expr {
                        kind: ExprKind::Ident {
                            name: hidden.clone(),
                            slot: None,
                        },
                        span: name.name_span,
                    }),
                    index,
                };
                VarDecl {
                    exported: false,
                    value: Expr {
                        kind: read,
                        span: name.name_span,
                    },
                    name: name.name,
                    name_span: name.name_span,
                    type_name: name.type_name,
                    mutable,
                    slot: None,
                    global: None,
                    span,
                }
            })
            .collect();
        Ok(Stmt::Destructure(Destructure { tuple, parts }))
    }

    /// Reads `var name: type = value;` or `const name: type = value;`.
    pub(super) fn parse_var_decl(
        &mut self,
        mutable: bool,
        start: Span,
    ) -> Result<VarDecl, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("variable name")?;
        self.expect(Token::Colon, "':'")?;
        let type_name = self.parse_type("type name")?;
        self.expect(Token::Equal, "'='")?;
        let value = self.parse_expr()?;
        let span = self.since(start);
        self.expect(Token::Semicolon, "';'")?;
        Ok(VarDecl {
            exported: false,
            name,
            name_span,
            type_name,
            value,
            mutable,
            slot: None,
            global: None,
            span,
        })
    }
}

/// Returns the error for a token that cannot start a statement, like `fn` inside a block.
fn misplaced(token: Token) -> ErrorCode {
    match token {
        Token::Fn => ErrorCode::FnNotTopLevel,
        Token::Struct => ErrorCode::StructNotTopLevel,
        Token::Enum => ErrorCode::EnumNotTopLevel,
        Token::Class | Token::Abstract => ErrorCode::ClassNotTopLevel,
        Token::Interface => ErrorCode::InterfaceNotTopLevel,
        Token::Import => ErrorCode::ImportNotFirst,
        Token::Export => ErrorCode::ExportNotTopLevel,
        found => ErrorCode::UnexpectedToken { found },
    }
}

/// Returns the operator of `+=`, `-=`, `*=`, `/=` or `%=`.
fn compound_op(token: &Token) -> Option<BinaryOp> {
    match token {
        Token::PlusEqual => Some(BinaryOp::Add),
        Token::MinusEqual => Some(BinaryOp::Sub),
        Token::StarEqual => Some(BinaryOp::Mul),
        Token::SlashEqual => Some(BinaryOp::Div),
        Token::PercentEqual => Some(BinaryOp::Mod),
        _ => None,
    }
}

/// Returns true for a call of any kind, `new`, `super` or one of them with `?`, which may stand alone as a statement.
fn stands_alone(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Call(_)
        | ExprKind::MethodCall(_)
        | ExprKind::StaticCall(_)
        | ExprKind::New(_)
        | ExprKind::SuperInit { .. }
        | ExprKind::SuperCall(_)
        | ExprKind::CallValue(_) => true,
        ExprKind::Try(inner) => stands_alone(inner),
        _ => false,
    }
}
