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
use loader::{Loader, SourceFile};
use typechecker::TypeChecker;

/// Reads the file from the command line and runs it. Exit code 1 on an error.
/// With `--bytecode`, it shows the instructions instead of running them.
fn main() {
    let args: Vec<String> = env::args().collect();
    let (show_bytecode, path) = match args.as_slice() {
        [_, path] => (false, path),
        [_, flag, path] if flag == "--bytecode" => (true, path),
        _ => {
            eprintln!("Usage: somoni [--bytecode] <file.smn>");
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

    let mut loader = Loader::default();
    let result = loader
        .load(path, source)
        .and_then(|order| run(&mut loader.files, &order, show_bytecode));
    if let Err(error) = result {
        eprintln!("{}", render(&error, &loader.files));
        process::exit(1);
    }
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

/// Returns the message of an error with the line of the file it is in.
/// Only errors outside the main file show the name of their file.
fn render(error: &SomoniError, files: &[SourceFile]) -> String {
    let number = error.span.map_or(0, |span| span.file);
    match files.get(number) {
        Some(file) => {
            let name = (number != 0).then_some(file.name.as_str());
            error.render(name, &file.source)
        }
        None => error.to_string(),
    }
}
