//! Checking statements and function bodies: variables, assignments, loops, `return`.

use crate::ast::{
    Assign, BinaryOp, Expr, ExprKind, ForEach, Function, IterKind, Match, Param, Stmt, THIS,
};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::types::Type;

use super::patterns::check_match;
use super::{ObjectField, Signature, TypeChecker};

impl TypeChecker {
    /// Checks statements one after another.
    pub(super) fn check_stmts(&mut self, stmts: &mut [Stmt]) -> Result<(), SomoniError> {
        for stmt in stmts {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    /// Checks a function.
    fn check_function(&mut self, function: &mut Function) -> Result<(), SomoniError> {
        let signature = self
            .names
            .functions
            .get(&function.name)
            .cloned()
            .ok_or_else(|| SomoniError::internal("function without signature"))?;
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

    /// Checks one statement and writes slots and other results into it.
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
            Stmt::Assign(assign) => self.check_assign(assign),
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
            } => self.check_if(condition, then_branch, else_branch.as_deref_mut()),
            Stmt::While {
                condition, body, ..
            } => {
                self.check_condition(condition)?;
                self.check_loop_body_after(condition, body)
            }
            Stmt::For {
                init,
                condition,
                step,
                body,
                ..
            } => self.in_scope(|this| this.check_for(init, condition, step, body)),
            Stmt::ForEach(each) => self.check_for_each(each),
            Stmt::FnDecl(function) => self.check_function(function),
            Stmt::StructDecl(_) | Stmt::EnumDecl(_) | Stmt::InterfaceDecl(_) | Stmt::Import(_) => {
                Ok(())
            }
            Stmt::ClassDecl(class) => self.check_class(class),
            Stmt::Match(m) => self.check_match_stmt(m),
            Stmt::Return { value, span } => self.check_return(value.as_mut(), *span),
            Stmt::Break { span } => self.check_inside_loop("break", *span),
            Stmt::Continue { span } => self.check_inside_loop("continue", *span),
        }
    }

    /// Checks `if`, each branch with the variables its condition proves not to be null.
    fn check_if(
        &mut self,
        condition: &mut Expr,
        then_branch: &mut Stmt,
        else_branch: Option<&mut Stmt>,
    ) -> Result<(), SomoniError> {
        self.check_condition(condition)?;
        let then_known = self.known_narrowed(condition, true);
        let else_known = self.known_narrowed(condition, false);
        self.with_narrowed(then_known, |this| this.check_stmt(then_branch))?;
        if let Some(else_branch) = else_branch {
            self.with_narrowed(else_known, |this| this.check_stmt(else_branch))?;
        }
        Ok(())
    }

    /// Checks a `match` statement: the subject, that every variant is covered, and each branch.
    fn check_match_stmt(&mut self, m: &mut Match) -> Result<(), SomoniError> {
        let ty = self.check_expr(&mut m.subject)?;
        m.arm_of_variant = check_match(&ty, m.subject.span, &m.arms, m.span)?;
        m.unwraps = matches!(ty, Type::Result(_, _));
        for arm in &mut m.arms {
            self.check_arm(arm, &ty)?;
        }
        Ok(())
    }

    /// Checks an assignment and records whether its target lies inside an object.
    fn check_assign(&mut self, assign: &mut Assign) -> Result<(), SomoniError> {
        let target_type = self.check_assign_target(&mut assign.target)?;
        let field = self.object_field(&mut assign.target)?;
        assign.in_object = field.is_some();
        self.check_target_mutable(&assign.target, assign.in_object)?;
        if let Some(field) = &field {
            self.check_not_readonly(field)?;
        }
        match assign.op {
            None => self.check_plain_assign(assign, target_type),
            Some((op, op_span)) => {
                self.check_compound_assign(op, op_span, &mut assign.value, target_type)
            }
        }
    }

    /// Checks `target = value`, where a checked variable may get any value of its declared type.
    fn check_plain_assign(
        &mut self,
        assign: &mut Assign,
        target_type: Type,
    ) -> Result<(), SomoniError> {
        let ExprKind::Ident { name, .. } = &assign.target.kind else {
            self.check_assignable(&mut assign.value, &target_type)?;
            return Ok(());
        };
        let declared = self
            .scopes
            .lookup(name)
            .map_or(target_type, |info| info.declared.clone());
        let found = self.check_assignable(&mut assign.value, &declared)?;
        self.narrow_after_assign(name, found);
        Ok(())
    }

