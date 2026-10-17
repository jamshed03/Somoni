use std::rc::Rc;

use crate::ast::{self, BinaryOp, CallTarget, Expr, ExprKind, Function, Stmt};
use crate::bytecode::{Chunk, Op, Program};
use crate::error::SomoniError;
use crate::runner::Stop;
use crate::span::Span;
use crate::value::Value;

/// Turns a checked program into bytecode.
pub fn compile(stmts: &[Stmt]) -> Result<Program, Stop> {
    let mut compiler = Compiler::default();
    compiler.chunk.name = "program".to_string();
    for stmt in stmts {
        compiler.compile_stmt(stmt)?;
    }
    let mut functions = Vec::new();
    for function in ast::functions(stmts) {
        functions.push(compile_function(function)?);
    }
    Ok(Program {
        main: compiler.chunk,
        functions,
    })
}

/// Turns one function into its own chunk. The parameters are its first variables.
/// A function that reaches its end without `return` returns `void`.
fn compile_function(function: &Function) -> Result<Chunk, Stop> {
    let mut compiler = Compiler::default();
    compiler.chunk.name = format!("fn {}", function.name);
    compiler.chunk.locals = function.params.len();
    for stmt in &function.body {
        compiler.compile_stmt(stmt)?;
    }
    compiler.push_constant(Value::Void, function.name_span);
    compiler.chunk.push(Op::Return, function.name_span);
    Ok(compiler.chunk)
}

/// Writes the instructions of one chunk.
#[derive(Default)]
struct Compiler {
    chunk: Chunk,
    loops: Vec<LoopJumps>,
}

