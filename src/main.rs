mod ast;
mod builtins;
mod bytecode;
mod compiler;
mod error;
mod lexer;
mod loader;
mod map;
mod parser;
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

use error::SomoniError;
use loader::{Loader, MAIN_FILE, SourceFile};
use typechecker::TypeChecker;

/// What the command line asks for: the file to run and whether to show its bytecode instead.
struct Options {
    path: String,
    show_bytecode: bool,
}

/// Runs the file from the command line, or shows its bytecode with `--bytecode`.
fn main() {
    let options = read_options(env::args().collect());
    let source = fs::read_to_string(&options.path)
        .unwrap_or_else(|err| fail(&format!("Cannot read '{}': {}", options.path, err)));
    let mut loader = Loader::default();
    let result = loader
        .load(&options.path, source)
        .and_then(|order| run(&mut loader.files, &order, options.show_bytecode));
    if let Err(error) = result {
        fail(&render(&error, &loader.files));
    }
}

/// Reads the options from the command line, or stops with the usage.
fn read_options(args: Vec<String>) -> Options {
    let (show_bytecode, path) = match args.as_slice() {
        [_, path] => (false, path),
        [_, flag, path] if flag == "--bytecode" => (true, path),
        _ => fail("Usage: somoni [--bytecode] <file.smn>"),
    };
    Options {
        path: path.clone(),
        show_bytecode,
    }
}

/// Prints the message and stops with exit code 1.
fn fail(message: &str) -> ! {
    eprintln!("{}", message);
    process::exit(1);
}

/// Runs the rest of the pipeline on the loaded files: type checker, compiler, VM.
fn run(files: &mut [SourceFile], order: &[usize], show_bytecode: bool) -> Result<(), SomoniError> {
    TypeChecker::new().check(files, order)?;
    let program = compiler::compile(files, order)?;
    if show_bytecode {
        print!("{}", bytecode::disassemble(&program));
        return Ok(());
    }
    vm::run(&program)
}

/// Returns the message of an error with its source line, and the file name outside the main file.
fn render(error: &SomoniError, files: &[SourceFile]) -> String {
    let number = error.span.map_or(MAIN_FILE, |span| span.file);
    match files.get(number) {
        Some(file) => {
            let name = (number != MAIN_FILE).then_some(file.name.as_str());
            error.render(name, &file.source)
        }
        None => error.to_string(),
    }
}
