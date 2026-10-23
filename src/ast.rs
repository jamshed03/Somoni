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
    Call(Call),
    New(New),
    MethodCall(MethodCall),
    /// `super(args)` in a constructor, which runs the `init` of the parent class (`target`) on `this`.
    SuperInit {
        args: Vec<Expr>,
        target: Option<usize>,
    },
    SuperCall(SuperCall),
    StaticCall(StaticCall),
    StaticAccess(StaticAccess),
    InstanceOf(InstanceOf),
    /// `Name { field: value, ... }`; `index` is the number of the struct in `structs`.
    StructLiteral {
        name: String,
        name_span: Span,
        fields: Vec<FieldInit>,
        index: Option<usize>,
    },
    /// A variant like `Farbe.Rot`, which the type checker makes from a field when `Farbe` is an enum.
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

/// `name(args)`, a call of a built-in or an own function.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub name: String,
    pub name_span: Span,
    pub args: Vec<Expr>,
    /// Filled in by the type checker: the function that is called.
    pub target: Option<CallTarget>,
}

/// `new Name(args)`, which creates an object of a class.
#[derive(Debug, Clone, PartialEq)]
pub struct New {
    pub name: String,
    pub name_span: Span,
    pub args: Vec<Expr>,
    /// Filled in by the type checker: the number of the class.
    pub class: Option<usize>,
    /// Filled in by the type checker: the function `init Name` of the class.
    pub target: Option<usize>,
}

/// `target.name(args)`, a call of a method of an object.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodCall {
    pub target: Box<Expr>,
    pub name: String,
    pub name_span: Span,
    pub args: Vec<Expr>,
    /// Filled in by the type checker: which function runs.
    pub call: Option<MethodTarget>,
}

/// `super.name(args)`, a call of the method of the parent class on `this` without dynamic dispatch.
#[derive(Debug, Clone, PartialEq)]
pub struct SuperCall {
    pub name: String,
    pub name_span: Span,
    pub args: Vec<Expr>,
    /// Filled in by the type checker: the function of that method.
    pub target: Option<usize>,
}

/// `Class::name(args)`, a call of a static method, which has no `this`.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticCall {
    pub class_name: String,
    pub class_span: Span,
    pub name: String,
    pub name_span: Span,
    pub args: Vec<Expr>,
    /// Filled in by the type checker: the function of the method.
    pub target: Option<usize>,
}

/// `Class::name`, a static field of a class.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticAccess {
    pub class_name: String,
    pub class_span: Span,
    pub name: String,
    pub name_span: Span,
    /// Filled in by the type checker: the global number of the field.
    pub global: Option<usize>,
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
            ExprKind::List(items) => first_this(items),
            ExprKind::Call(call) => first_this(&call.args),
            ExprKind::New(new) => first_this(&new.args),
            ExprKind::Map(entries) => entries
                .iter()
                .find_map(|(key, value)| key.find_this().or_else(|| value.find_this())),
            ExprKind::Index { target, index } => target.find_this().or_else(|| index.find_this()),
            ExprKind::StructLiteral { fields, .. } => {
                fields.iter().find_map(|init| init.value.find_this())
            }
            ExprKind::Field { target, .. } => target.find_this(),
            ExprKind::MethodCall(call) => {
                call.target.find_this().or_else(|| first_this(&call.args))
            }
            ExprKind::SuperInit { args, .. } => first_this(args),
            ExprKind::SuperCall(_) => Some(self.span),
            ExprKind::StaticCall(call) => first_this(&call.args),
            ExprKind::StaticAccess(_) => None,
            ExprKind::InstanceOf(test) => test.value.find_this(),
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

    /// Returns the variable at the start of an assignment target, e.g. `l` in `l[0].x`, if there is one.
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

/// Returns the place of the first `this` in these expressions, if there is one.
fn first_this(exprs: &[Expr]) -> Option<Span> {
    exprs.iter().find_map(Expr::find_this)
}

/// Which method a method call runs; the class of the object picks the function at runtime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MethodTarget {
    /// Method slot `slot` of a class.
    Virtual { slot: usize },
    /// Method number `method` of interface number `interface`.
    Interface { interface: usize, method: usize },
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

/// `value instanceof Name`, which asks whether an object belongs to a class or interface.
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceOf {
    pub value: Box<Expr>,
    pub type_name: String,
    pub type_span: Span,
    /// Filled in by the type checker: the class or interface by its number.
    pub target: Option<InstanceTarget>,
}

