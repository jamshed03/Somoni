mod ast;
mod lexer;
mod token;

use ast::{BinaryOp, Expr};

fn main() {
    let expr = Expr::Binary {
        left: Box::new(Expr::Binary {
            left: Box::new(Expr::Int(5)),
            op: BinaryOp::Add,
            right: Box::new(Expr::Int(3)),
        }),
        op: BinaryOp::Mul,
        right: Box::new(Expr::Int(2)),
    };
    println!("{:#?}", expr);
}
