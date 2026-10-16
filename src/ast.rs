use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    Ident(String),
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        op_span: Span,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Bool(bool),
    Call {
        name: String,
        name_span: Span,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Less,
    Greater,
    LessEq,
    GreaterEq,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
}

// `span` of a statement: from its first token to its ';' (for if, while, for, break,
// continue only the keyword). Names and types have their own span for precise errors.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VarDecl {
        name: String,
        name_span: Span,
        type_name: String,
        type_span: Span,
        value: Expr,
        mutable: bool,
        span: Span,
    },
    Expr {
        expr: Expr,
        span: Span,
    },
    Assign {
        name: String,
        name_span: Span,
        value: Expr,
        span: Span,
    },
    Block(Vec<Stmt>),
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
        span: Span,
    },
    // body is always a Block, so every round starts with a fresh scope
    While {
        condition: Expr,
        body: Box<Stmt>,
        span: Span,
    },
    // for (init; condition; step) body – init is a VarDecl, step an Assign, body a Block
    For {
        init: Box<Stmt>,
        condition: Expr,
        step: Box<Stmt>,
        body: Box<Stmt>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
}

impl Stmt {
    pub fn span(&self) -> Option<Span> {
        match self {
            Stmt::VarDecl { span, .. }
            | Stmt::Expr { span, .. }
            | Stmt::Assign { span, .. }
            | Stmt::If { span, .. }
            | Stmt::While { span, .. }
            | Stmt::For { span, .. }
            | Stmt::Break { span }
            | Stmt::Continue { span } => Some(*span),
            Stmt::Block(_) => None,
        }
    }
}
