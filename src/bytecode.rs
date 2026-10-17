use std::fmt::Write;

use crate::ast::{BinaryOp, UnaryOp};
use crate::builtins::Builtin;
use crate::span::Span;
use crate::value::{Quoted, Value};

/// One instruction of the VM. Most of them take their operands from the stack
/// and push their result back onto it.
#[derive(Debug, Clone)]
pub enum Op {
    /// Pushes constant number `i` of the chunk.
    Push(usize),
    /// Pushes the variable in this slot.
    GetLocal(usize),
    /// Pops a value and stores it in the variable in this slot.
    SetLocal(usize),
    /// Pops a value and forgets it.
    Pop,
    /// Pops the right and the left operand, pushes the result.
    Binary(BinaryOp),
    /// Pops the operand, pushes the result.
    Unary(UnaryOp),
    /// Pops the arguments, calls the built-in function, pushes the result.
    CallBuiltin(Builtin, usize),
    /// Goes on at this instruction.
    Jump(usize),
    /// Pops a bool; if it is false, goes on at this instruction.
    JumpIfFalse(usize),
}

/// A list of instructions with the constants they use.
/// `spans[i]` is the place in the source of `code[i]`, for error messages.
#[derive(Debug, Default)]
pub struct Chunk {
    pub code: Vec<Op>,
    pub spans: Vec<Span>,
    pub constants: Vec<Value>,
    /// How many variable slots the code needs.
    pub locals: usize,
}

impl Chunk {
    /// Adds an instruction and returns its position.
    pub fn push(&mut self, op: Op, span: Span) -> usize {
        self.code.push(op);
        self.spans.push(span);
        self.code.len() - 1
    }

    /// Adds a constant and returns its number.
    pub fn constant(&mut self, value: Value) -> usize {
        self.constants.push(value);
        self.constants.len() - 1
    }
}

/// A compiled program.
#[derive(Debug, Default)]
pub struct Program {
    pub main: Chunk,
}

/// Returns the instructions of a program as text, one per line, for `--bytecode`.
pub fn disassemble(program: &Program) -> String {
    let mut out = String::new();
    let chunk = &program.main;
    let _ = writeln!(out, "== main ({} locals) ==", chunk.locals);
    let mut last_line = 0;
    for (i, (op, span)) in chunk.code.iter().zip(&chunk.spans).enumerate() {
        let line = if span.line == last_line {
            "    ".to_string()
        } else {
            format!("{:>4}", span.line)
        };
        last_line = span.line;
        let _ = writeln!(out, "{:04}  {}  {}", i, line, show_op(op, chunk));
    }
    out
}

/// Shows one instruction, e.g. `Push 5` or `Binary Add`.
fn show_op(op: &Op, chunk: &Chunk) -> String {
    match op {
        Op::Push(i) => format!("Push {}", Quoted(&chunk.constants[*i])),
        Op::GetLocal(slot) => format!("GetLocal {}", slot),
        Op::SetLocal(slot) => format!("SetLocal {}", slot),
        Op::Pop => "Pop".to_string(),
        Op::Binary(op) => format!("Binary {:?}", op),
        Op::Unary(op) => format!("Unary {:?}", op),
        Op::CallBuiltin(builtin, args) => format!("CallBuiltin {} {}", builtin.name(), args),
        Op::Jump(target) => format!("Jump {:04}", target),
        Op::JumpIfFalse(target) => format!("JumpIfFalse {:04}", target),
    }
}
