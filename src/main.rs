mod ast;
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

use error::SomoniError;
use interpreter::Interpreter;
use lexer::Lexer;
use parser::Parser;
use typechecker::TypeChecker;

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

    if let Err(error) = run(&source) {
        eprintln!("{}", error);
        process::exit(1);
    }
}

// The pipeline: every station gets the output of the one before.
// `?` stops at the first error, later stations do not run.
fn run(source: &str) -> Result<(), SomoniError> {
    let tokens = Lexer::new(source).tokenize()?;
    let stmts = Parser::new(tokens).parse_program()?;
    TypeChecker::new().check(&stmts)?;
    Interpreter::new().run(&stmts)
}
