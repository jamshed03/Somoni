mod ast;
mod builtins;
mod bytecode;
mod compiler;
mod error;
mod interpreter;
mod lexer;
mod map;
mod parser;
mod runner;
mod scope;
mod span;
mod text;
mod token;
mod typechecker;
mod types;
mod value;
mod vm;

use std::env;
use std::fs;
use std::process;
use std::thread;

use interpreter::Interpreter;
use lexer::Lexer;
use parser::Parser;
use runner::{Runner, Stop};
use typechecker::TypeChecker;
use vm::VmRunner;

/// Stack size of the thread that runs the program. Deep recursion in Somoni needs a big stack.
const STACK_SIZE: usize = 256 * 1024 * 1024;

/// How the program is run.
#[derive(Clone, Copy)]
enum Mode {
    /// Walk the tree (`somoni file.smn`).
    Tree,
    /// Compile to bytecode and run it in the VM (`somoni --vm file.smn`).
    Vm,
    /// Only show the bytecode (`somoni --bytecode file.smn`).
    Bytecode,
}

/// Reads the file from the command line and runs it. Exit code 1 on an error.
fn main() {
    let args: Vec<String> = env::args().collect();
    let (mode, path) = match args.as_slice() {
        [_, path] => (Mode::Tree, path),
        [_, flag, path] if flag == "--vm" => (Mode::Vm, path),
        [_, flag, path] if flag == "--bytecode" => (Mode::Bytecode, path),
        _ => {
            eprintln!("Usage: somoni [--vm | --bytecode] <file.smn>");
            process::exit(1);
        }
    };

    let source = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("Cannot read '{}': {}", path, err);
            process::exit(1);
        }
    };

    let pipeline = thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(move || match run(&source, mode) {
            Ok(()) => Ok(()),
            Err(Stop::Error(error)) => Err(error.render(&source)),
        })
        .expect("cannot start the interpreter thread");

    match pipeline.join() {
        Ok(Ok(())) => {}
        Ok(Err(message)) => {
            eprintln!("{}", message);
            process::exit(1);
        }
        Err(_) => process::exit(101),
    }
}

/// Runs the whole pipeline: lexer, parser, type checker, then the tree interpreter or the VM.
fn run(source: &str, mode: Mode) -> Result<(), Stop> {
    let tokens = Lexer::new(source).tokenize()?;
    let mut stmts = Parser::new(tokens).parse_program()?;
    TypeChecker::new().check(&mut stmts)?;
    let mut runner: Box<dyn Runner> = match mode {
        Mode::Tree => Box::new(Interpreter::new()),
        Mode::Vm => Box::new(VmRunner),
        Mode::Bytecode => {
            let program = compiler::compile(&stmts)?;
            print!("{}", bytecode::disassemble(&program));
            return Ok(());
        }
    };
    runner.run(&stmts)
}
