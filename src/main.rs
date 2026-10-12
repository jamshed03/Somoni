mod lexer;
mod token;

use lexer::Lexer;

fn main() {
    let mut lexer = Lexer::new("   \n  ");
    let tokens = lexer.tokenize();
    println!("{:?}", tokens);
}
