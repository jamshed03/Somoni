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
    /// `{key: value, ...}`
    Map(Vec<(Expr, Expr)>),
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
    /// `new Name(args)`: creates an object of a class. The type checker fills in `target`:
    /// the number of the function that builds it (its constructor).
    New {
        name: String,
        name_span: Span,
        args: Vec<Expr>,
        target: Option<usize>,
    },
    /// `target.name(args)`: calls a method of an object. The type checker fills in `target_fn`:
    /// the number of the function of the method.
    MethodCall {
        target: Box<Expr>,
        name: String,
        name_span: Span,
        args: Vec<Expr>,
        target_fn: Option<usize>,
    },
    /// `Name { field: value, ... }`; `index` is the number of the struct in `structs`.
    StructLiteral {
        name: String,
        name_span: Span,
        fields: Vec<FieldInit>,
        index: Option<usize>,
    },
    /// A variant like `Farbe.Rot`. The parser reads it as a field; the type checker
    /// turns it into this when `Farbe` is an enum and not a variable.
    EnumValue {
        decl: Rc<EnumDecl>,
        variant: usize,
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
    /// Returns the place of the first `this` in the expression, if there is one.
    pub fn find_this(&self) -> Option<Span> {
        match &self.kind {
            ExprKind::Ident { name, .. } => (name == THIS).then_some(self.span),
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::EnumValue { .. } => None,
            ExprKind::Binary { left, right, .. } => left.find_this().or_else(|| right.find_this()),
            ExprKind::Unary { operand, .. } => operand.find_this(),
            ExprKind::List(items)
            | ExprKind::Call { args: items, .. }
            | ExprKind::New { args: items, .. } => items.iter().find_map(Expr::find_this),
            ExprKind::Map(entries) => entries
                .iter()
                .find_map(|(key, value)| key.find_this().or_else(|| value.find_this())),
            ExprKind::Index { target, index } => target.find_this().or_else(|| index.find_this()),
            ExprKind::StructLiteral { fields, .. } => {
                fields.iter().find_map(|init| init.value.find_this())
            }
            ExprKind::Field { target, .. } => target.find_this(),
            ExprKind::MethodCall { target, args, .. } => target
                .find_this()
                .or_else(|| args.iter().find_map(Expr::find_this)),
        }
    }

    /// Returns `rest` if this expression is `x + rest` and `x` is the variable in `slot`.
    pub fn appended_to(&self, slot: usize) -> Option<&Expr> {
        let ExprKind::Binary {
            left,
            op: BinaryOp::Add,
            right,
            ..
        } = &self.kind
        else {
            return None;
        };
        let ExprKind::Ident {
            slot: Some(left_slot),
            ..
        } = left.kind
        else {
            return None;
        };
        (left_slot == slot).then_some(right)
    }

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

/// What a `for`-each loop runs over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IterKind {
    List,
    Map,
    Str,
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
    /// With `op`, it is `target += value;` (or `-=`, ...) with the place of the operator.
    /// The type checker fills in `in_object`: true if the target lies inside an object (`u.age`).
    Assign {
        target: Expr,
        op: Option<(BinaryOp, Span)>,
        value: Expr,
        in_object: bool,
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
    /// `for (x: T in iterable) body` or `for (a: T, b: U in iterable) body`.
    /// The type checker fills in `slots` (one per variable), `seq_slot` (a hidden copy of the
    /// iterable, the hidden index is the slot after it) and `over`.
    ForEach {
        vars: Vec<Param>,
        iterable: Expr,
        body: Box<Stmt>,
        slots: Vec<usize>,
        seq_slot: Option<usize>,
        over: Option<IterKind>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    FnDecl(Function),
    Import(Import),
    StructDecl(Rc<StructDecl>),
    ClassDecl(ClassDecl),
    EnumDecl(Rc<EnumDecl>),
    /// `match (subject) { Farbe.Rot => { ... } _ => { ... } }`.
    /// The type checker fills in `arm_of_variant`: for each variant, the number of its branch,
    /// and `unwraps`: true for a result, whose branches get the value inside it.
    Match {
        subject: Expr,
        arms: Vec<MatchArm>,
        arm_of_variant: Vec<usize>,
        unwraps: bool,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
}

impl Stmt {
    /// Returns the place of the first `this` in the statement, if there is one.
    /// Declarations inside it (`fn`, `struct`, ...) do not count.
    pub fn find_this(&self) -> Option<Span> {
        match self {
            Stmt::VarDecl { value, .. } => value.find_this(),
            Stmt::Expr { expr, .. } => expr.find_this(),
            Stmt::Assign { target, value, .. } => target.find_this().or_else(|| value.find_this()),
            Stmt::Block(stmts) => stmts.iter().find_map(Stmt::find_this),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => condition
                .find_this()
                .or_else(|| then_branch.find_this())
                .or_else(|| else_branch.as_ref().and_then(|branch| branch.find_this())),
            Stmt::While {
                condition, body, ..
            } => condition.find_this().or_else(|| body.find_this()),
            Stmt::For {
                init,
                condition,
                step,
                body,
                ..
            } => init
                .find_this()
                .or_else(|| condition.find_this())
                .or_else(|| step.find_this())
                .or_else(|| body.find_this()),
            Stmt::ForEach { iterable, body, .. } => {
                iterable.find_this().or_else(|| body.find_this())
            }
            Stmt::Match { subject, arms, .. } => subject
                .find_this()
                .or_else(|| arms.iter().find_map(|arm| arm.body.find_this())),
            Stmt::Return { value, .. } => value.as_ref().and_then(Expr::find_this),
            Stmt::Break { .. }
            | Stmt::Continue { .. }
            | Stmt::FnDecl(_)
            | Stmt::Import(_)
            | Stmt::StructDecl(_)
            | Stmt::ClassDecl(_)
            | Stmt::EnumDecl(_) => None,
        }
    }
}

/// `fn name(param: type, ...): return_type { body }`, `exported` with `export` in front.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub exported: bool,
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub return_type: TypeName,
    pub body: Vec<Stmt>,
}

/// `import { quadrat, Punkt } from "mathe.smn";`
/// The loader fills in `file`: the number of the imported file.
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub names: Vec<ImportName>,
    pub path: String,
    pub path_span: Span,
    pub file: Option<usize>,
    pub span: Span,
}

/// One name in the `{ ... }` of an import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportName {
    pub name: String,
    pub span: Span,
}

