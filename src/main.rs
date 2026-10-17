mod ast;
mod builtins;
mod bytecode;
mod compiler;
mod error;
mod lexer;
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
use lexer::Lexer;
use parser::Parser;
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

    if let Err(error) = run(&source, show_bytecode) {
        eprintln!("{}", error.render(&source));
        process::exit(1);
    }
}

/// Runs the whole pipeline: lexer, parser, type checker, compiler, VM.
fn run(source: &str, show_bytecode: bool) -> Result<(), SomoniError> {
    let tokens = Lexer::new(source).tokenize()?;
    let mut stmts = Parser::new(tokens).parse_program()?;
    TypeChecker::new().check(&mut stmts)?;
    let program = compiler::compile(&stmts)?;
    if show_bytecode {
        print!("{}", bytecode::disassemble(&program));
        return Ok(());
    }
    vm::run(&program)
}
