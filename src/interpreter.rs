use std::collections::HashMap;

use crate::ast::{BinaryOp, Expr, Stmt};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
}

struct Variable {
    value: Value,
    mutable: bool,
}

pub struct Interpreter {
    vars: HashMap<String, Variable>,
}

impl Interpreter {
    pub fn new() -> Interpreter {
        Interpreter { vars: HashMap::new() }
    }

    pub fn run(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        for stmt in stmts {
            self.exec(stmt)?;
        }
        Ok(())
    }

    fn exec(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::VarDecl { name, value, mutable, .. } => {
                let v = self.eval(value)?;
                self.vars.insert(name.clone(), Variable { value: v, mutable: *mutable });
                Ok(())
            }
            Stmt::Print { value, .. } => {
                let v = self.eval(value)?;
                match v {
                    Value::Int(n) => println!("{}", n),
                }
                Ok(())
            }
        }
    }

    fn eval(&self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Ident(name) => match self.vars.get(name) {
                Some(var) => Ok(var.value.clone()),
                None => Err(format!("Unknown variable '{}'", name)),
            },
            Expr::Binary { left, op, right } => {
                let l = self.eval(left)?;
                let r = self.eval(right)?;
                match (l, r) {
                    (Value::Int(a), Value::Int(b)) => match op {
                        BinaryOp::Add => a.checked_add(b).map(Value::Int)
                            .ok_or("Integer overflow".to_string()),
                        BinaryOp::Sub => a.checked_sub(b).map(Value::Int)
                            .ok_or("Integer overflow".to_string()),
                        BinaryOp::Mul => a.checked_mul(b).map(Value::Int)
                            .ok_or("Integer overflow".to_string()),
                        BinaryOp::Div => {
                            if b == 0 {
                                Err("Division by zero".to_string())
                            } else {
                                Ok(Value::Int(a / b))
                            }
                        }
                    },
                }
            }
        }
    }
}
