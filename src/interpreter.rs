use std::fmt;

use crate::ast::{BinaryOp, Expr, ExprKind, Stmt, UnaryOp};
use crate::builtins::Builtin;
use crate::error::{ErrorCode, SomoniError};
use crate::scope::Scopes;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Void,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(x) => write!(f, "{}", x),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Str(s) => write!(f, "{}", s),
            Value::Void => write!(f, "void"),
        }
    }
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

    pub fn run(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        self.exec_all(stmts)?;
        Ok(())
    }

    fn exec_all(&mut self, stmts: &[Stmt]) -> Result<ControlFlow, SomoniError> {
        for stmt in stmts {
            let flow = self.exec(stmt)?;
            if flow != ControlFlow::Normal {
                return Ok(flow);
            }
        }
        Ok(ControlFlow::Normal)
    }

    fn exec(&mut self, stmt: &Stmt) -> Result<ControlFlow, SomoniError> {
        self.exec_stmt(stmt)
            .map_err(|error| error.with_span_if_missing(stmt.span()))
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> Result<ControlFlow, SomoniError> {
        match stmt {
            Stmt::VarDecl { name, value, .. } => {
                let v = self.eval(value)?;
                self.scopes.declare(name, v);
                Ok(ControlFlow::Normal)
            }
            Stmt::Expr { expr, .. } => {
                self.eval(expr)?;
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
                    None => Err(SomoniError::internal(format!(
                        "unknown variable '{}'",
                        name
                    ))),
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
    ) -> Result<ControlFlow, SomoniError> {
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
    ) -> Result<ControlFlow, SomoniError> {
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
    fn eval_condition(&self, condition: &Expr) -> Result<bool, SomoniError> {
        match self.eval(condition)? {
            Value::Bool(b) => Ok(b),
            _ => Err(SomoniError::internal("condition is not a bool")),
        }
    }

    // a runtime error without a place gets the place of this expression; the innermost wins
    fn eval(&self, expr: &Expr) -> Result<Value, SomoniError> {
        self.eval_kind(&expr.kind)
            .map_err(|error| error.with_span_if_missing(Some(expr.span)))
    }

    fn eval_kind(&self, kind: &ExprKind) -> Result<Value, SomoniError> {
        match kind {
            ExprKind::Int(n) => Ok(Value::Int(*n)),
            ExprKind::Float(f) => Ok(Value::Float(*f)),
            ExprKind::Str(s) => Ok(Value::Str(s.clone())),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Ident(name) => match self.scopes.lookup(name) {
                Some(value) => Ok(value.clone()),
                None => Err(SomoniError::internal(format!(
                    "unknown variable '{}'",
                    name
                ))),
            },
            ExprKind::Call { name, args, .. } => {
                let builtin = Builtin::from_name(name)
                    .ok_or_else(|| SomoniError::internal(format!("unknown function '{}'", name)))?;
                let Some(arg) = args.first() else {
                    return Err(SomoniError::internal("call without argument"));
                };
                let value = self.eval(arg)?;
                builtin.call(value)
            }
            ExprKind::Unary { op, operand } => match (op, self.eval(operand)?) {
                (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                (UnaryOp::Neg, Value::Int(n)) => n
                    .checked_neg()
                    .map(Value::Int)
                    .ok_or(ErrorCode::IntegerOverflow.into()),
                (UnaryOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
                _ => Err(SomoniError::internal(
                    "operator not supported for this type",
                )),
            },
            ExprKind::Binary {
                left, op, right, ..
            } => {
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
                            .ok_or(ErrorCode::IntegerOverflow.into()),
                        BinaryOp::Sub => a
                            .checked_sub(b)
                            .map(Value::Int)
                            .ok_or(ErrorCode::IntegerOverflow.into()),
                        BinaryOp::Mul => a
                            .checked_mul(b)
                            .map(Value::Int)
                            .ok_or(ErrorCode::IntegerOverflow.into()),
                        BinaryOp::Div => {
                            if b == 0 {
                                Err(ErrorCode::DivisionByZero.into())
                            } else {
                                a.checked_div(b)
                                    .map(Value::Int)
                                    .ok_or(ErrorCode::IntegerOverflow.into())
                            }
                        }
                        BinaryOp::Mod => {
                            if b == 0 {
                                Err(ErrorCode::DivisionByZero.into())
                            } else {
                                a.checked_rem(b)
                                    .map(Value::Int)
                                    .ok_or(ErrorCode::IntegerOverflow.into())
                            }
                        }
                        _ => Err(SomoniError::internal("operator not supported for integers")),
                    },
                    (Value::Str(a), Value::Str(b)) => match op {
                        BinaryOp::Add => Ok(Value::Str(a + &b)),
                        _ => Err(SomoniError::internal("operator not supported for strings")),
                    },
                    (Value::Float(a), Value::Float(b)) => match op {
                        BinaryOp::Add => Ok(Value::Float(a + b)),
                        BinaryOp::Sub => Ok(Value::Float(a - b)),
                        BinaryOp::Mul => Ok(Value::Float(a * b)),
                        BinaryOp::Div => {
                            if b == 0.0 {
                                Err(ErrorCode::DivisionByZero.into())
                            } else {
                                Ok(Value::Float(a / b))
                            }
                        }
                        _ => Err(SomoniError::internal("operator not supported for floats")),
                    },
                    (Value::Bool(_), Value::Bool(b)) => match op {
                        BinaryOp::And | BinaryOp::Or => Ok(Value::Bool(b)),
                        _ => Err(SomoniError::internal("operator not supported for bool")),
                    },
                    _ => Err(SomoniError::internal(
                        "operator not supported for these types",
                    )),
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