    /// Checks the target of an assignment and returns its type, or T005 for an enum variant like `Farbe.Rot`.
    fn check_assign_target(&mut self, target: &mut Expr) -> Result<Type, SomoniError> {
        if let Some((decl, variant)) = self.enum_variant(target)? {
            return Err(SomoniError::at(
                target.span,
                ErrorCode::AssignToConst {
                    name: format!("{}.{}", decl.name, decl.variants[variant].0),
                },
            ));
        }
        self.check_expr(target)
    }

    /// Returns T005 if the variable of a target outside an object is `const`.
    fn check_target_mutable(&self, target: &Expr, in_object: bool) -> Result<(), SomoniError> {
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
        Ok(())
    }

    /// Checks `target op= value`, whose result must have the type of the target.
    fn check_compound_assign(
        &mut self,
        op: BinaryOp,
        op_span: Span,
        value: &mut Expr,
        target_type: Type,
    ) -> Result<(), SomoniError> {
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
        Ok(())
    }

    /// Returns the field of an object that an assignment to `target` changes, if the way goes through an object.
    /// It is the field right after the last object, because `const` and `readonly` do not protect what lies behind an object.
    fn object_field(&mut self, target: &mut Expr) -> Result<Option<ObjectField>, SomoniError> {
        match &mut target.kind {
            ExprKind::Field {
                target: inner,
                name,
                name_span,
                ..
            } => {
                let Type::Class(layout) = self.check_expr(inner)? else {
                    return self.object_field(inner);
                };
                let through_this =
                    matches!(&inner.kind, ExprKind::Ident { name, .. } if name == THIS);
                Ok(Some(ObjectField {
                    layout,
                    name: name.clone(),
                    span: *name_span,
                    through_this,
                }))
            }
            ExprKind::Index { target: inner, .. } => self.object_field(inner),
            _ => Ok(None),
        }
    }

    /// Checks that `return` is inside a function and returns the right type.
    fn check_return(&mut self, value: Option<&mut Expr>, span: Span) -> Result<(), SomoniError> {
        let Some(expected) = self.current_return.clone() else {
            return Err(SomoniError::at(span, ErrorCode::ReturnOutsideFunction));
        };
        match value {
            Some(value) => {
                self.check_fits(value, &expected, |found| ErrorCode::ReturnType {
                    found,
                    expected: expected.clone(),
                })?;
                Ok(())
            }
            None if expected == Type::Void => Ok(()),
            None => Err(SomoniError::at(
                span,
                ErrorCode::ReturnType {
                    found: Type::Void,
                    expected,
                },
            )),
        }
    }

    /// Checks a `for` loop with start, condition and step.
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
        self.check_loop_body_after(condition, body)
    }

    /// Checks a `for`-each loop and gives the list, its index and the loop variables their slots.
    fn check_for_each(&mut self, each: &mut ForEach) -> Result<(), SomoniError> {
        let ty = self.check_expr(&mut each.iterable)?;
        let span = each.iterable.span;
        let Some((kind, expected)) = loop_var_types(&ty, each.vars.len()) else {
            return Err(SomoniError::at(span, ErrorCode::NotIterable { found: ty }));
        };
        each.over = Some(kind);
        self.in_scope(|this| {
            each.seq_slot = Some(this.declare("#seq", span, ty, false)?);
            this.declare("#index", span, Type::Int, true)?;
            each.slots = this.declare_bound_vars(&each.vars, expected)?;
            this.check_loop_body(&mut each.body)
        })
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

    /// Checks the body of a `while` or `for` loop with the variables its condition proves not to be null.
    fn check_loop_body_after(
        &mut self,
        condition: &Expr,
        body: &mut Stmt,
    ) -> Result<(), SomoniError> {
        let known = self.known_narrowed(condition, true);
        self.with_narrowed(known, |this| this.check_loop_body(body))
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
        Stmt::While { .. } | Stmt::For { .. } | Stmt::ForEach(_) => false,
        Stmt::VarDecl { .. }
        | Stmt::Assign(_)
        | Stmt::Expr { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::FnDecl(_)
        | Stmt::StructDecl(_)
        | Stmt::EnumDecl(_)
        | Stmt::ClassDecl(_)
        | Stmt::InterfaceDecl(_)
        | Stmt::Import(_) => false,
        Stmt::Match(m) => m.arms.iter().all(|arm| stmt_always_returns(&arm.body)),
    }
}
