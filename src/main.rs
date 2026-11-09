mod ast;
mod builtins;
mod bytecode;
mod compiler;
mod error;
mod explain;
mod lexer;
mod loader;
mod map;
mod parser;
mod repl;
mod scope;
mod serve;
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

use bytecode::Program;
use error::SomoniError;
use loader::{Loader, MAIN_FILE, SourceFile};
use typechecker::TypeChecker;

/// The port of `somoni serve` without `--port`.
const DEFAULT_PORT: u16 = 8000;

/// What the command line asks for.
enum Command {
    /// `somoni [--bytecode] file.smn [args...]`: run the file with the arguments after it, or show its bytecode.
    Run {
        path: String,
        show_bytecode: bool,
        args: Vec<String>,
    },
    /// `somoni serve [folder] [--port 8000]`: answer requests from the browser with the files of `dir`.
    Serve { dir: String, port: u16 },
    /// `somoni --explain T003`: show what an error code means, with an example.
    Explain { code: String },
    /// `somoni` without a file: read Somoni line by line in the REPL.
    Repl,
    /// `somoni --help`: show how to call somoni.
    Help,
}

/// How to call somoni, shown by `--help` and after a call it does not understand.
const USAGE: &str = "Usage: somoni                                  start the REPL
       somoni [--bytecode] <file.smn> [args...]
       somoni serve [folder] [--port 8000]
       somoni --explain <code>
       somoni --help";

/// Runs what the command line asks for.
fn main() {
    match read_command(env::args().skip(1).collect()) {
        Command::Run {
            path,
            show_bytecode,
            args,
        } => run(&path, show_bytecode, args),
        Command::Serve { dir, port } => {
            serve::serve(&dir, port).unwrap_or_else(|message| fail(&message))
        }
        Command::Explain { code } => match explain::explain(&code) {
            Ok(text) => print!("{}", text),
            Err(message) => fail(&message),
        },
        Command::Repl => repl::run(),
        Command::Help => println!("{}", USAGE),
    }
}

/// Reads the command from the arguments after the program name, or stops with the usage.
fn read_command(args: Vec<String>) -> Command {
    match args.as_slice() {
        [] => Command::Repl,
        [flag] if flag == "--help" => Command::Help,
        [flag, code] if flag == "--explain" => Command::Explain { code: code.clone() },
        [serve, rest @ ..] if serve == "serve" => read_serve(rest).unwrap_or_else(|| fail(USAGE)),
        [flag, path, rest @ ..] if flag == "--bytecode" => Command::Run {
            path: path.clone(),
            show_bytecode: true,
            args: rest.to_vec(),
        },
        [path, rest @ ..] if !path.starts_with("--") => Command::Run {
            path: path.clone(),
            show_bytecode: false,
            args: rest.to_vec(),
        },
        _ => fail(USAGE),
    }
}

/// Reads `[folder] [--port N]` after `serve`, or None if they do not fit.
fn read_serve(args: &[String]) -> Option<Command> {
    let (dir, rest) = match args {
        [first, rest @ ..] if first != "--port" => (first.clone(), rest),
        rest => (".".to_string(), rest),
    };
    let port = match rest {
        [] => DEFAULT_PORT,
        [flag, port] if flag == "--port" => port.parse().ok()?,
        _ => return None,
    };
    Some(Command::Serve { dir, port })
}

/// Runs the file `path` with `args`, or shows its bytecode, and stops with exit code 1 on an error.
fn run(path: &str, show_bytecode: bool, args: Vec<String>) {
    let (program, files) = build(path).unwrap_or_else(|message| fail(&message));
    if show_bytecode {
        print!("{}", bytecode::disassemble(&program));
        return;
    }
    if let Err(error) = vm::run(&program, args) {
        fail(&render(&error, &files));
    }
}

/// Reads, checks and compiles the program that starts in `path`, with its files for later error messages,
/// or returns the error ready to show.
fn build(path: &str) -> Result<(Program, Vec<SourceFile>), String> {
    let source =
        fs::read_to_string(path).map_err(|err| format!("Cannot read '{}': {}", path, err))?;
    let mut loader = Loader::default();
    let compiled = loader.load(path, source).and_then(|order| {
        TypeChecker::new().check(&mut loader.files, &order)?;
        compiler::compile(&loader.files, &order)
    });
    match compiled {
        Ok(program) => Ok((program, loader.files)),
        Err(error) => Err(render(&error, &loader.files)),
    }
}

/// Prints the message and stops with exit code 1.
fn fail(message: &str) -> ! {
    eprintln!("{}", message);
    process::exit(1);
}

/// Returns the message of an error with its source line, and the file name outside the main file.
fn render(error: &SomoniError, files: &[SourceFile]) -> String {
    let number = error.span.map_or(MAIN_FILE, |span| span.file);
    match files.get(number) {
        Some(file) => {
            let name = (number != MAIN_FILE).then_some(file.name.as_str());
            let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
            error.render(name, &file.source, &names)
        }
        None => error.to_string(),
    }
}
