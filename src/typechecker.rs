use crate::ast::{BinaryOp, Expr, Stmt, UnaryOp};
use crate::error::SomoniError;
use crate::scope::Scopes;
use crate::types::Type;

struct VarInfo {
    ty: Type,
    mutable: bool,
}

pub struct TypeChecker {
    scopes: Scopes<VarInfo>,
    loop_depth: usize,
}

impl TypeChecker {
    pub fn new() -> TypeChecker {
        TypeChecker {
            scopes: Scopes::new(),
            loop_depth: 0,
        }
    }

    pub fn check(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for stmt in stmts {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), SomoniError> {
        match stmt {
            Stmt::VarDecl {
                name,
                type_name,
                value,
                mutable,
                line,
            } => {
                let declared = Type::from_name(type_name).ok_or(SomoniError::type_error(
                    *line,
                    format!("unknown type '{}'", type_name),
                ))?;
                let actual = self.check_expr(value, *line)?;
                if declared != actual {
                    return Err(SomoniError::type_error(
                        *line,
                        format!("cannot assign {} to {}", actual.name(), declared.name()),
                    ));
                }
                let info = VarInfo {
                    ty: declared,
                    mutable: *mutable,
                };
                if !self.scopes.declare(name, info) {
                    return Err(SomoniError::type_error(
                        *line,
                        format!("'{}' is already declared in this block", name),
                    ));
                }
                Ok(())
            }
            Stmt::Print { value, line } => {
                self.check_expr(value, *line)?;
                Ok(())
            }
            Stmt::Assign { name, value, line } => {
                let info = self.scopes.lookup(name).ok_or(SomoniError::type_error(
                    *line,
                    format!("unknown variable '{}'", name),
                ))?;
                if !info.mutable {
                    return Err(SomoniError::type_error(
                        *line,
                        format!("cannot assign to const '{}'", name),
                    ));
                }
                let expected = info.ty;
                let actual = self.check_expr(value, *line)?;
                if expected != actual {
                    return Err(SomoniError::type_error(
                        *line,
                        format!("cannot assign {} to {}", actual.name(), expected.name()),
                    ));
                }
                Ok(())
            }
            Stmt::Block(stmts) => {
                self.scopes.push();
                let result = self.check(stmts);
                self.scopes.pop();
                result
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                line,
            } => {
                self.check_condition(condition, *line)?;
                // both branches are checked, even the one that will not run
                self.check_stmt(then_branch)?;
                if let Some(else_branch) = else_branch {
                    self.check_stmt(else_branch)?;
                }
                Ok(())
            }
            Stmt::While {
                condition,
                body,
                line,
            } => {
                self.check_condition(condition, *line)?;
                // checked once, even if it runs many times (or never)
                self.check_loop_body(body)
            }
            Stmt::For {
                init,
                condition,
                step,
                body,
                line,
            } => {
                // the counter lives in its own scope around the whole loop, gone afterwards
                self.scopes.push();
                let result = self.check_for(init, condition, step, body, *line);
                self.scopes.pop();
                result
            }
            Stmt::Break { line } => self.check_inside_loop("break", *line),
            Stmt::Continue { line } => self.check_inside_loop("continue", *line),
        }
    }

    fn check_for(
        &mut self,
        init: &Stmt,
        condition: &Expr,
        step: &Stmt,
        body: &Stmt,
        line: usize,
    ) -> Result<(), SomoniError> {
        self.check_stmt(init)?;
        self.check_condition(condition, line)?;
        self.check_stmt(step)?;
        self.check_loop_body(body)
    }

    fn check_loop_body(&mut self, body: &Stmt) -> Result<(), SomoniError> {
        self.loop_depth += 1;
        let result = self.check_stmt(body);
        self.loop_depth -= 1;
        result
    }

    fn check_inside_loop(&self, keyword: &str, line: usize) -> Result<(), SomoniError> {
        if self.loop_depth == 0 {
            return Err(SomoniError::type_error(
                line,
                format!("'{}' outside of a loop", keyword),
            ));
        }
        Ok(())
    }

    // conditions of if and while must be bool, `if (1)` is an error
    fn check_condition(&self, condition: &Expr, line: usize) -> Result<(), SomoniError> {
        let ty = self.check_expr(condition, line)?;
        if ty != Type::Bool {
            return Err(SomoniError::type_error(
                line,
                format!("condition must be bool, got {}", ty.name()),
            ));
        }
        Ok(())
    }

    fn check_expr(&self, expr: &Expr, line: usize) -> Result<Type, SomoniError> {
        match expr {
            Expr::Int(_) => Ok(Type::Int),
            Expr::Float(_) => Ok(Type::Float),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::Str(_) => Ok(Type::Str),
            Expr::Ident(name) => {
                self.scopes
                    .lookup(name)
                    .map(|info| info.ty)
                    .ok_or(SomoniError::type_error(
                        line,
                        format!("unknown variable '{}'", name),
                    ))
            }
            Expr::Binary { left, op, right } => {
                let l = self.check_expr(left, line)?;
                let r = self.check_expr(right, line)?;
                if l != r {
                    return Err(SomoniError::type_error(
                        line,
                        format!("cannot use operator on {} and {}", l.name(), r.name()),
                    ));
                }
                match (l, op) {
                    (_, BinaryOp::Eq | BinaryOp::NotEq) => Ok(Type::Bool),
                    (Type::Int, BinaryOp::Mod) => Ok(Type::Int),
                    (
                        Type::Int | Type::Float,
                        BinaryOp::Less | BinaryOp::Greater | BinaryOp::LessEq | BinaryOp::GreaterEq,
                    ) => Ok(Type::Bool),
                    (
                        Type::Int | Type::Float,
                        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div,
                    ) => Ok(l),
                    (Type::Str, BinaryOp::Add) => Ok(Type::Str),
                    (Type::Bool, BinaryOp::And | BinaryOp::Or) => Ok(Type::Bool),
                    _ => Err(SomoniError::type_error(
                        line,
                        format!("operator not allowed for {}", l.name()),
                    )),
                }
            }
            Expr::Unary { op, operand } => {
                let t = self.check_expr(operand, line)?;
                match (op, t) {
                    (UnaryOp::Not, Type::Bool) => Ok(Type::Bool),
                    (UnaryOp::Not, _) => Err(SomoniError::type_error(
                        line,
                        format!("'!' needs bool, got {}", t.name()),
                    )),
                    (UnaryOp::Neg, Type::Int | Type::Float) => Ok(t),
                    (UnaryOp::Neg, _) => Err(SomoniError::type_error(
                        line,
                        format!("'-' needs int or float, got {}", t.name()),
                    )),
                }
            }
        }
    }
}
