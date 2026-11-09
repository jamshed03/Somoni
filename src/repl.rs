//! The REPL: `somoni` without a file reads Somoni line by line and runs every input right away.
//! Errors are shown on stdout like results, because in the REPL they are part of the conversation, and end nothing.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use crate::ast::Stmt;
use crate::bytecode::Program;
use crate::compiler;
use crate::error::{ErrorCode, SomoniError};
use crate::lexer::Lexer;
use crate::loader::{MAIN_FILE, SourceFile};
use crate::parser::Parser;
use crate::typechecker::TypeChecker;
use crate::vm;

/// The prompt before an input, shown only when a person types into a terminal.
const PROMPT: &str = "> ";

/// Reads inputs until the input ends (Ctrl+D) and runs each one.
pub fn run() {
    let stdin = io::stdin();
    let terminal = stdin.is_terminal();
    loop {
        if terminal {
            print!("{}", PROMPT);
            let _ = io::stdout().flush();
        }
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if !line.trim().is_empty() {
            run_input(line);
        }
    }
    if terminal {
        println!();
    }
}

/// Reads, checks, compiles and runs one input, and shows an error if there is one.
fn run_input(source: String) {
    let mut files = vec![SourceFile {
        name: "<repl>".to_string(),
        dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        source,
        stmts: Vec::new(),
    }];
    let ran = build(&mut files).and_then(|program| vm::run(&program, Vec::new()));
    if let Err(error) = ran {
        println!("{}", crate::render(&error, &files));
    }
}

/// Parses, checks and compiles the input in `files`, the only file; `import` is M008 for now.
fn build(files: &mut [SourceFile]) -> Result<Program, SomoniError> {
    let tokens = Lexer::new(&files[MAIN_FILE].source, MAIN_FILE).tokenize()?;
    let stmts = Parser::new(tokens).parse_program(false)?;
    if let Some(Stmt::Import(import)) = stmts.iter().find(|stmt| matches!(stmt, Stmt::Import(_))) {
        return Err(SomoniError::at(import.span, ErrorCode::ImportInRepl));
    }
    files[MAIN_FILE].stmts = stmts;
    TypeChecker::new().check(files, &[MAIN_FILE])?;
    compiler::compile(files, &[MAIN_FILE])
}
