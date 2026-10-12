mod ast;
mod interpreter;
mod lexer;
mod parser;
mod token;

use std::env;
use std::fs;
use std::process;

use interpreter::Interpreter;
use lexer::Lexer;
use parser::Parser;

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

    let tokens = Lexer::new(&source).tokenize();

    let stmts = match Parser::new(tokens).parse_program() {
        Ok(stmts) => stmts,
        Err(msg) => {
            eprintln!("Parse error: {}", msg);
            process::exit(1);
        }
    };

    if let Err(msg) = Interpreter::new().run(&stmts) {
        eprintln!("Runtime error: {}", msg);
        process::exit(1);
    }
}