/// The jumps of `break` and `continue` in one loop. Their targets are filled in
/// when the loop is finished, because the end is not known before.
#[derive(Default)]
struct LoopJumps {
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

impl Compiler {
    /// Writes the instructions of a statement. Afterwards the stack is as before.
    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), Stop> {
        match stmt {
            Stmt::VarDecl {
                value, slot, span, ..
            } => {
                self.compile_expr(value)?;
                let slot = self.slot(*slot)?;
                self.chunk.push(Op::SetLocal(slot), *span);
            }
            Stmt::Assign {
                target,
                value,
                span,
            } => {
                let ExprKind::Ident { slot, .. } = target.kind else {
                    return Err(not_in_vm(
                        "assignment to a list element, map entry or field",
                    ));
                };
                self.compile_expr(value)?;
                let slot = self.slot(slot)?;
                self.chunk.push(Op::SetLocal(slot), *span);
            }
            Stmt::Expr { expr, span } => {
                self.compile_expr(expr)?;
                self.chunk.push(Op::Pop, *span);
            }
            Stmt::Block(stmts) => {
                for stmt in stmts {
                    self.compile_stmt(stmt)?;
                }
            }
            Stmt::FnDecl(_) | Stmt::StructDecl(_) => {}
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                self.compile_expr(condition)?;
                let to_else = self.chunk.push(Op::JumpIfFalse(0), condition.span);
                self.compile_stmt(then_branch)?;
                match else_branch {
                    Some(else_branch) => {
                        let to_end = self.chunk.push(Op::Jump(0), *span);
                        self.patch_jump(to_else);
                        self.compile_stmt(else_branch)?;
                        self.patch_jump(to_end);
                    }
                    None => self.patch_jump(to_else),
                }
            }
            Stmt::While {
                condition, body, ..
            } => self.compile_loop(condition, body, None)?,
            Stmt::For {
                init,
                condition,
                step,
                body,
                ..
            } => {
                self.compile_stmt(init)?;
                self.compile_loop(condition, body, Some(step))?;
            }
            Stmt::Break { span } => {
                let jump = self.chunk.push(Op::Jump(0), *span);
                self.current_loop()?.breaks.push(jump);
            }
            Stmt::Continue { span } => {
                let jump = self.chunk.push(Op::Jump(0), *span);
                self.current_loop()?.continues.push(jump);
            }
            Stmt::Return { value, span } => {
                match value {
                    Some(value) => self.compile_expr(value)?,
                    None => self.push_constant(Value::Void, *span),
                }
                self.chunk.push(Op::Return, *span);
            }
        }
        Ok(())
    }

    /// Writes a `while` loop, or the part of a `for` loop after its start.
    /// `continue` goes to the step (in a for loop) or straight back to the condition.
    fn compile_loop(
        &mut self,
        condition: &Expr,
        body: &Stmt,
        step: Option<&Stmt>,
    ) -> Result<(), Stop> {
        let start = self.chunk.code.len();
        self.compile_expr(condition)?;
        let to_end = self.chunk.push(Op::JumpIfFalse(0), condition.span);
        self.loops.push(LoopJumps::default());
        self.compile_stmt(body)?;
        let jumps = self.loops.pop().unwrap_or_default();
        let next_round = self.chunk.code.len();
        if let Some(step) = step {
            self.compile_stmt(step)?;
        }
        self.chunk.push(Op::Jump(start), condition.span);
        for jump in jumps.continues {
            self.patch_jump_to(jump, next_round);
        }
        self.patch_jump(to_end);
        for jump in jumps.breaks {
            self.patch_jump(jump);
        }
        Ok(())
    }

    /// Returns the jumps of the innermost loop.
    fn current_loop(&mut self) -> Result<&mut LoopJumps, Stop> {
        self.loops
            .last_mut()
            .ok_or_else(|| SomoniError::internal("break or continue outside of a loop").into())
    }

    /// Lets the jump at position `at` go to the next instruction that will be written.
    fn patch_jump(&mut self, at: usize) {
        self.patch_jump_to(at, self.chunk.code.len());
    }

    /// Lets the jump at position `at` go to `target`.
    fn patch_jump_to(&mut self, at: usize, target: usize) {
        if let Some(Op::Jump(to) | Op::JumpIfFalse(to)) = self.chunk.code.get_mut(at) {
            *to = target;
        }
    }

    /// Writes the instructions of an expression. Afterwards its value is on top of the stack.
    fn compile_expr(&mut self, expr: &Expr) -> Result<(), Stop> {
        let span = expr.span;
        match &expr.kind {
            ExprKind::Int(n) => self.push_constant(Value::Int(*n), span),
            ExprKind::Float(f) => self.push_constant(Value::Float(*f), span),
            ExprKind::Bool(b) => self.push_constant(Value::Bool(*b), span),
            ExprKind::Str(s) => self.push_constant(Value::Str(Rc::clone(s)), span),
            ExprKind::Null => self.push_constant(Value::Null, span),
            ExprKind::Ident { slot, .. } => {
                let slot = self.slot(*slot)?;
                self.chunk.push(Op::GetLocal(slot), span);
            }
            ExprKind::Binary {
                left,
                op: BinaryOp::And,
                right,
                ..
            } => {
                self.compile_expr(left)?;
                let to_false = self.chunk.push(Op::JumpIfFalse(0), span);
                self.compile_expr(right)?;
                let to_end = self.chunk.push(Op::Jump(0), span);
                self.patch_jump(to_false);
                self.push_constant(Value::Bool(false), span);
                self.patch_jump(to_end);
            }
            ExprKind::Binary {
                left,
                op: BinaryOp::Or,
                right,
                ..
            } => {
                self.compile_expr(left)?;
                let to_right = self.chunk.push(Op::JumpIfFalse(0), span);
                self.push_constant(Value::Bool(true), span);
                let to_end = self.chunk.push(Op::Jump(0), span);
                self.patch_jump(to_right);
                self.compile_expr(right)?;
                self.patch_jump(to_end);
            }
            ExprKind::Binary {
                left, op, right, ..
            } => {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.chunk.push(Op::Binary(*op), span);
            }
            ExprKind::Unary { op, operand } => {
                self.compile_expr(operand)?;
                self.chunk.push(Op::Unary(*op), span);
            }
            ExprKind::Call { args, target, .. } => {
                for arg in args {
                    self.compile_expr(arg)?;
                }
                let op = match target {
                    Some(CallTarget::Builtin(builtin)) => Op::CallBuiltin(*builtin, args.len()),
                    Some(CallTarget::Function(i)) => Op::Call(*i, args.len()),
                    None => return Err(SomoniError::internal("call without target").into()),
                };
                self.chunk.push(op, span);
            }
            ExprKind::List(_) => return Err(not_in_vm("lists")),
            ExprKind::Map(_) => return Err(not_in_vm("maps")),
            ExprKind::Index { .. } => return Err(not_in_vm("[index]")),
            ExprKind::StructLiteral { .. } => return Err(not_in_vm("structs")),
            ExprKind::Field { .. } => return Err(not_in_vm("fields")),
        }
        Ok(())
    }

    /// Adds a constant and the instruction that pushes it.
    fn push_constant(&mut self, value: Value, span: Span) {
        let i = self.chunk.constant(value);
        self.chunk.push(Op::Push(i), span);
    }

    /// Returns the slot the type checker gave a variable, and makes room for it.
    fn slot(&mut self, slot: Option<usize>) -> Result<usize, Stop> {
        let slot = slot.ok_or_else(|| SomoniError::internal("variable without a slot"))?;
        self.chunk.locals = self.chunk.locals.max(slot + 1);
        Ok(slot)
    }
}

/// The stop for a part of the language the VM cannot run yet.
fn not_in_vm(what: &str) -> Stop {
    Stop::NotInVm(what.to_string())
}
