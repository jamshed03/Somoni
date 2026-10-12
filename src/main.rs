mod lexer;
mod token;

use lexer::Lexer;

fn main() {
    let mut lexer = Lexer::new("var x: int = 5;");
    let tokens = lexer.tokenize();
    println!("{:?}", tokens);
}
