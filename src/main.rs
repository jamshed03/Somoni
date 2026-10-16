mod ast;
mod builtins;
mod error;
mod interpreter;
mod lexer;
mod parser;
mod scope;
mod span;
mod token;
mod typechecker;
mod types;

use std::env;
use std::fs;
use std::process;
use std::thread;

use error::SomoniError;
use interpreter::Interpreter;
use lexer::Lexer;
use parser::Parser;
use typechecker::TypeChecker;

// Every Somoni call uses several Rust calls, and the main thread has only a few MB
// of stack (a debug build crashes after about 700 Somoni calls). So the pipeline
// runs in its own thread with a big stack; memory is only used when needed.
const STACK_SIZE: usize = 256 * 1024 * 1024;

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
        // a panic (a bug in Somoni): Rust already printed the message
        Err(_) => process::exit(101),
    }
}

fn run(source: &str) -> Result<(), SomoniError> {
    let tokens = Lexer::new(source).tokenize()?;
    let stmts = Parser::new(tokens).parse_program()?;
    TypeChecker::new().check(&stmts)?;
    Interpreter::new().run(&stmts)
}
