use std::collections::HashMap;

use crate::ast::{BinaryOp, Expr, Stmt, UnaryOp};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

pub struct Interpreter {
    // one map per open block; the first entry is the top level, the last one the innermost block
    scopes: Vec<HashMap<String, Value>>,
}

impl Interpreter {
    pub fn new() -> Interpreter {
        Interpreter { scopes: vec![HashMap::new()] }
    }

    fn declare(&mut self, name: &str, value: Value) {
        let scope = self.scopes.last_mut().expect("scope stack is never empty");
        scope.insert(name.to_string(), value);
    }

    // innermost block first
    fn lookup(&self, name: &str) -> Option<&Value> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    // changes the variable in the block where it was declared
    fn assign(&mut self, name: &str, value: Value) -> Result<(), String> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(slot) = scope.get_mut(name) {
                *slot = value;
                return Ok(());
            }
        }
        Err(format!("Unknown variable '{}'", name))
    }

    pub fn run(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        for stmt in stmts {
            self.exec(stmt)?;
        }
        Ok(())
    }

    fn exec(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::VarDecl { name, value, .. } => {
                let v = self.eval(value)?;
                self.declare(name, v);
                Ok(())
            }
            Stmt::Print { value, .. } => {
                let v = self.eval(value)?;
                match v {
                    Value::Int(n) => println!("{}", n),
                    Value::Float(f) => println!("{}", f),
                    Value::Str(s) => println!("{}", s),
                    Value::Bool(b) => println!("{}", b),
                }
                Ok(())
            }
            Stmt::Assign { name, value, .. } => {
                let v = self.eval(value)?;
                self.assign(name, v)
            }
            Stmt::Block(stmts) => {
                self.scopes.push(HashMap::new());
                let result = self.run(stmts);
                self.scopes.pop();
                result
            }
        }
    }

    fn eval(&self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Ident(name) => match self.lookup(name) {
                Some(value) => Ok(value.clone()),
                None => Err(format!("Unknown variable '{}'", name)),
            },
            Expr::Unary { op, operand } => match (op, self.eval(operand)?) {
                (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                (UnaryOp::Neg, Value::Int(n)) => n
                    .checked_neg()
                    .map(Value::Int)
                    .ok_or("Integer overflow".to_string()),
                (UnaryOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
                _ => Err("Operator not supported for this type".to_string()),
            },
            Expr::Binary { left, op, right } => {
                let l = self.eval(left)?;

                match (op, &l) {
                    (BinaryOp::And, Value::Bool(false)) => return Ok(Value::Bool(false)),
                    (BinaryOp::Or, Value::Bool(true)) => return Ok(Value::Bool(true)),
                    _ => {}
                }

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
                                a.checked_div(b).map(Value::Int)
                                    .ok_or("Integer overflow".to_string())
                            }
                        }
                        BinaryOp::Mod => {
                            if b == 0 {
                                Err("Division by zero".to_string())
                            } else {
                                a.checked_rem(b).map(Value::Int)
                                    .ok_or("Integer overflow".to_string())
                            }
                        }
                        BinaryOp::Eq => Ok(Value::Bool(a == b)),
                        BinaryOp::NotEq => Ok(Value::Bool(a != b)),
                        BinaryOp::Less => Ok(Value::Bool(a < b)),
                        BinaryOp::Greater => Ok(Value::Bool(a > b)),
                        BinaryOp::LessEq => Ok(Value::Bool(a <= b)),
                        BinaryOp::GreaterEq => Ok(Value::Bool(a >= b)),
                        _ => Err("Operator not supported for integers".to_string()),
                    },
                    (Value::Str(a), Value::Str(b)) => match op {
                        BinaryOp::Add => Ok(Value::Str(a + &b)),
                        BinaryOp::Eq => Ok(Value::Bool(a == b)),
                        BinaryOp::NotEq => Ok(Value::Bool(a != b)),
                        _ => Err("Operator not supported for strings".to_string()),
                    },
                    (Value::Float(a), Value::Float(b)) => match op {
                        BinaryOp::Add => Ok(Value::Float(a + b)),
                        BinaryOp::Sub => Ok(Value::Float(a - b)),
                        BinaryOp::Mul => Ok(Value::Float(a * b)),
                        BinaryOp::Div => Ok(Value::Float(a / b)),
                        BinaryOp::Eq => Ok(Value::Bool(a == b)),
                        BinaryOp::NotEq => Ok(Value::Bool(a != b)),
                        BinaryOp::Less => Ok(Value::Bool(a < b)),
                        BinaryOp::Greater => Ok(Value::Bool(a > b)),
                        BinaryOp::LessEq => Ok(Value::Bool(a <= b)),
                        BinaryOp::GreaterEq => Ok(Value::Bool(a >= b)),
                        _ => Err("Operator not supported for floats".to_string()),
                    },
                    (Value::Bool(a), Value::Bool(b)) => match op {
                        BinaryOp::And | BinaryOp::Or => Ok(Value::Bool(b)),
                        BinaryOp::Eq => Ok(Value::Bool(a == b)),
                        BinaryOp::NotEq => Ok(Value::Bool(a != b)),
                        _ => Err("Operator not supported for bool".to_string()),
                    },
                    _ => Err("Operator not supported for these types".to_string()),
                }
            }
        }
    }
}
