mod token;
use token::Token;

fn main() {
    let tokens = vec![
        Token::Var,
        Token::Ident("x".to_string()),
        Token::Colon,
        Token::Ident("int".to_string()),
        Token::Equal,
        Token::Int(5),
        Token::Semicolon,
        Token::Eof,
    ];
    println!("{:?}", tokens);
}
