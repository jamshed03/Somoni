use std::rc::Rc;

use crate::ast::{BinaryOp, CallTarget, Expr, ExprKind, Stmt};
use crate::bytecode::{Chunk, Op, Program};
use crate::error::SomoniError;
use crate::runner::Stop;
use crate::span::Span;
use crate::value::Value;

/// Turns a checked program into bytecode.
pub fn compile(stmts: &[Stmt]) -> Result<Program, Stop> {
    let mut compiler = Compiler::default();
    for stmt in stmts {
        compiler.compile_stmt(stmt)?;
    }
    Ok(Program {
        main: compiler.chunk,
    })
}

/// Writes the instructions of one chunk.
#[derive(Default)]
struct Compiler {
    chunk: Chunk,
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
            Stmt::If { .. } => return Err(not_in_vm("if")),
            Stmt::While { .. } => return Err(not_in_vm("while")),
            Stmt::For { .. } => return Err(not_in_vm("for")),
            Stmt::Break { .. } => return Err(not_in_vm("break")),
            Stmt::Continue { .. } => return Err(not_in_vm("continue")),
            Stmt::Return { .. } => return Err(not_in_vm("return")),
        }
        Ok(())
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
                op: BinaryOp::And | BinaryOp::Or,
                ..
            } => return Err(not_in_vm("&& and ||")),
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
                let Some(CallTarget::Builtin(builtin)) = target else {
                    return Err(not_in_vm("calls of own functions"));
                };
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.chunk.push(Op::CallBuiltin(*builtin, args.len()), span);
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
