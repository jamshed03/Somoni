mod ast;
mod lexer;
mod parser;
mod token;

use lexer::Lexer;
use parser::Parser;

fn main() {
    let mut lexer = Lexer::new("(5 + 3) * 2");
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);

    match parser.parse_expr() {
        Ok(expr) => println!("{:#?}", expr),
        Err(msg) => println!("Error: {}", msg),
    }
}
