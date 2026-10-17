use crate::ast::Stmt;
use crate::bytecode::{Chunk, Op, Program};
use crate::compiler;
use crate::error::SomoniError;
use crate::runner::{Runner, Stop};
use crate::value::{self, Value};

/// Runs a program by compiling it to bytecode first.
pub struct VmRunner;

impl<'a> Runner<'a> for VmRunner {
    fn run(&mut self, stmts: &'a [Stmt]) -> Result<(), Stop> {
        let program = compiler::compile(stmts)?;
        Vm::default().run(&program)?;
        Ok(())
    }
}

/// The virtual machine: a loop over the instructions with a stack for the values
/// they work on, and the variables of the program in `locals`.
#[derive(Default)]
struct Vm {
    stack: Vec<Value>,
    locals: Vec<Value>,
}

impl Vm {
    /// Runs the main chunk of a program.
    fn run(&mut self, program: &Program) -> Result<(), SomoniError> {
        let chunk = &program.main;
        self.locals.resize(chunk.locals, Value::Void);
        let mut ip = 0;
        while let Some(op) = chunk.code.get(ip) {
            ip += 1;
            self.execute(op, chunk)
                .map_err(|error| error.with_span_if_missing(Some(chunk.spans[ip - 1])))?;
        }
        Ok(())
    }

    /// Runs one instruction.
    fn execute(&mut self, op: &Op, chunk: &Chunk) -> Result<(), SomoniError> {
        match op {
            Op::Push(i) => self.stack.push(chunk.constants[*i].clone()),
            Op::GetLocal(slot) => self.stack.push(self.locals[*slot].clone()),
            Op::SetLocal(slot) => self.locals[*slot] = self.pop()?,
            Op::Pop => {
                self.pop()?;
            }
            Op::Binary(op) => {
                let right = self.pop()?;
                let left = self.pop()?;
                self.stack.push(value::binary(*op, left, right)?);
            }
            Op::Unary(op) => {
                let operand = self.pop()?;
                self.stack.push(value::unary(*op, operand)?);
            }
            Op::CallBuiltin(builtin, count) => {
                let start = self.stack.len() - count;
                let result = builtin.call(&self.stack[start..])?;
                self.stack.truncate(start);
                self.stack.push(result);
            }
        }
        Ok(())
    }

    /// Takes the top value from the stack.
    fn pop(&mut self) -> Result<Value, SomoniError> {
        self.stack
            .pop()
            .ok_or_else(|| SomoniError::internal("the stack is empty"))
    }
}
