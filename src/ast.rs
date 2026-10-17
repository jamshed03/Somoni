use std::rc::Rc;

use crate::builtins::Builtin;
use crate::span::Span;
use crate::text::Text;

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    // Rc: the interpreter shares the literal's text instead of copying it on every evaluation
    Str(Rc<Text>),
    // slot: the variable's place in the frame, filled in by the type checker
    Ident {
        name: String,
        slot: Option<usize>,
    },
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
    // [a, b, c]
    List(Vec<Expr>),
    // target[index]
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    Call {
        name: String,
        name_span: Span,
        args: Vec<Expr>,
        // what is called, filled in by the type checker
        target: Option<CallTarget>,
    },
    // Name { field: value, ... }
    StructLiteral {
        name: String,
        name_span: Span,
        fields: Vec<FieldInit>,
        // the index of the struct in `structs(stmts)`, filled in by the type checker
        index: Option<usize>,
    },
    // target.name
    Field {
        target: Box<Expr>,
        name: String,
        name_span: Span,
        // the place of the field in the struct declaration, filled in by the type checker
        index: Option<usize>,
    },
}

impl Expr {
    // the variable an assignment writes to, None if the expression is no variable
    pub fn root_variable(&self) -> Option<(&str, Span)> {
        match &self.kind {
            ExprKind::Ident { name, .. } => Some((name, self.span)),
            _ => None,
        }
    }
}

// `field: value` in a struct literal
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub name: String,
    pub name_span: Span,
    pub value: Expr,
    // the place of the field in the struct declaration, filled in by the type checker
    pub index: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CallTarget {
    Builtin(Builtin),
    // the index of the function in `functions(stmts)`
    Function(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
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
        type_name: TypeName,
        value: Expr,
        mutable: bool,
        slot: Option<usize>,
        span: Span,
    },
    Expr {
        expr: Expr,
        span: Span,
    },
    // target = value; the target is an expression, so the type checker can check it
    // like any other (the parser only lets a variable through)
    Assign {
        target: Expr,
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
    FnDecl(Function),
    // Rc: every struct value points to its declaration, for the names in print
    StructDecl(Rc<StructDecl>),
    Return {
        value: Option<Expr>,
        span: Span,
    },
}

// fn name(param: type, ...): return_type { body }
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub return_type: TypeName,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub name_span: Span,
    pub type_name: TypeName,
}

// struct Name { field: type, ... }
#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: String,
    pub name_span: Span,
    pub fields: Vec<Field>,
}

// Looks like Param, but is kept apart: the two have different reasons to change
// (parameters may get default values, fields may not).
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub name_span: Span,
    pub type_name: TypeName,
}

// a type as written in the source, e.g. `list<int>`: name, type arguments and place
#[derive(Debug, Clone, PartialEq)]
pub struct TypeName {
    pub name: String,
    pub args: Vec<TypeName>,
    pub span: Span,
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
            | Stmt::Continue { span }
            | Stmt::Return { span, .. } => Some(*span),
            Stmt::FnDecl(function) => Some(function.name_span),
            Stmt::StructDecl(decl) => Some(decl.name_span),
            Stmt::Block(_) => None,
        }
    }
}

// All structs of the program, numbered like `functions`.
pub fn structs(stmts: &[Stmt]) -> impl Iterator<Item = &Rc<StructDecl>> {
    stmts.iter().filter_map(|stmt| match stmt {
        Stmt::StructDecl(decl) => Some(decl),
        _ => None,
    })
}

// All functions of the program, in the order that gives them their index.
// The type checker and the interpreter both use this, so their numbering is the same.
pub fn functions(stmts: &[Stmt]) -> impl Iterator<Item = &Function> {
    stmts.iter().filter_map(|stmt| match stmt {
        Stmt::FnDecl(function) => Some(function),
        _ => None,
    })
}
