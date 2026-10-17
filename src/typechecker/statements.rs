//! Checking statements and function bodies: variables, assignments, loops, `return`, null checks.

use crate::ast::{BinaryOp, Expr, ExprKind, Function, IterKind, Param, Stmt, THIS, UnaryOp};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::types::Type;

use super::{Signature, TypeChecker, VarInfo};

impl TypeChecker {
    pub(super) fn check_stmts(&mut self, stmts: &mut [Stmt]) -> Result<(), SomoniError> {
        for stmt in stmts {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    /// Checks a function.
    fn check_function(&mut self, function: &mut Function) -> Result<(), SomoniError> {
        let signature = self.names.functions[&function.name].clone();
        let name = function.name.clone();
        self.check_body(function, &name, &signature, None)
    }

    /// Checks the body of a function or method and that it returns on every path.
    pub(super) fn check_body(
        &mut self,
        function: &mut Function,
        name: &str,
        signature: &Signature,
        this_type: Option<Type>,
    ) -> Result<(), SomoniError> {
        let return_type = signature.return_type.clone();
        self.in_function(Some(return_type.clone()), |this| {
            if let Some(ty) = this_type {
                this.declare(THIS, function.name_span, ty, false)?;
            }
            this.declare_params(&function.params, &signature.params)?;
            this.check_stmts(&mut function.body)
        })?;

        if return_type != Type::Void && !always_returns(&function.body) {
            return Err(SomoniError::at(
                function.return_type.span,
                ErrorCode::MissingReturn {
                    name: name.to_string(),
                    expected: return_type,
                },
            ));
        }
        Ok(())
    }

    pub(super) fn check_stmt(&mut self, stmt: &mut Stmt) -> Result<(), SomoniError> {
        match stmt {
            Stmt::VarDecl {
                name,
                name_span,
                type_name,
                value,
                mutable,
                slot,
                ..
            } => {
                let declared = self.resolve_type(type_name, false)?;
                self.check_assignable(value, &declared)?;
                *slot = Some(self.declare(name, *name_span, declared, *mutable)?);
                Ok(())
            }
            Stmt::Assign {
                target,
                op,
                value,
                in_object,
                ..
            } => {
                *in_object = self.check_assign(target, *op, value)?;
                Ok(())
            }
            Stmt::Expr { expr, .. } => {
                self.check_expr(expr)?;
                Ok(())
            }
            Stmt::Block(stmts) => self.in_scope(|this| this.check_stmts(stmts)),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.check_condition(condition)?;
                let then_known = self.known_not_null(condition, true);
                let else_known = self.known_not_null(condition, false);
                self.with_not_null(then_known, |this| this.check_stmt(then_branch))?;
                if let Some(else_branch) = else_branch {
                    self.with_not_null(else_known, |this| this.check_stmt(else_branch))?;
                }
                Ok(())
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.check_condition(condition)?;
                let known = self.known_not_null(condition, true);
                self.with_not_null(known, |this| this.check_loop_body(body))
            }
            Stmt::For {
                init,
                condition,
                step,
                body,
                ..
            } => self.in_scope(|this| this.check_for(init, condition, step, body)),
            Stmt::ForEach {
                vars,
                iterable,
                body,
                slots,
                seq_slot,
                over,
                ..
            } => {
                let ty = self.check_expr(iterable)?;
                let Some((kind, expected)) = loop_var_types(&ty, vars.len()) else {
                    return Err(SomoniError::at(
                        iterable.span,
                        ErrorCode::NotIterable { found: ty },
                    ));
                };
                *over = Some(kind);
                let span = iterable.span;
                self.in_scope(|this| {
                    *seq_slot = Some(this.declare("#seq", span, ty, false)?);
                    this.declare("#index", span, Type::Int, true)?;
                    *slots = this.declare_bound_vars(vars, expected)?;
                    this.check_loop_body(body)
                })
            }
            Stmt::FnDecl(function) => self.check_function(function),
            Stmt::StructDecl(_) | Stmt::EnumDecl(_) | Stmt::InterfaceDecl(_) | Stmt::Import(_) => {
                Ok(())
            }
            Stmt::ClassDecl(class) => self.check_class(class),
            Stmt::Match {
                subject,
                arms,
                arm_of_variant,
                unwraps,
                span,
            } => {
                let ty = self.check_expr(subject)?;
                *arm_of_variant = self.check_match(&ty, subject.span, arms, *span)?;
                *unwraps = matches!(ty, Type::Result(_, _));
                for arm in arms {
                    self.check_arm(arm, &ty)?;
                }
                Ok(())
            }
            Stmt::Return { value, span } => self.check_return(value.as_mut(), *span),
            Stmt::Break { span } => self.check_inside_loop("break", *span),
            Stmt::Continue { span } => self.check_inside_loop("continue", *span),
        }
    }

    /// Runs `check` while the variables in `known` are known not to be null.
    pub(super) fn with_not_null<T>(
        &mut self,
        known: Vec<(String, Type)>,
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.scopes.push();
        for (name, ty) in known {
            if let Some(info) = self.scopes.lookup(&name) {
                let narrowed = VarInfo {
                    ty,
                    mutable: info.mutable,
                    slot: info.slot,
                };
                self.scopes.declare(&name, narrowed);
            }
        }
        let result = check(self);
        self.scopes.pop();
        result
    }

    /// Returns the variables that cannot be null when `condition` is `when`.
    pub(super) fn known_not_null(&self, condition: &Expr, when: bool) -> Vec<(String, Type)> {
        match &condition.kind {
            ExprKind::Binary {
                left, op, right, ..
            } => match (op, when) {
                (BinaryOp::NotEq, true) | (BinaryOp::Eq, false) => {
                    self.null_checked(left, right).into_iter().collect()
                }
                (BinaryOp::And, true) | (BinaryOp::Or, false) => {
                    let mut known = self.known_not_null(left, when);
                    known.extend(self.known_not_null(right, when));
                    known
                }
                _ => Vec::new(),
            },
            ExprKind::Unary {
                op: UnaryOp::Not,
                operand,
            } => self.known_not_null(operand, !when),
            _ => Vec::new(),
        }
    }

    /// Returns the variable and its type without `?` for `a != null`.
    fn null_checked(&self, left: &Expr, right: &Expr) -> Option<(String, Type)> {
        let name = match (&left.kind, &right.kind) {
            (ExprKind::Ident { name, .. }, ExprKind::Null)
            | (ExprKind::Null, ExprKind::Ident { name, .. }) => name,
            _ => return None,
        };
        let Type::Optional(inner) = &self.scopes.lookup(name)?.ty else {
            return None;
        };
        Some((name.clone(), (**inner).clone()))
    }

    /// Checks an assignment and returns true if its target lies inside an object.
    fn check_assign(
        &mut self,
        target: &mut Expr,
        op: Option<(BinaryOp, Span)>,
        value: &mut Expr,
    ) -> Result<bool, SomoniError> {
        if let Some((decl, variant)) = self.enum_variant(target)? {
            return Err(SomoniError::at(
                target.span,
                ErrorCode::AssignToConst {
                    name: format!("{}.{}", decl.name, decl.variants[variant].0),
                },
            ));
        }
        let target_type = self.check_expr(target)?;
        let in_object = self.target_in_object(target)?;
        let Some((name, name_span)) = target.root_variable() else {
            return Err(SomoniError::internal("assignment to a non-variable"));
        };
        if !in_object && !self.scopes.lookup(name).is_some_and(|info| info.mutable) {
            return Err(SomoniError::at(
                name_span,
                ErrorCode::AssignToConst {
                    name: name.to_string(),
                },
            ));
        }
        let Some((op, op_span)) = op else {
            self.check_assignable(value, &target_type)?;
            return Ok(in_object);
        };
        let value_type = self.check_expr_expecting(value, &target_type)?;
        let result = self.binary_type(op, target_type.clone(), value_type, op_span)?;
        if result != target_type {
            return Err(SomoniError::at(
                op_span,
                ErrorCode::CannotAssign {
                    found: result,
                    expected: target_type,
                },
            ));
        }
        Ok(in_object)
    }

    /// Returns true if an assignment changes an object, which `const` does not protect.
    fn target_in_object(&mut self, target: &mut Expr) -> Result<bool, SomoniError> {
        let (ExprKind::Field { target: inner, .. } | ExprKind::Index { target: inner, .. }) =
            &mut target.kind
        else {
            return Ok(false);
        };
        if let Type::Class(_) = self.check_expr(inner)? {
            return Ok(true);
        }
        self.target_in_object(inner)
    }

    /// Checks that `return` is inside a function and returns the right type.
    fn check_return(&mut self, value: Option<&mut Expr>, span: Span) -> Result<(), SomoniError> {
        let Some(expected) = &self.current_return.clone() else {
            return Err(SomoniError::at(span, ErrorCode::ReturnOutsideFunction));
        };
        let (found, place) = match value {
            Some(value) => (self.check_expr_expecting(value, expected)?, value.span),
            None => (Type::Void, span),
        };
        if !self.fits(&found, expected) {
            return Err(SomoniError::at(
                place,
                ErrorCode::ReturnType {
                    found,
                    expected: expected.clone(),
                },
            ));
        }
        Ok(())
    }

    fn check_for(
        &mut self,
        init: &mut Stmt,
        condition: &mut Expr,
        step: &mut Stmt,
        body: &mut Stmt,
    ) -> Result<(), SomoniError> {
        self.check_stmt(init)?;
        self.check_condition(condition)?;
        self.check_stmt(step)?;
        let known = self.known_not_null(condition, true);
        self.with_not_null(known, |this| this.check_loop_body(body))
    }

    /// Declares the variables of a `for`-each loop or of `ok(n: int)` and returns their slots.
    pub(super) fn declare_bound_vars(
        &mut self,
        vars: &[Param],
        expected: Vec<Type>,
    ) -> Result<Vec<usize>, SomoniError> {
        let mut slots = Vec::new();
        for (var, expected) in vars.iter().zip(expected) {
            let declared = self.resolve_type(&var.type_name, false)?;
            if declared != expected {
                return Err(SomoniError::at(
                    var.type_name.span,
                    ErrorCode::BoundVarType {
                        name: var.name.clone(),
                        expected,
                        found: declared,
                    },
                ));
            }
            slots.push(self.declare(&var.name, var.name_span, declared, false)?);
        }
        Ok(slots)
    }

    /// Checks a loop body, where `break` and `continue` are allowed.
    fn check_loop_body(&mut self, body: &mut Stmt) -> Result<(), SomoniError> {
        self.loop_depth += 1;
        let result = self.check_stmt(body);
        self.loop_depth -= 1;
        result
    }

    /// Checks that `break` or `continue` is inside a loop.
    fn check_inside_loop(&self, keyword: &'static str, span: Span) -> Result<(), SomoniError> {
        if self.loop_depth == 0 {
            return Err(SomoniError::at(span, ErrorCode::OutsideLoop { keyword }));
        }
        Ok(())
    }

    /// Checks that a condition of `if`, `while` or `for` is a bool.
    fn check_condition(&mut self, condition: &mut Expr) -> Result<(), SomoniError> {
        let ty = self.check_expr(condition)?;
        if ty != Type::Bool {
            return Err(SomoniError::at(
                condition.span,
                ErrorCode::ConditionNotBool { found: ty },
            ));
        }
        Ok(())
    }
}

/// Returns what a `for`-each loop over `ty` runs over and the types of its variables.
fn loop_var_types(ty: &Type, count: usize) -> Option<(IterKind, Vec<Type>)> {
    let (kind, first, second) = match ty {
        Type::List(element) => (IterKind::List, Type::Int, (**element).clone()),
        Type::Map(key, value) => (IterKind::Map, (**key).clone(), (**value).clone()),
        Type::Str => (IterKind::Str, Type::Int, Type::Str),
        _ => return None,
    };
    let types = match (kind, count) {
        (_, 2) => vec![first, second],
        (IterKind::Map, _) => vec![first],
        (IterKind::List | IterKind::Str, _) => vec![second],
    };
    Some((kind, types))
}

/// Returns true if every path through these statements ends with `return`.
fn always_returns(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_always_returns)
}

/// Returns true if this statement always ends with `return`.
fn stmt_always_returns(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return { .. } => true,
        Stmt::Block(stmts) => always_returns(stmts),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => match else_branch {
            Some(else_branch) => {
                stmt_always_returns(then_branch) && stmt_always_returns(else_branch)
            }
            None => false,
        },
        Stmt::While { .. } | Stmt::For { .. } | Stmt::ForEach { .. } => false,
        Stmt::VarDecl { .. }
        | Stmt::Assign { .. }
        | Stmt::Expr { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::FnDecl(_)
        | Stmt::StructDecl(_)
        | Stmt::EnumDecl(_)
        | Stmt::ClassDecl(_)
        | Stmt::InterfaceDecl(_)
        | Stmt::Import(_) => false,
        Stmt::Match { arms, .. } => arms.iter().all(|arm| stmt_always_returns(&arm.body)),
    }
}
