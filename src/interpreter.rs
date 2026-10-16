use std::fmt;
use std::rc::Rc;

use crate::ast::{self, BinaryOp, CallTarget, Expr, ExprKind, Function, Stmt, UnaryOp};
use crate::error::{ErrorCode, SomoniError};
use crate::text::Text;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    // Rc: reading or passing a string only counts one more user instead of copying
    // the text. Text (not str) so a string with only one user can still grow.
    Str(Rc<Text>),
    // Rc like Str: a copy of a list shares the elements until one of them is changed
    List(Rc<Vec<Value>>),
    Void,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(x) => write!(f, "{}", x),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Str(s) => write!(f, "{}", s),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    // strings in a list get quotes: ["1"] and [1] must look different
                    match item {
                        Value::Str(s) => write!(f, "\"{}\"", s)?,
                        other => write!(f, "{}", other)?,
                    }
                }
                write!(f, "]")
            }
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

// All frames lie one after another on one stack (like a real VM). The current frame
// starts at `base`; slot i of a variable is stack[base + i]. A call needs no own memory.
pub struct Interpreter<'a> {
    stack: Vec<Value>,
    base: usize,
    // indexed like ast::functions, the type checker wrote the index into every call
    functions: Vec<&'a Function>,
    call_depth: usize,
}

impl<'a> Interpreter<'a> {
    pub fn new() -> Interpreter<'a> {
        Interpreter {
            stack: Vec::new(),
            base: 0,
            functions: Vec::new(),
            call_depth: 0,
        }
    }

    pub fn run(&mut self, stmts: &'a [Stmt]) -> Result<(), SomoniError> {
        self.functions = ast::functions(stmts).collect();
        self.exec_all(stmts)?;
        Ok(())
    }

    fn call_function(&mut self, function: &Function, args: &[Expr]) -> Result<Value, SomoniError> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(ErrorCode::RecursionTooDeep {
                limit: MAX_CALL_DEPTH,
            }
            .into());
        }
        // The arguments go on top of the stack and become slots 0, 1, ... of the new frame.
        // They are evaluated first, while the caller's frame is still the current one.
        let new_base = self.stack.len();
        for arg in args {
            match self.eval(arg) {
                Ok(value) => self.stack.push(value),
                Err(error) => {
                    self.stack.truncate(new_base);
                    return Err(error);
                }
            }
        }
        let caller_base = std::mem::replace(&mut self.base, new_base);
        self.call_depth += 1;
        let result = self.exec_all(&function.body);
        self.call_depth -= 1;
        self.base = caller_base;
        // the frame of the called function is gone
        self.stack.truncate(new_base);
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
            Stmt::VarDecl { value, slot, .. } => {
                let v = self.eval(value)?;
                self.set(*slot, v)?;
                Ok(ControlFlow::Normal)
            }
            Stmt::Assign { value, slot, .. } => {
                self.assign(*slot, value)?;
                Ok(ControlFlow::Normal)
            }
            Stmt::Expr { expr, .. } => {
                self.eval(expr)?;
                Ok(ControlFlow::Normal)
            }
            // scopes are already resolved into slots, a block needs nothing at runtime
            Stmt::Block(stmts) => self.exec_all(stmts),
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
                self.run_for(init, condition, step, body)
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

    fn assign(&mut self, slot: Option<usize>, value: &Expr) -> Result<(), SomoniError> {
        if let Some(slot) = slot
            && let Some(text) = appended_text(slot, value)
            && let Some(Value::Str(_)) = self.stack.get(self.base + slot)
        {
            let Value::Str(text) = self.eval(text)? else {
                return Err(SomoniError::internal("string + non-string"));
            };
            if let Some(Value::Str(target)) = self.stack.get_mut(self.base + slot) {
                // copy-on-write: grows in place if only this variable uses the text
                Rc::make_mut(target).push(&text);
                return Ok(());
            }
        }
        let v = self.eval(value)?;
        self.set(slot, v)
    }

    // the value of a variable; the type checker filled in every slot
    fn get(&self, slot: Option<usize>) -> Result<Value, SomoniError> {
        slot.and_then(|slot| self.stack.get(self.base + slot))
            .cloned()
            .ok_or_else(|| SomoniError::internal("variable without a value"))
    }

    // the frame grows when a variable gets a slot that is not there yet
    fn set(&mut self, slot: Option<usize>, value: Value) -> Result<(), SomoniError> {
        let Some(slot) = slot else {
            return Err(SomoniError::internal("variable without a slot"));
        };
        let index = self.base + slot;
        if index >= self.stack.len() {
            self.stack.resize(index + 1, Value::Void);
        }
        self.stack[index] = value;
        Ok(())
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
            ExprKind::Str(s) => Ok(Value::Str(Rc::clone(s))),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Ident { slot, .. } => self.get(*slot),
            ExprKind::Call { target, args, .. } => self.eval_call(*target, args),
            ExprKind::List(elements) => {
                let mut items = Vec::with_capacity(elements.len());
                for element in elements {
                    items.push(self.eval(element)?);
                }
                Ok(Value::List(Rc::new(items)))
            }
            ExprKind::Index { target, index } => {
                let list = self.eval(target)?;
                let index = self.eval(index)?;
                let (Value::List(items), Value::Int(i)) = (list, index) else {
                    return Err(SomoniError::internal("index on a non-list"));
                };
                element_at(&items, i).cloned()
            }
            ExprKind::Unary { op, operand } => {
                let value = self.eval(operand)?;
                unary(*op, value)
            }
            ExprKind::Binary {
                left, op, right, ..
            } => self.eval_binary(*op, left, right),
        }
    }

    fn eval_call(
        &mut self,
        target: Option<CallTarget>,
        args: &[Expr],
    ) -> Result<Value, SomoniError> {
        match target {
            Some(CallTarget::Builtin(builtin)) => {
                let [arg] = args else {
                    return Err(SomoniError::internal("a built-in takes one argument"));
                };
                let value = self.eval(arg)?;
                builtin.call(value)
            }
            Some(CallTarget::Function(index)) => {
                let Some(&function) = self.functions.get(index) else {
                    return Err(SomoniError::internal("unknown function index"));
                };
                self.call_function(function, args)
            }
            None => Err(SomoniError::internal("call without target")),
        }
    }

    fn eval_binary(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<Value, SomoniError> {
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
            (Value::Int(a), Value::Int(b)) => int_arithmetic(op, a, b),
            (Value::Float(a), Value::Float(b)) => float_arithmetic(op, a, b),
            (Value::Str(a), Value::Str(b)) if op == BinaryOp::Add => {
                // takes over a's text if nothing else uses it, otherwise copies it once
                let mut joined = Rc::unwrap_or_clone(a);
                joined.push(&b);
                Ok(Value::Str(Rc::new(joined)))
            }
            (Value::Bool(_), Value::Bool(b)) if matches!(op, BinaryOp::And | BinaryOp::Or) => {
                Ok(Value::Bool(b))
            }
            _ => Err(SomoniError::internal(
                "operator not supported for these values",
            )),
        }
    }
}

