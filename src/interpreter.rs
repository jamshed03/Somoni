use std::rc::Rc;

use crate::ast::{
    self, BinaryOp, CallTarget, Expr, ExprKind, FieldInit, Function, Stmt, StructDecl,
};
use crate::error::{ErrorCode, SomoniError};
use crate::map::MapValue;
use crate::span::Span;
use crate::value::{
    self, StructValue, Value, append, key_not_found, list_position, map_key, show_key, unary,
};

/// One step from a variable to a part of it: a list index or a field.
enum Step {
    Index { index: Value, span: Span },
    Field(usize),
}

/// What happens after a statement: go on, leave a loop, next round, or leave a function.
#[derive(Debug, Clone, PartialEq)]
enum ControlFlow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

/// More nested calls than this are runtime error R004.
const MAX_CALL_DEPTH: usize = 10_000;

/// Runs a checked program.
/// All variables live on one stack; the variables of the current function start at `base`.
pub struct Interpreter<'a> {
    stack: Vec<Value>,
    base: usize,
    functions: Vec<&'a Function>,
    structs: Vec<Rc<StructDecl>>,
    call_depth: usize,
}

impl<'a> Interpreter<'a> {
    pub fn new() -> Interpreter<'a> {
        Interpreter {
            stack: Vec::new(),
            base: 0,
            functions: Vec::new(),
            structs: Vec::new(),
            call_depth: 0,
        }
    }

    /// Runs all statements of the program.
    pub fn run(&mut self, stmts: &'a [Stmt]) -> Result<(), SomoniError> {
        self.functions = ast::functions(stmts).collect();
        self.structs = ast::structs(stmts).cloned().collect();
        self.exec_all(stmts)?;
        Ok(())
    }

    /// Calls a function: puts the arguments on the stack, runs the body, returns the result.
    fn call_function(&mut self, function: &Function, args: &[Expr]) -> Result<Value, SomoniError> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(ErrorCode::RecursionTooDeep {
                limit: MAX_CALL_DEPTH,
            }
            .into());
        }
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
        self.stack.truncate(new_base);
        match result? {
            ControlFlow::Return(value) => Ok(value),
            ControlFlow::Normal => Ok(Value::Void),
            ControlFlow::Break | ControlFlow::Continue => {
                Err(SomoniError::internal("break or continue outside of a loop"))
            }
        }
    }

    /// Runs statements one after another until one of them breaks, continues or returns.
    fn exec_all(&mut self, stmts: &[Stmt]) -> Result<ControlFlow, SomoniError> {
        for stmt in stmts {
            let flow = self.exec(stmt)?;
            if flow != ControlFlow::Normal {
                return Ok(flow);
            }
        }
        Ok(ControlFlow::Normal)
    }

    /// Runs one statement. An error without a place gets the place of the statement.
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
            Stmt::Assign { target, value, .. } => {
                self.assign(target, value)?;
                Ok(ControlFlow::Normal)
            }
            Stmt::Expr { expr, .. } => {
                self.eval(expr)?;
                Ok(ControlFlow::Normal)
            }
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
            } => self.run_for(init, condition, step, body),
            Stmt::FnDecl(_) | Stmt::StructDecl(_) => Ok(ControlFlow::Normal),
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

    /// Runs `target = value`.
    /// `s = s + text` and `l = l + [x]` append in place, so the string or list is not copied.
    fn assign(&mut self, target: &Expr, value: &Expr) -> Result<(), SomoniError> {
        let ExprKind::Ident { slot, .. } = target.kind else {
            return self.assign_part(target, value);
        };
        if let Some(slot) = slot
            && let Some(rest) = appended_part(slot, value)
            && let Some(Value::Str(_) | Value::List(_)) = self.stack.get(self.base + slot)
        {
            let rest = self.eval(rest)?;
            let Some(target) = self.stack.get_mut(self.base + slot) else {
                return Err(SomoniError::internal("variable without a value"));
            };
            return append(target, rest);
        }
        let v = self.eval(value)?;
        self.set(slot, v)
    }

    /// Writes a value into a list element, a map entry or a field, e.g. `l[0].x = 5`.
    /// A list, map or struct that another variable also uses is copied first.
    /// Only the last step may add a key to a map: `m["new"] = 1`.
    fn assign_part(&mut self, target: &Expr, value: &Expr) -> Result<(), SomoniError> {
        let mut steps = Vec::new();
        let slot = self.place_steps(target, &mut steps)?;
        let value = self.eval(value)?;
        let Some(mut place) = slot.and_then(|slot| self.stack.get_mut(self.base + slot)) else {
            return Err(SomoniError::internal("variable without a value"));
        };
        let count = steps.len();
        for (i, step) in steps.into_iter().enumerate() {
            let is_last = i + 1 == count;
            place = match (step, place) {
                (
                    Step::Index {
                        index: Value::Int(index),
                        span,
                    },
                    Value::List(items),
                ) => {
                    let items = Rc::make_mut(items);
                    let position = list_position(index, items.len())
                        .map_err(|error| error.with_span_if_missing(Some(span)))?;
                    &mut items[position]
                }
                (Step::Index { index, span }, Value::Map(map)) => {
                    let key = map_key(&index)?;
                    let map = Rc::make_mut(map);
                    if is_last {
                        map.entry(key)
                    } else {
                        map.get_mut(&key)
                            .ok_or_else(|| key_not_found(&key).with_span_if_missing(Some(span)))?
                    }
                }
                (Step::Field(position), Value::Struct(fields)) => Rc::make_mut(fields)
                    .fields
                    .get_mut(position)
                    .ok_or_else(|| SomoniError::internal("unknown field"))?,
                _ => return Err(SomoniError::internal("step does not fit the value")),
            };
        }
        *place = value;
        Ok(())
    }

    /// Collects the steps from the variable to the part, e.g. `l[0].x` gives index 0, field x.
    /// Indices are evaluated from left to right. Returns the slot of the variable.
    fn place_steps(
        &mut self,
        target: &Expr,
        steps: &mut Vec<Step>,
    ) -> Result<Option<usize>, SomoniError> {
        let target_span = target.span;
        match &target.kind {
            ExprKind::Ident { slot, .. } => Ok(*slot),
            ExprKind::Index { target, index } => {
                let slot = self.place_steps(target, steps)?;
                let index = self.eval(index)?;
                steps.push(Step::Index {
                    index,
                    span: target_span,
                });
                Ok(slot)
            }
            ExprKind::Field { target, index, .. } => {
                let slot = self.place_steps(target, steps)?;
                let Some(position) = *index else {
                    return Err(SomoniError::internal("field without a place"));
                };
                steps.push(Step::Field(position));
                Ok(slot)
            }
            _ => Err(SomoniError::internal("assignment to a non-variable")),
        }
    }

    /// Reads the variable in `slot` of the current function.
    fn get(&self, slot: Option<usize>) -> Result<Value, SomoniError> {
        slot.and_then(|slot| self.stack.get(self.base + slot))
            .cloned()
            .ok_or_else(|| SomoniError::internal("variable without a value"))
    }

    /// Writes the variable in `slot` of the current function; the stack grows if needed.
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

    /// Runs `for (init; condition; step) body`.
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

    /// Runs `body` while `condition` is true; `step` (in a for loop) runs after every round.
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

    /// Computes the value of an expression. An error without a place gets the place of it.
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
            ExprKind::Null => Ok(Value::Null),
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
                let container = self.eval(target)?;
                let index = self.eval(index)?;
                value::index(&container, &index)
            }
            ExprKind::Map(entries) => {
                let mut map = MapValue::default();
                for (key_expr, value) in entries {
                    let key = map_key(&self.eval(key_expr)?)?;
                    let value = self.eval(value)?;
                    if map.contains(&key) {
                        let code = ErrorCode::DuplicateKey {
                            key: show_key(&key),
                        };
                        return Err(SomoniError::at(key_expr.span, code));
                    }
                    map.insert(key, value);
                }
                Ok(Value::Map(Rc::new(map)))
            }
            ExprKind::StructLiteral { fields, index, .. } => {
                self.eval_struct_literal(*index, fields)
            }
            ExprKind::Field { target, index, .. } => {
                let Value::Struct(value) = self.eval(target)? else {
                    return Err(SomoniError::internal("field of a non-struct"));
                };
                index
                    .and_then(|i| value.fields.get(i))
                    .cloned()
                    .ok_or_else(|| SomoniError::internal("unknown field"))
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

    /// Builds a struct value, e.g. `Point { y: 2, x: 1 }`.
    /// The fields are evaluated in the order of the literal and stored in declaration order.
    fn eval_struct_literal(
        &mut self,
        index: Option<usize>,
        fields: &[FieldInit],
    ) -> Result<Value, SomoniError> {
        let Some(decl) = index.and_then(|i| self.structs.get(i)).cloned() else {
            return Err(SomoniError::internal("struct literal without struct"));
        };
        let mut values = vec![Value::Void; decl.fields.len()];
        for init in fields {
            let value = self.eval(&init.value)?;
            let Some(place) = init.index.and_then(|i| values.get_mut(i)) else {
                return Err(SomoniError::internal("field without a place"));
            };
            *place = value;
        }
        Ok(Value::Struct(Rc::new(StructValue {
            decl,
            fields: values,
        })))
    }

    /// Calls a built-in function or a function of the program.
    fn eval_call(
        &mut self,
        target: Option<CallTarget>,
        args: &[Expr],
    ) -> Result<Value, SomoniError> {
        match target {
            Some(CallTarget::Builtin(builtin)) => match args {
                [a] => {
                    let a = self.eval(a)?;
                    builtin.call(&[a])
                }
                [a, b] => {
                    let a = self.eval(a)?;
                    let b = self.eval(b)?;
                    builtin.call(&[a, b])
                }
                _ => Err(SomoniError::internal(
                    "a built-in takes one or two arguments",
                )),
            },
            Some(CallTarget::Function(index)) => {
                let Some(&function) = self.functions.get(index) else {
                    return Err(SomoniError::internal("unknown function index"));
                };
                self.call_function(function, args)
            }
            None => Err(SomoniError::internal("call without target")),
        }
    }

    /// Computes `left op right`. `&&` and `||` skip the right side when the left decides.
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
        value::binary(op, l, r)
    }
}

/// Returns `rest` if `value` is `x + rest` and `x` is the variable in `slot`.
fn appended_part(slot: usize, value: &Expr) -> Option<&Expr> {
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
