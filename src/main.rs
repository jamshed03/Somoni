mod ast;
mod interpreter;
mod lexer;
mod parser;
mod token;
mod types;
mod typechecker;

use std::env;
use std::fs;
use std::process;

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

    let tokens = match Lexer::new(&source).tokenize() {
        Ok(tokens) => tokens,
        Err(msg) => {
            eprintln!("Lex error: {}", msg);
            process::exit(1);
        }
    };

    let stmts = match Parser::new(tokens).parse_program() {
        Ok(stmts) => stmts,
        Err(msg) => {
            eprintln!("Parse error: {}", msg);
            process::exit(1);
        }
    };

    if let Err(msg) = TypeChecker::new().check(&stmts) {
        eprintln!("Type error: {}", msg);
        process::exit(1);
    }
}