fn unary(op: UnaryOp, value: Value) -> Result<Value, SomoniError> {
    match (op, value) {
        (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        (UnaryOp::Neg, Value::Int(n)) => int_or_overflow(n.checked_neg()),
        (UnaryOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
        _ => Err(SomoniError::internal(
            "operator not supported for this value",
        )),
    }
}

// + - * / % for int: overflow and division by zero are runtime errors, never silent
fn int_arithmetic(op: BinaryOp, a: i64, b: i64) -> Result<Value, SomoniError> {
    let result = match op {
        BinaryOp::Div | BinaryOp::Mod if b == 0 => return Err(ErrorCode::DivisionByZero.into()),
        BinaryOp::Add => a.checked_add(b),
        BinaryOp::Sub => a.checked_sub(b),
        BinaryOp::Mul => a.checked_mul(b),
        BinaryOp::Div => a.checked_div(b),
        BinaryOp::Mod => a.checked_rem(b),
        _ => return Err(SomoniError::internal("operator not supported for int")),
    };
    int_or_overflow(result)
}

// checked_* gives None on overflow
fn int_or_overflow(result: Option<i64>) -> Result<Value, SomoniError> {
    result
        .map(Value::Int)
        .ok_or_else(|| ErrorCode::IntegerOverflow.into())
}

fn float_arithmetic(op: BinaryOp, a: f64, b: f64) -> Result<Value, SomoniError> {
    let result = match op {
        BinaryOp::Div if b == 0.0 => return Err(ErrorCode::DivisionByZero.into()),
        BinaryOp::Add => a + b,
        BinaryOp::Sub => a - b,
        BinaryOp::Mul => a * b,
        BinaryOp::Div => a / b,
        _ => return Err(SomoniError::internal("operator not supported for float")),
    };
    Ok(Value::Float(result))
}

fn compare(op: BinaryOp, l: &Value, r: &Value) -> Option<bool> {
    match (op, l, r) {
        (BinaryOp::Eq, _, _) => Some(l == r),
        (BinaryOp::NotEq, _, _) => Some(l != r),
        (_, Value::Int(a), Value::Int(b)) => order(op, a, b),
        (_, Value::Float(a), Value::Float(b)) => order(op, a, b),
        _ => None,
    }
}

fn order<T: PartialOrd>(op: BinaryOp, a: T, b: T) -> Option<bool> {
    match op {
        BinaryOp::Less => Some(a < b),
        BinaryOp::Greater => Some(a > b),
        BinaryOp::LessEq => Some(a <= b),
        BinaryOp::GreaterEq => Some(a >= b),
        _ => None,
    }
}

// the `text` in `s = s + text`, if `value` has this form for the variable in `slot`
fn appended_text(slot: usize, value: &Expr) -> Option<&Expr> {
    let ExprKind::Binary {
        left,
        op: BinaryOp::Add,
        right,
        ..
    } = &value.kind
    else {
        return None;
    };
    let ExprKind::Ident {
        slot: Some(left_slot),
        ..
    } = left.kind
    else {
        return None;
    };
    (left_slot == slot).then_some(right)
}

// the element at `index`, or R005 if the index is outside the list (also negative)
fn element_at(items: &[Value], index: i64) -> Result<&Value, SomoniError> {
    usize::try_from(index)
        .ok()
        .and_then(|i| items.get(i))
        .ok_or_else(|| {
            ErrorCode::IndexOutOfRange {
                index,
                length: items.len(),
            }
            .into()
        })
}