/// The class or interface that `instanceof` asks for, by its number in the program.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InstanceTarget {
    Class(usize),
    Interface(usize),
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
    Assign(Assign),
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
    ForEach(ForEach),
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
    InterfaceDecl(InterfaceDecl),
    EnumDecl(Rc<EnumDecl>),
    Match(Match),
    Return {
        value: Option<Expr>,
        span: Span,
    },
}

/// `target = value;` or `target += value;`, where the target is a variable, a list element or a field.
#[derive(Debug, Clone, PartialEq)]
pub struct Assign {
    pub target: Expr,
    /// The operator of `+=`, `-=`, ... and its place.
    pub op: Option<(BinaryOp, Span)>,
    pub value: Expr,
    /// Filled in by the type checker: true if the target lies inside an object (`u.age`).
    pub in_object: bool,
    pub span: Span,
}

/// `for (x: T in iterable) body` or `for (a: T, b: U in iterable) body`.
#[derive(Debug, Clone, PartialEq)]
pub struct ForEach {
    pub vars: Vec<Param>,
    pub iterable: Expr,
    pub body: Box<Stmt>,
    /// Filled in by the type checker: the slot of each variable.
    pub slots: Vec<usize>,
    /// Filled in by the type checker: the slot of a hidden copy of the iterable.
    pub seq_slot: Option<usize>,
    /// Filled in by the type checker: what the loop runs over.
    pub over: Option<IterKind>,
    pub span: Span,
}

impl ForEach {
    /// Returns true for `for (a: T, b: U in ...)`, which gets the index or key and the value.
    pub fn binds_both(&self) -> bool {
        self.vars.len() == 2
    }
}

/// `match (subject) { Farbe.Rot => { ... } _ => { ... } }`.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub subject: Expr,
    pub arms: Vec<MatchArm>,
    /// Filled in by the type checker: for each variant, the number of its branch.
    pub arm_of_variant: Vec<usize>,
    /// Filled in by the type checker: true for a result, whose branches get the value inside it.
    pub unwraps: bool,
    pub span: Span,
}

impl Stmt {
    /// Returns the place of the first `this` in the statement, not counting declarations inside it.
    pub fn find_this(&self) -> Option<Span> {
        match self {
            Stmt::VarDecl { value, .. } => value.find_this(),
            Stmt::Expr { expr, .. } => expr.find_this(),
            Stmt::Assign(assign) => assign
                .target
                .find_this()
                .or_else(|| assign.value.find_this()),
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
            Stmt::ForEach(each) => each.iterable.find_this().or_else(|| each.body.find_this()),
            Stmt::Match(m) => m
                .subject
                .find_this()
                .or_else(|| m.arms.iter().find_map(|arm| arm.body.find_this())),
            Stmt::Return { value, .. } => value.as_ref().and_then(Expr::find_this),
            Stmt::Break { .. }
            | Stmt::Continue { .. }
            | Stmt::FnDecl(_)
            | Stmt::Import(_)
            | Stmt::StructDecl(_)
            | Stmt::ClassDecl(_)
            | Stmt::InterfaceDecl(_)
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
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub names: Vec<ImportName>,
    pub path: String,
    pub path_span: Span,
    /// Filled in by the loader: the number of the imported file.
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

/// `class Name { fields, constructor, methods }`, `exported` with `export` in front.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub exported: bool,
    pub is_abstract: bool,
    pub layout: Rc<ClassLayout>,
    pub name_span: Span,
    pub fields: Vec<ClassField>,
    /// The `static` fields, which belong to the class and not to its objects.
    pub static_fields: Vec<ClassField>,
    pub constructor: Option<Constructor>,
    /// The methods; each gets `this` as a hidden first parameter (slot 0).
    pub methods: Vec<Method>,
    /// The class after `extends`.
    pub extends: Option<(String, Span)>,
    /// The interfaces after `implements`.
    pub implements: Vec<(String, Span)>,
    /// Filled in by the type checker for the compiler.
    pub plan: ClassPlan,
}

/// What the type checker works out about a class for the compiler.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClassPlan {
    /// The names of all fields, those of the parent classes first.
    pub fields: Vec<String>,
    /// The position of the first own field (after the fields of the parent classes).
    pub field_offset: usize,
    /// How many parameters `init` has (the own constructor's or the inherited one's).
    pub params: usize,
    /// The `init` function of the parent class, if there is one.
    pub parent_init: Option<usize>,
    /// The number of the parent class, if there is one.
    pub parent_class: Option<usize>,
    /// For each method slot: the function the objects of this class run for it.
    pub methods: Vec<usize>,
    /// One vtable for each interface the class implements.
    pub vtables: Vec<VTable>,
    /// The global number of each static field, in the order of `ClassDecl.static_fields`.
    pub statics: Vec<usize>,
}

impl ClassPlan {
    /// Returns the position of an own field among all fields of the class.
    pub fn own_field(&self, position: usize) -> usize {
        self.field_offset + position
    }
}

/// The functions a class runs for the methods of one interface, in the order of the interface.
#[derive(Debug, Clone, PartialEq)]
pub struct VTable {
    pub interface: usize,
    pub functions: Vec<usize>,
}

/// `interface Name { method(params): type; ... }`, `exported` with `export` in front.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDecl {
    pub exported: bool,
    pub layout: Rc<InterfaceLayout>,
    pub name_span: Span,
    pub methods: Vec<InterfaceMethod>,
}

