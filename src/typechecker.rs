use std::collections::HashMap;

use crate::ast::{BinaryOp, Expr, Stmt};
use crate::types::Type;

pub struct TypeChecker {
    vars: HashMap<String, Type>,
}

impl TypeChecker {
    pub fn new() -> TypeChecker {
        TypeChecker { vars: HashMap::new() }
    }

    pub fn check(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        for stmt in stmts {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::VarDecl { name, type_name, value, line, .. } => {
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
                self.vars.insert(name.clone(), declared);
                Ok(())
            }
            Stmt::Print { value, line } => {
                self.check_expr(value, *line)?;
                Ok(())
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
                .vars
                .get(name)
                .copied()
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
                    (Type::Int | Type::Float, _) => Ok(l),
                    (Type::Str, BinaryOp::Add) => Ok(Type::Str),
                    _ => Err(format!("Line {}: operator not allowed for {}", line, l.name())),
                }
            }
        }
    }
}
