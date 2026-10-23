use std::fmt::Write;
use std::rc::Rc;

use crate::ast::{BinaryOp, ClassLayout, InterfaceLayout, StructDecl, UnaryOp, VTable};
use crate::builtins::Builtin;
use crate::span::Span;
use crate::value::{Quoted, Value};

/// One instruction of the VM, which mostly takes its operands from the stack and pushes its result.
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
    /// Calls method slot `slot` with `args` values from the stack, the object first, by its method table.
    CallVirtual { slot: usize, args: usize },
    /// Calls method `method` of interface `interface` with `args` values from the stack, the object first, by its vtable.
    CallInterface {
        interface: usize,
        method: usize,
        args: usize,
    },
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
    /// Pushes a new object of class number `i` whose fields are not set yet.
    NewObject(usize),
    /// Pops a value and sets field `p` of the struct or object below.
    InitField(usize),
    /// Pops a value and sets field `p` of the object in local `slot` (start values in `init`).
    InitLocalField(usize, usize),
    /// Pops an index (or key) and a list (or map), pushes the element.
    Index,
    /// Pops a struct or object, pushes its field `p`.
    Field(usize),
    /// Pops a value and appends it to the string or list in this slot, or adds it to a number (`s = s + x`).
    AppendLocal(usize),
    /// Pops a value and the indices of path number `i`, writes the value into that part.
    SetPart(usize),
    /// Like `SetPart`, but computes `part op value` and writes that (`l[i] += 1`).
    UpdatePart(usize, BinaryOp),
    /// Pops a string, pushes the list of its characters (for `for`-each over a string).
    Chars,
    /// One round of a `for`-each loop over `slot`: pushes the next element or key (with `both` also the index or value), or goes on at `end`.
    Next { slot: usize, end: usize, both: bool },
    /// Pops an enum or result and goes on at its branch in jump table `i`, pushing the value inside a result first.
    Switch(usize),
}

/// The way from a variable to a part of it, e.g. `l[i].x`: the slot of `l` and the steps.
#[derive(Debug)]
pub struct Path {
    pub slot: usize,
    pub steps: Vec<PathStep>,
    /// True if the way goes through an object, see `value::Way`.
    pub in_object: bool,
}

impl Path {
    /// Returns how many index values the path takes from the stack.
    pub fn index_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| matches!(step, PathStep::Index(_)))
            .count()
    }
}

/// Returns the slot of the hidden index of a `for`-each loop, right after its sequence in `seq`.
pub fn index_slot(seq: usize) -> usize {
    seq + 1
}

/// One step of a path: an index, whose value is on the stack, or a field.
#[derive(Debug)]
pub enum PathStep {
    Index(Span),
    Field(usize),
}

/// The instructions of one function with their places in the source, constants, paths and jump tables.
#[derive(Debug, Default)]
pub struct Chunk {
    pub name: String,
    pub code: Vec<Op>,
    /// The place of each instruction, kept apart from `code` so the VM loop reads only instructions.
    pub spans: Vec<Span>,
    pub constants: Vec<Value>,
    pub paths: Vec<Path>,
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

    /// Adds a path of `SetPart` or `UpdatePart` and returns its number.
    pub fn add_path(&mut self, path: Path) -> usize {
        self.paths.push(path);
        self.paths.len() - 1
    }

    /// Adds an empty jump table of `Switch` and returns its number.
    pub fn add_table(&mut self) -> usize {
        self.tables.push(Vec::new());
        self.tables.len() - 1
    }
}

/// A compiled program: the top-level code and its functions, structs, classes and interfaces, numbered file by file.
#[derive(Debug, Default)]
pub struct Program {
    pub main: Chunk,
    pub functions: Vec<Chunk>,
    pub structs: Vec<Rc<StructDecl>>,
    pub classes: Vec<ClassCode>,
    pub interfaces: Vec<Rc<InterfaceLayout>>,
}

/// The layout of a class and the functions its objects run for each method.
#[derive(Debug)]
pub struct ClassCode {
    pub layout: Rc<ClassLayout>,
    pub method_table: Vec<usize>,
    pub vtables: Vec<VTable>,
}

impl ClassCode {
    /// Returns the function this class runs for method `method` of interface `interface`.
    pub fn interface_function(&self, interface: usize, method: usize) -> Option<usize> {
        self.vtables
            .iter()
            .find(|vtable| vtable.interface == interface)
            .map(|vtable| vtable.functions[method])
    }
}

/// Returns the instructions of a program as text, one per line, for `--bytecode`.
pub fn disassemble(program: &Program) -> String {
    let mut out = String::new();
    for chunk in std::iter::once(&program.main).chain(&program.functions) {
        disassemble_chunk(&mut out, chunk, program);
    }
    out
}

/// Writes the instructions of one chunk as text, the line number only where it changes.
fn disassemble_chunk(out: &mut String, chunk: &Chunk, program: &Program) {
    let _ = writeln!(out, "== {} ({} locals) ==", chunk.name, chunk.locals);
    let mut last_line = 0;
    for (i, (op, span)) in chunk.code.iter().zip(&chunk.spans).enumerate() {
        let line = show_line(span.line, last_line);
        last_line = span.line;
        let _ = writeln!(out, "{:04}  {}  {}", i, line, show_op(op, chunk, program));
    }
}

/// Shows a line number, or blanks if it is the same as the line before.
fn show_line(line: usize, last_line: usize) -> String {
    if line == last_line {
        "    ".to_string()
    } else {
        format!("{:>4}", line)
    }
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

/// Shows the targets of a jump table, e.g. `0004, 0009`.
fn show_table(targets: &[usize]) -> String {
    let targets: Vec<String> = targets
        .iter()
        .map(|target| format!("{:04}", target))
        .collect();
    targets.join(", ")
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
        Op::CallVirtual { slot, args } => format!("CallVirtual #{} {}", slot, args),
        Op::CallInterface {
            interface,
            method,
            args,
        } => {
            let layout = &program.interfaces[*interface];
            let method = &layout.methods[*method];
            format!("CallInterface {}.{} {}", layout.name, method, args)
        }
        Op::Return => "Return".to_string(),
        Op::MakeList(count) => format!("MakeList {}", count),
        Op::NewMap => "NewMap".to_string(),
        Op::MapInsert => "MapInsert".to_string(),
        Op::NewStruct(i) => format!("NewStruct {}", program.structs[*i].name),
        Op::NewObject(i) => format!("NewObject {}", program.classes[*i].layout.name),
        Op::InitField(p) => format!("InitField {}", p),
        Op::InitLocalField(slot, p) => format!("InitLocalField {} {}", slot, p),
        Op::Index => "Index".to_string(),
        Op::Field(p) => format!("Field {}", p),
        Op::AppendLocal(slot) => format!("AppendLocal {}", slot),
        Op::Chars => "Chars".to_string(),
        Op::Switch(i) => format!("Switch [{}]", show_table(&chunk.tables[*i])),
        Op::Next { slot, end, both } => {
            let both = if *both { " both" } else { "" };
            format!("Next {} {:04}{}", slot, end, both)
        }
        Op::SetPart(i) => format!("SetPart {}", show_path(&chunk.paths[*i])),
        Op::UpdatePart(i, op) => format!("UpdatePart {} {:?}", show_path(&chunk.paths[*i]), op),
    }
}