/// The name of an interface and of its methods in order, which is the identity of the interface.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceLayout {
    pub name: String,
    pub methods: Vec<String>,
}

/// One method of an interface: `name(params): type;`, without a body.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceMethod {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub return_type: TypeName,
}

/// Who may use a field or method: everyone, the class and its subclasses, or only the class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

impl Visibility {
    /// Returns the keyword of the visibility, e.g. `public`.
    pub fn keyword(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Private => "private",
        }
    }
}

/// A method of a class with its visibility, `static` (without `this`) or `override` in front.
#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub visibility: Visibility,
    pub is_static: bool,
    /// True for `abstract laut(): string;`, whose body is empty.
    pub is_abstract: bool,
    pub is_override: bool,
    pub function: Function,
}

/// `constructor(name: string) { ... }` of a class, with `this` in slot 0 before the parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct Constructor {
    pub visibility: Visibility,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// The name of the variable that holds the object in a constructor or method, a keyword no other variable can have.
pub const THIS: &str = "this";

/// The word that starts the constructor of a class, which is no keyword (like `from` in an import).
pub const CONSTRUCTOR: &str = "constructor";

/// The name of a class and of its fields in order, which is the identity of the class (see `types::Decl`).
#[derive(Debug, Clone, PartialEq)]
pub struct ClassLayout {
    pub name: String,
    pub fields: Vec<String>,
}

/// One field of a class, `name: type;` or `name: type = start;`, with its visibility and `readonly`.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassField {
    pub visibility: Visibility,
    pub readonly: bool,
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

/// The variant number of `ok` in a `match` over a result.
pub const OK_VARIANT: usize = 0;

/// The variant number of `err` in a `match` over a result.
pub const ERR_VARIANT: usize = 1;

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
    /// `ok(n: int)` or `err(e: string)`, whose variable (in `slot`) gets the value inside the result.
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

/// A type as written in the source, e.g. `list<int>?` with the name `list`, the argument `int` and `optional`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeName {
    pub name: String,
    pub args: Vec<TypeName>,
    pub optional: bool,
    pub span: Span,
}

/// Writes a function `name` that returns all `Stmt::Variant(Type)` declarations of a file in order.
macro_rules! declarations {
    ($(#[$doc:meta])* $name:ident, $variant:ident, $item:ty) => {
        $(#[$doc])*
        pub fn $name(stmts: &[Stmt]) -> impl Iterator<Item = &$item> {
            stmts.iter().filter_map(|stmt| match stmt {
                Stmt::$variant(decl) => Some(decl),
                _ => None,
            })
        }
    };
}

declarations!(
    /// Returns all functions of a file. Their order gives each function its number.
    functions, FnDecl, Function
);
declarations!(
    /// Returns all structs of a file. Their order gives each struct its number.
    structs, StructDecl, Rc<StructDecl>
);
declarations!(
    /// Returns all enums of a file.
    enums, EnumDecl, Rc<EnumDecl>
);
declarations!(
    /// Returns all classes of a file. Their order gives each class its number.
    classes, ClassDecl, ClassDecl
);
declarations!(
    /// Returns all interfaces of a file. Their order gives each interface its number.
    interfaces, InterfaceDecl, InterfaceDecl
);
