use crate::ast::{BinaryOp, Expr, Stmt, UnaryOp};
use crate::scope::Scopes;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ControlFlow {
    Normal,
    Break,
    Continue,
}

pub struct Interpreter {
    scopes: Scopes<Value>,
}

impl Interpreter {
    pub fn new() -> Interpreter {
        Interpreter {
            scopes: Scopes::new(),
        }
    }

    pub fn run(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        self.exec_all(stmts)?;
        Ok(())
    }

    fn exec_all(&mut self, stmts: &[Stmt]) -> Result<ControlFlow, String> {
        for stmt in stmts {
            let flow = self.exec(stmt)?;
            if flow != ControlFlow::Normal {
                return Ok(flow);
            }
        }
        Ok(ControlFlow::Normal)
    }

    fn exec(&mut self, stmt: &Stmt) -> Result<ControlFlow, String> {
        match stmt {
            Stmt::VarDecl { name, value, .. } => {
                let v = self.eval(value)?;
                // duplicates were already rejected by the type checker
                self.scopes.declare(name, v);
                Ok(ControlFlow::Normal)
            }
            Stmt::Print { value, .. } => {
                let v = self.eval(value)?;
                match v {
                    Value::Int(n) => println!("{}", n),
                    Value::Float(f) => println!("{}", f),
                    Value::Str(s) => println!("{}", s),
                    Value::Bool(b) => println!("{}", b),
                }
                Ok(ControlFlow::Normal)
            }
            Stmt::Assign { name, value, .. } => {
                let v = self.eval(value)?;
                // changes the variable in the block where it was declared
                match self.scopes.lookup_mut(name) {
                    Some(slot) => {
                        *slot = v;
                        Ok(ControlFlow::Normal)
                    }
                    None => Err(format!("Unknown variable '{}'", name)),
                }
            }
            Stmt::Block(stmts) => {
                self.scopes.push();
                let result = self.exec_all(stmts);
                self.scopes.pop();
                result
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                if self.eval_condition(condition)? {
                    self.exec(then_branch)
                } else if let Some(else_branch) = else_branch {
                    self.exec(else_branch)
                } else {
                    Ok(ControlFlow::Normal)
                }
            }
            Stmt::While {
                condition, body, ..
            } => self.run_loop(condition, body, None),
            Stmt::For {
                init,
                condition,
                step,
                body,
                ..
            } => {
                // the counter lives in its own scope around the whole loop, gone afterwards
                self.scopes.push();
                let result = self.run_for(init, condition, step, body);
                self.scopes.pop();
                result
            }
            Stmt::Break { .. } => Ok(ControlFlow::Break),
            Stmt::Continue { .. } => Ok(ControlFlow::Continue),
        }
    }

    fn run_for(
        &mut self,
        init: &Stmt,
        condition: &Expr,
        step: &Stmt,
        body: &Stmt,
    ) -> Result<ControlFlow, String> {
        self.exec(init)?;
        self.run_loop(condition, body, Some(step))
    }

    // Shared by while and for. The condition is evaluated again before every round.
    // `step` (only for `for`) runs after every round, also after `continue`.
    fn run_loop(
        &mut self,
        condition: &Expr,
        body: &Stmt,
        step: Option<&Stmt>,
    ) -> Result<ControlFlow, String> {
        while self.eval_condition(condition)? {
            match self.exec(body)? {
                ControlFlow::Break => break,
                ControlFlow::Continue | ControlFlow::Normal => {}
            }
            if let Some(step) = step {
                self.exec(step)?;
            }
        }
        Ok(ControlFlow::Normal)
    }

    // the type checker made sure it is a bool
    fn eval_condition(&self, condition: &Expr) -> Result<bool, String> {
        match self.eval(condition)? {
            Value::Bool(b) => Ok(b),
            _ => Err("Condition is not a bool".to_string()),
        }
    }

    fn eval(&self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Ident(name) => match self.scopes.lookup(name) {
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
                if let Some(result) = compare(op, &l, &r) {
                    return Ok(Value::Bool(result));
                }

                match (l, r) {
                    (Value::Int(a), Value::Int(b)) => match op {
                        BinaryOp::Add => a
                            .checked_add(b)
                            .map(Value::Int)
                            .ok_or("Integer overflow".to_string()),
                        BinaryOp::Sub => a
                            .checked_sub(b)
                            .map(Value::Int)
                            .ok_or("Integer overflow".to_string()),
                        BinaryOp::Mul => a
                            .checked_mul(b)
                            .map(Value::Int)
                            .ok_or("Integer overflow".to_string()),
                        BinaryOp::Div => {
                            if b == 0 {
                                Err("Division by zero".to_string())
                            } else {
                                a.checked_div(b)
                                    .map(Value::Int)
                                    .ok_or("Integer overflow".to_string())
                            }
                        }
                        BinaryOp::Mod => {
                            if b == 0 {
                                Err("Division by zero".to_string())
                            } else {
                                a.checked_rem(b)
                                    .map(Value::Int)
                                    .ok_or("Integer overflow".to_string())
                            }
                        }
                        _ => Err("Operator not supported for integers".to_string()),
                    },
                    (Value::Str(a), Value::Str(b)) => match op {
                        BinaryOp::Add => Ok(Value::Str(a + &b)),
                        _ => Err("Operator not supported for strings".to_string()),
                    },
                    (Value::Float(a), Value::Float(b)) => match op {
                        BinaryOp::Add => Ok(Value::Float(a + b)),
                        BinaryOp::Sub => Ok(Value::Float(a - b)),
                        BinaryOp::Mul => Ok(Value::Float(a * b)),
                        BinaryOp::Div => Ok(Value::Float(a / b)),
                        _ => Err("Operator not supported for floats".to_string()),
                    },
                    (Value::Bool(_), Value::Bool(b)) => match op {
                        BinaryOp::And | BinaryOp::Or => Ok(Value::Bool(b)),
                        _ => Err("Operator not supported for bool".to_string()),
                    },
                    _ => Err("Operator not supported for these types".to_string()),
                }
            }
        }
    }
}

fn compare(op: &BinaryOp, l: &Value, r: &Value) -> Option<bool> {
    match (op, l, r) {
        (BinaryOp::Eq, _, _) => Some(l == r),
        (BinaryOp::NotEq, _, _) => Some(l != r),
        (_, Value::Int(a), Value::Int(b)) => order(op, a, b),
        (_, Value::Float(a), Value::Float(b)) => order(op, a, b),
        _ => None,
    }
}

fn order<T: PartialOrd>(op: &BinaryOp, a: T, b: T) -> Option<bool> {
    match op {
        BinaryOp::Less => Some(a < b),
        BinaryOp::Greater => Some(a > b),
        BinaryOp::LessEq => Some(a <= b),
        BinaryOp::GreaterEq => Some(a >= b),
        _ => None,
    }
}
