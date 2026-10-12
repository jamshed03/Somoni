mod ast;
mod lexer;
mod parser;
mod token;

use lexer::Lexer;
use parser::Parser;

fn main() {
    let source = "var x: int = 5;\nvar y: int = (x + 3) * 2;";
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);

    match parser.parse_program() {
        Ok(stmts) => println!("{:#?}", stmts),
        Err(msg) => println!("Error: {}", msg),
    }
}
