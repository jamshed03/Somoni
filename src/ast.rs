use std::rc::Rc;

use crate::builtins::Builtin;
use crate::span::Span;
use crate::text::Text;

/// An expression with its place in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// The kinds of expressions. Fields that are `None` after parsing are filled in by the type checker.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(Rc<Text>),
    /// A variable; `slot` is its place on the stack.
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
    Null,
    /// `[a, b, c]`
    List(Vec<Expr>),
    /// `target[index]`
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    /// `name(args)`; `target` says which function is called.
    Call {
        name: String,
        name_span: Span,
        args: Vec<Expr>,
        target: Option<CallTarget>,
    },
    /// `Name { field: value, ... }`; `index` is the number of the struct in `structs`.
    StructLiteral {
        name: String,
        name_span: Span,
        fields: Vec<FieldInit>,
        index: Option<usize>,
    },
    /// `target.name`; `index` is the position of the field in the struct.
    Field {
        target: Box<Expr>,
        name: String,
        name_span: Span,
        index: Option<usize>,
    },
}

impl Expr {
    /// Returns the variable at the start of an assignment target, e.g. `l` in `l[0].x`.
    /// Returns None if the target does not start with a variable.
    pub fn root_variable(&self) -> Option<(&str, Span)> {
        match &self.kind {
            ExprKind::Ident { name, .. } => Some((name, self.span)),
            ExprKind::Index { target, .. } | ExprKind::Field { target, .. } => {
                target.root_variable()
            }
            _ => None,
        }
    }
}

/// One `field: value` in a struct literal; `index` is the position of the field in the struct.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub name: String,
    pub name_span: Span,
    pub value: Expr,
    pub index: Option<usize>,
}

/// What a call calls: a built-in function, or a function of the program by its number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CallTarget {
    Builtin(Builtin),
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

/// The kinds of statements. `span` is the place error messages point to.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `var name: type = value;` or `const ...`
    VarDecl {
        name: String,
        name_span: Span,
        type_name: TypeName,
        value: Expr,
        mutable: bool,
        slot: Option<usize>,
        span: Span,
    },
    /// A call as a statement, e.g. `print(x);`
    Expr {
        expr: Expr,
        span: Span,
    },
    /// `target = value;` where the target is a variable, a list element or a field.
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
    While {
        condition: Expr,
        body: Box<Stmt>,
        span: Span,
    },
    /// `for (init; condition; step) body`
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
    StructDecl(Rc<StructDecl>),
    Return {
        value: Option<Expr>,
        span: Span,
    },
}

/// `fn name(param: type, ...): return_type { body }`
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub return_type: TypeName,
    pub body: Vec<Stmt>,
}

/// One `name: type` in the parameters of a function.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub name_span: Span,
    pub type_name: TypeName,
}

/// `struct Name { field: type, ... }`
#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: String,
    pub name_span: Span,
    pub fields: Vec<Field>,
}

/// One `name: type` in a struct declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub name_span: Span,
    pub type_name: TypeName,
}

/// A type as written in the source, e.g. `list<int>?`: the name `list`, the argument `int`
/// and `optional` for the `?`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeName {
    pub name: String,
    pub args: Vec<TypeName>,
    pub optional: bool,
    pub span: Span,
}

impl Stmt {
    /// Returns the place of the statement for error messages (None for a block).
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

/// Returns all structs of the program. Their order gives each struct its number.
pub fn structs(stmts: &[Stmt]) -> impl Iterator<Item = &Rc<StructDecl>> {
    stmts.iter().filter_map(|stmt| match stmt {
        Stmt::StructDecl(decl) => Some(decl),
        _ => None,
    })
}

/// Returns all functions of the program. Their order gives each function its number.
pub fn functions(stmts: &[Stmt]) -> impl Iterator<Item = &Function> {
    stmts.iter().filter_map(|stmt| match stmt {
        Stmt::FnDecl(function) => Some(function),
        _ => None,
    })
}
