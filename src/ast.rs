#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Ident(String),
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VarDecl {
        name: String,
        type_name: String,
        value: Expr,
        mutable: bool,
        line: usize,
    },
    Print {
        value: Expr,
        line: usize,
    },
}
