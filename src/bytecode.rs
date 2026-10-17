use std::fmt::Write;
use std::rc::Rc;

use crate::ast::{BinaryOp, StructDecl, UnaryOp};
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
    /// Calls function number `i` with `n` arguments from the stack.
    Call(usize, usize),
    /// Pops the result and goes back to the caller.
    Return,
    /// Pops `n` values and pushes a list of them.
    MakeList(usize),
    /// Pushes an empty map.
    NewMap,
    /// Pops a value and a key and adds them to the map below; R007 if the key is already there.
    MapInsert,
    /// Pushes a value of struct number `i` whose fields are not set yet.
    NewStruct(usize),
    /// Pops a value and sets field `p` of the struct below.
    InitField(usize),
    /// Pops an index (or key) and a list (or map), pushes the element.
    Index,
    /// Pops a struct, pushes its field `p`.
    Field(usize),
    /// Pops a value and appends it to the string or list in this slot (`s = s + x`).
    /// For other values (`i = i + 1`) it adds them.
    AppendLocal(usize),
    /// Pops a value and the indices of path number `i`, writes the value into that part.
    SetPart(usize),
    /// Like `SetPart`, but computes `part op value` and writes that (`l[i] += 1`).
    UpdatePart(usize, BinaryOp),
    /// Pops a string, pushes the list of its characters (for `for`-each over a string).
    Chars,
    /// One round of a `for`-each loop over the list or map in `slot`; the index is in `slot + 1`.
    /// If the index is past the end, goes on at `end`. Otherwise pushes the element (list) or
    /// key (map), with `both` the index and the element or the key and the value, and counts up.
    Next { slot: usize, end: usize, both: bool },
    /// Pops an enum value and goes on at the instruction for its variant in jump table `i`.
    Switch(usize),
}

/// The way from a variable to a part of it, e.g. `l[i].x`: the slot of `l` and the steps.
#[derive(Debug)]
pub struct Path {
    pub slot: usize,
    pub steps: Vec<PathStep>,
}

/// One step of a path. The value of an index is on the stack when the path is used.
#[derive(Debug)]
pub enum PathStep {
    Index(Span),
    Field(usize),
}

/// A list of instructions with the constants they use.
/// `spans[i]` is the place in the source of `code[i]`, for error messages.
#[derive(Debug, Default)]
pub struct Chunk {
    /// `program` or `fn name`, for `--bytecode`.
    pub name: String,
    pub code: Vec<Op>,
    pub spans: Vec<Span>,
    pub constants: Vec<Value>,
    /// The paths of `SetPart`.
    pub paths: Vec<Path>,
    /// The jump tables of `Switch`: for each variant, the instruction of its branch.
    pub tables: Vec<Vec<usize>>,
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

/// A compiled program: the code at the top level and one chunk per function,
/// numbered like `ast::functions`.
#[derive(Debug, Default)]
pub struct Program {
    pub main: Chunk,
    pub functions: Vec<Chunk>,
    /// The structs of the program, numbered like `ast::structs`.
    pub structs: Vec<Rc<StructDecl>>,
}

/// Returns the instructions of a program as text, one per line, for `--bytecode`.
pub fn disassemble(program: &Program) -> String {
    let mut out = String::new();
    for chunk in std::iter::once(&program.main).chain(&program.functions) {
        let _ = writeln!(out, "== {} ({} locals) ==", chunk.name, chunk.locals);
        let mut last_line = 0;
        for (i, (op, span)) in chunk.code.iter().zip(&chunk.spans).enumerate() {
            let line = if span.line == last_line {
                "    ".to_string()
            } else {
                format!("{:>4}", span.line)
            };
            last_line = span.line;
            let _ = writeln!(out, "{:04}  {}  {}", i, line, show_op(op, chunk, program));
        }
    }
    out
}

/// Shows a path as its slot and steps, e.g. `2[].1` for `l[i].name` with `l` in slot 2.
fn show_path(path: &Path) -> String {
    let steps: String = path
        .steps
        .iter()
        .map(|step| match step {
            PathStep::Index(_) => "[]".to_string(),
            PathStep::Field(p) => format!(".{}", p),
        })
        .collect();
    format!("{}{}", path.slot, steps)
}

/// Shows one instruction, e.g. `Push 5` or `Binary Add`.
fn show_op(op: &Op, chunk: &Chunk, program: &Program) -> String {
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
        Op::Call(i, args) => format!("Call {} {}", program.functions[*i].name, args),
        Op::Return => "Return".to_string(),
        Op::MakeList(count) => format!("MakeList {}", count),
        Op::NewMap => "NewMap".to_string(),
        Op::MapInsert => "MapInsert".to_string(),
        Op::NewStruct(i) => format!("NewStruct {}", program.structs[*i].name),
        Op::InitField(p) => format!("InitField {}", p),
        Op::Index => "Index".to_string(),
        Op::Field(p) => format!("Field {}", p),
        Op::AppendLocal(slot) => format!("AppendLocal {}", slot),
        Op::Chars => "Chars".to_string(),
        Op::Switch(i) => {
            let targets: Vec<String> = chunk.tables[*i]
                .iter()
                .map(|target| format!("{:04}", target))
                .collect();
            format!("Switch [{}]", targets.join(", "))
        }
        Op::Next { slot, end, both } => {
            let both = if *both { " both" } else { "" };
            format!("Next {} {:04}{}", slot, end, both)
        }
        Op::SetPart(i) => format!("SetPart {}", show_path(&chunk.paths[*i])),
        Op::UpdatePart(i, op) => format!("UpdatePart {} {:?}", show_path(&chunk.paths[*i]), op),
    }
}