/// One `name: type` in the parameters of a function.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub name_span: Span,
    pub type_name: TypeName,
}

/// `struct Name { field: type, ... }`, `exported` with `export` in front.
#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub exported: bool,
    pub name: String,
    pub name_span: Span,
    pub fields: Vec<Field>,
}

/// `class Name { field: type = start; ... constructor(...) { ... } method(...): type { ... } }`,
/// `exported` with `export` in front.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub exported: bool,
    pub layout: Rc<ClassLayout>,
    pub name_span: Span,
    pub fields: Vec<ClassField>,
    pub constructor: Option<Constructor>,
    /// The methods; each gets `this` as a hidden first parameter (slot 0).
    pub methods: Vec<Method>,
}

/// Who may use a field or method of a class: everyone, or only the class itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Visibility {
    Public,
    Private,
}

/// A method of a class with its visibility.
#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub visibility: Visibility,
    pub function: Function,
}

/// `constructor(name: string) { ... }` of a class. The type checker fills in `this_slot`:
/// the slot of the hidden variable `this`, after the parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct Constructor {
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub this_slot: Option<usize>,
    pub span: Span,
}

/// The name of the variable that holds the object in a constructor or method.
/// `this` is a keyword, so no other variable can have this name.
pub const THIS: &str = "this";

/// The name of a class and of its fields, in declaration order. Types and objects point to it,
/// so it is the identity of the class (see `types::Decl`).
#[derive(Debug, Clone, PartialEq)]
pub struct ClassLayout {
    pub name: String,
    pub fields: Vec<String>,
}

/// One field of a class: `name: type;` or with a start value `name: type = start;`,
/// maybe with `public` or `private` in front.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassField {
    pub visibility: Visibility,
    pub field: Field,
    pub start: Option<Expr>,
}

/// `enum Name { Variant, ... }`, `exported` with `export` in front.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub exported: bool,
    pub name: String,
    pub name_span: Span,
    pub variants: Vec<(String, Span)>,
}

/// One branch of a `match`: `Farbe.Rot => { ... }` or `_ => { ... }`.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Stmt,
}

/// What a branch of `match` stands for.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// `Farbe.Rot`
    Variant {
        enum_name: String,
        enum_span: Span,
        name: String,
        name_span: Span,
    },
    /// `ok(n: int)` or `err(e: string)`: the variable gets the value inside the result.
    /// The type checker fills in the slot of the variable.
    Result {
        ok: bool,
        var: Param,
        slot: Option<usize>,
        span: Span,
    },
    /// `_`: all variants that have no branch of their own
    Rest(Span),
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

/// Returns all structs of the program. Their order gives each struct its number.
pub fn structs(stmts: &[Stmt]) -> impl Iterator<Item = &Rc<StructDecl>> {
    stmts.iter().filter_map(|stmt| match stmt {
        Stmt::StructDecl(decl) => Some(decl),
        _ => None,
    })
}

/// Returns all classes of a file. Their order gives each class its number.
pub fn classes(stmts: &[Stmt]) -> impl Iterator<Item = &ClassDecl> {
    stmts.iter().filter_map(|stmt| match stmt {
        Stmt::ClassDecl(decl) => Some(decl),
        _ => None,
    })
}

/// Returns all enums of the program.
pub fn enums(stmts: &[Stmt]) -> impl Iterator<Item = &Rc<EnumDecl>> {
    stmts.iter().filter_map(|stmt| match stmt {
        Stmt::EnumDecl(decl) => Some(decl),
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
