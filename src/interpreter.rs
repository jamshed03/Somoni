use std::collections::HashMap;
use std::fmt;

use crate::ast::{BinaryOp, Expr, ExprKind, Function, Stmt, UnaryOp};
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

// Return carries the value up to the call, like Break carries "leave the loop"
#[derive(Debug, Clone, PartialEq)]
enum ControlFlow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

// more nested calls are an error (R004) instead of a crash; Python allows 1000, JavaScript ~10000
const MAX_CALL_DEPTH: usize = 10_000;

pub struct Interpreter<'a> {
    scopes: Scopes<Value>,
    functions: HashMap<&'a str, &'a Function>,
    call_depth: usize,
}

impl<'a> Interpreter<'a> {
    pub fn new() -> Interpreter<'a> {
        Interpreter {
            scopes: Scopes::new(),
            functions: HashMap::new(),
            call_depth: 0,
        }
    }

    pub fn run(&mut self, stmts: &'a [Stmt]) -> Result<(), SomoniError> {
        for stmt in stmts {
            if let Stmt::FnDecl(function) = stmt {
                self.functions.insert(&function.name, function);
            }
        }
        self.exec_all(stmts)?;
        Ok(())
    }

    fn call_function(
        &mut self,
        function: &Function,
        args: Vec<Value>,
    ) -> Result<Value, SomoniError> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(ErrorCode::RecursionTooDeep {
                limit: MAX_CALL_DEPTH,
            }
            .into());
        }
        let caller_scopes = std::mem::replace(&mut self.scopes, Scopes::new());
        for (param, value) in function.params.iter().zip(args) {
            self.scopes.declare(&param.name, value);
        }
        self.call_depth += 1;
        let result = self.exec_all(&function.body);
        self.call_depth -= 1;
        self.scopes = caller_scopes;
        match result? {
            ControlFlow::Return(value) => Ok(value),
            ControlFlow::Normal => Ok(Value::Void),
            ControlFlow::Break | ControlFlow::Continue => {
                Err(SomoniError::internal("break or continue outside of a loop"))
            }
        }
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
            Stmt::FnDecl(_) => Ok(ControlFlow::Normal),
            Stmt::Return { value, .. } => {
                let value = match value {
                    Some(value) => self.eval(value)?,
                    None => Value::Void,
                };
                Ok(ControlFlow::Return(value))
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
                flow @ ControlFlow::Return(_) => return Ok(flow),
            }
            if let Some(step) = step {
                self.exec(step)?;
            }
        }
        Ok(ControlFlow::Normal)
    }

    fn eval_condition(&mut self, condition: &Expr) -> Result<bool, SomoniError> {
        match self.eval(condition)? {
            Value::Bool(b) => Ok(b),
            _ => Err(SomoniError::internal("condition is not a bool")),
        }
    }

    fn eval(&mut self, expr: &Expr) -> Result<Value, SomoniError> {
        self.eval_kind(&expr.kind)
            .map_err(|error| error.with_span_if_missing(Some(expr.span)))
    }

    fn eval_kind(&mut self, kind: &ExprKind) -> Result<Value, SomoniError> {
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
                let mut values = Vec::new();
                for arg in args {
                    values.push(self.eval(arg)?);
                }
                if let Some(builtin) = Builtin::from_name(name) {
                    let Some(value) = values.pop() else {
                        return Err(SomoniError::internal("call without argument"));
                    };
                    return builtin.call(value);
                }
                let Some(&function) = self.functions.get(name.as_str()) else {
                    return Err(SomoniError::internal(format!(
                        "unknown function '{}'",
                        name
                    )));
                };
                self.call_function(function, values)
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
