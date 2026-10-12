mod ast;
mod lexer;
mod parser;
mod token;

use lexer::Lexer;
use parser::Parser;

fn main() {
    let source = "const a: int = 7;\nvar x: int = 5;\nprint(x + a);";
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);

    match parser.parse_program() {
        Ok(stmts) => println!("{:#?}", stmts),
        Err(msg) => println!("Error: {}", msg),
    }
}
