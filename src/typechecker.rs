use crate::ast::{BinaryOp, Expr, Stmt, UnaryOp};
use crate::scope::Scopes;
use crate::types::Type;

struct VarInfo {
    ty: Type,
    mutable: bool,
}

pub struct TypeChecker {
    scopes: Scopes<VarInfo>,
}

impl TypeChecker {
    pub fn new() -> TypeChecker {
        TypeChecker {
            scopes: Scopes::new(),
        }
    }

    pub fn check(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        for stmt in stmts {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::VarDecl {
                name,
                type_name,
                value,
                mutable,
                line,
            } => {
                let declared = Type::from_name(type_name)
                    .ok_or(format!("Line {}: unknown type '{}'", line, type_name))?;
                let actual = self.check_expr(value, *line)?;
                if declared != actual {
                    return Err(format!(
                        "Line {}: cannot assign {} to {}",
                        line,
                        actual.name(),
                        declared.name()
                    ));
                }
                let info = VarInfo {
                    ty: declared,
                    mutable: *mutable,
                };
                if !self.scopes.declare(name, info) {
                    return Err(format!(
                        "Line {}: '{}' is already declared in this block",
                        line, name
                    ));
                }
                Ok(())
            }
            Stmt::Print { value, line } => {
                self.check_expr(value, *line)?;
                Ok(())
            }
            Stmt::Assign { name, value, line } => {
                let info = self
                    .scopes
                    .lookup(name)
                    .ok_or(format!("Line {}: unknown variable '{}'", line, name))?;
                if !info.mutable {
                    return Err(format!("Line {}: cannot assign to const '{}'", line, name));
                }
                let expected = info.ty;
                let actual = self.check_expr(value, *line)?;
                if expected != actual {
                    return Err(format!(
                        "Line {}: cannot assign {} to {}",
                        line,
                        actual.name(),
                        expected.name()
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
        }
    }

    fn check_expr(&self, expr: &Expr, line: usize) -> Result<Type, String> {
        match expr {
            Expr::Int(_) => Ok(Type::Int),
            Expr::Float(_) => Ok(Type::Float),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::Str(_) => Ok(Type::Str),
            Expr::Ident(name) => self
                .scopes
                .lookup(name)
                .map(|info| info.ty)
                .ok_or(format!("Line {}: unknown variable '{}'", line, name)),
            Expr::Binary { left, op, right } => {
                let l = self.check_expr(left, line)?;
                let r = self.check_expr(right, line)?;
                if l != r {
                    return Err(format!(
                        "Line {}: cannot use operator on {} and {}",
                        line,
                        l.name(),
                        r.name()
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
                    _ => Err(format!(
                        "Line {}: operator not allowed for {}",
                        line,
                        l.name()
                    )),
                }
            }
            Expr::Unary { op, operand } => {
                let t = self.check_expr(operand, line)?;
                match (op, t) {
                    (UnaryOp::Not, Type::Bool) => Ok(Type::Bool),
                    (UnaryOp::Not, _) => {
                        Err(format!("Line {}: '!' needs bool, got {}", line, t.name()))
                    }
                    (UnaryOp::Neg, Type::Int | Type::Float) => Ok(t),
                    (UnaryOp::Neg, _) => Err(format!(
                        "Line {}: '-' needs int or float, got {}",
                        line,
                        t.name()
                    )),
                }
            }
        }
    }
}
