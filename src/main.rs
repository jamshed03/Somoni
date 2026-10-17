mod ast;
mod builtins;
mod error;
mod interpreter;
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

use std::env;
use std::fs;
use std::process;
use std::thread;

use error::SomoniError;
use interpreter::Interpreter;
use lexer::Lexer;
use parser::Parser;
use typechecker::TypeChecker;

/// Stack size of the thread that runs the program. Deep recursion in Somoni needs a big stack.
const STACK_SIZE: usize = 256 * 1024 * 1024;

/// Reads the file from the command line and runs it. Exit code 1 on an error.
fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: somoni <file.smn>");
        process::exit(1);
    }

    let source = match fs::read_to_string(&args[1]) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("Cannot read '{}': {}", args[1], err);
            process::exit(1);
        }
    };

    let pipeline = thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(move || run(&source).map_err(|error| error.render(&source)))
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

/// Runs the whole pipeline: lexer, parser, type checker, interpreter.
fn run(source: &str) -> Result<(), SomoniError> {
    let tokens = Lexer::new(source).tokenize()?;
    let mut stmts = Parser::new(tokens).parse_program()?;
    TypeChecker::new().check(&mut stmts)?;
    Interpreter::new().run(&stmts)
}
