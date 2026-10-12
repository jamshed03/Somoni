mod ast;
mod interpreter;
mod lexer;
mod parser;
mod token;

use interpreter::Interpreter;
use lexer::Lexer;
use parser::Parser;

fn main() {
    let source = "const a: int = 7;\nvar x: int = 5;\nprint(x + a);\nprint((x + a) * 2);";
    let tokens = Lexer::new(source).tokenize();

    let stmts = match Parser::new(tokens).parse_program() {
        Ok(stmts) => stmts,
        Err(msg) => {
            eprintln!("Parse error: {}", msg);
            return;
        }
    };

    if let Err(msg) = Interpreter::new().run(&stmts) {
        eprintln!("Runtime error: {}", msg);
    }
}
