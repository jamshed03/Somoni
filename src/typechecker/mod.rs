//! The type checker: checks the types of a program before it runs and writes into the tree what
//! the compiler needs (slots, call targets, field positions, class plans).
//! The parts of `TypeChecker` are spread over the files of this folder, one task per file.

use std::collections::{HashMap, HashSet};

use crate::ast::{ClassLayout, EnumDecl, Expr, InterfaceLayout, Param, StructDecl, Visibility};
use crate::error::{ErrorCode, SomoniError};
use crate::loader::SourceFile;
use crate::scope::Scopes;
use crate::span::Span;
use crate::types::{Decl, Type};

mod classes;
mod declarations;
mod expressions;
mod interfaces;
mod modules;
mod patterns;
mod statements;

/// What the type checker knows about a variable.
struct VarInfo {
    ty: Type,
    mutable: bool,
    slot: usize,
}

/// The number, parameter types and return type of a function.
#[derive(Clone)]
struct Signature {
    index: usize,
    params: Vec<Type>,
    return_type: Type,
}

/// The number of a struct and the name and type of each field, in declaration order.
#[derive(Clone)]
struct StructInfo {
    index: usize,
    fields: Vec<(String, Type)>,
}

/// What the type checker knows about a class: its fields (those of the parent classes first),
/// its constructor (`init`), its methods (also the inherited ones) and who may use each name.
#[derive(Clone)]
struct ClassInfo {
    fields: Vec<(String, Type)>,
    /// The position of the first own field.
    field_offset: usize,
    parent: Option<Decl<ClassLayout>>,
    constructor: Signature,
    methods: HashMap<String, MethodInfo>,
    /// For each method slot: the function the objects of this class run.
    method_table: Vec<usize>,
    /// The number of the class (for `NewObject`).
    index: usize,
    /// Every field and method name, also inherited ones.
    members: HashMap<String, Member>,
    /// The interfaces it implements, and for each the functions of its methods (for the VM).
    implements: Vec<Decl<InterfaceLayout>>,
    vtables: Vec<(usize, Vec<usize>)>,
}

/// A field or method name of a class: who may use it and in which class it is declared.
#[derive(Clone)]
struct Member {
    visibility: Visibility,
    owner: Decl<ClassLayout>,
    is_method: bool,
}

/// A method of a class: its slot in the method table, and its signature (`index` is the
/// function this class runs for it).
#[derive(Clone)]
struct MethodInfo {
    slot: usize,
    signature: Signature,
}

/// The number of an interface and its methods in order; `index` of each signature is the
/// position of the method in the interface.
#[derive(Clone)]
struct InterfaceInfo {
    index: usize,
    methods: Vec<(String, Signature)>,
}

/// The functions, structs, enums and classes declared at the top level of a file, by name.
#[derive(Default)]
struct Names {
    functions: HashMap<String, Signature>,
    structs: HashMap<String, Decl<StructDecl>>,
    enums: HashMap<String, Decl<EnumDecl>>,
    classes: HashMap<String, Decl<ClassLayout>>,
    interfaces: HashMap<String, Decl<InterfaceLayout>>,
}

impl Names {
    /// Returns true if a function, struct, enum or class has this name.
    fn contains(&self, name: &str) -> bool {
        self.functions.contains_key(name)
            || self.structs.contains_key(name)
            || self.enums.contains_key(name)
            || self.classes.contains_key(name)
            || self.interfaces.contains_key(name)
    }

    /// Copies the function, struct, enum, class and interface called `name` from `from`, if any.
    fn add_from(&mut self, from: &Names, name: &str) {
        if let Some(signature) = from.functions.get(name) {
            self.functions.insert(name.to_string(), signature.clone());
        }
        if let Some(decl) = from.structs.get(name) {
            self.structs.insert(name.to_string(), decl.clone());
        }
        if let Some(decl) = from.enums.get(name) {
            self.enums.insert(name.to_string(), decl.clone());
        }
        if let Some(layout) = from.classes.get(name) {
            self.classes.insert(name.to_string(), layout.clone());
        }
        if let Some(layout) = from.interfaces.get(name) {
            self.interfaces.insert(name.to_string(), layout.clone());
        }
    }
}

/// What a checked file offers to the files that import it: its exported names,
/// and the names of its other declarations (for a clear error if someone imports one).
#[derive(Default)]
struct Exports {
    names: Names,
    private: HashSet<String>,
}

/// Checks the types of a program before it runs.
/// It also writes into the tree what the compiler needs: slots, call targets, field positions.
pub struct TypeChecker {
    scopes: Scopes<VarInfo>,
    loop_depth: usize,
    /// The names of the file being checked: its own and the imported ones.
    names: Names,
    /// What each checked file exports, by file number.
    exports: HashMap<usize, Exports>,
    structs: HashMap<Decl<StructDecl>, StructInfo>,
    classes: HashMap<Decl<ClassLayout>, ClassInfo>,
    interfaces: HashMap<Decl<InterfaceLayout>, InterfaceInfo>,
    /// The class whose start values, constructor or methods are being checked.
    current_class: Option<Decl<ClassLayout>>,
    /// True while the first statement of the constructor of a subclass is checked:
    /// only there `super(...)` is allowed.
    super_init_allowed: bool,
    /// How many functions the files checked so far have; gives each function its number.
    function_count: usize,
    current_return: Option<Type>,
    next_slot: usize,
}

impl TypeChecker {
    pub fn new() -> TypeChecker {
        TypeChecker {
            scopes: Scopes::new(),
            loop_depth: 0,
            names: Names::default(),
            exports: HashMap::new(),
            structs: HashMap::new(),
            classes: HashMap::new(),
            interfaces: HashMap::new(),
            current_class: None,
            super_init_allowed: false,
            function_count: 0,
            current_return: None,
            next_slot: 0,
        }
    }

    /// Checks the whole program, one file after the other in `order`
    /// (every file after the files it imports).
    pub fn check(&mut self, files: &mut [SourceFile], order: &[usize]) -> Result<(), SomoniError> {
        for &number in order {
            let stmts = &mut files[number].stmts;
            self.names = Names::default();
            self.import_names(stmts)?;
            self.declare_enums(stmts)?;
            self.declare_structs(stmts)?;
            self.declare_functions(stmts)?;
            self.declare_class_functions(stmts)?;
            self.check_stmts(stmts)?;
            let exports = self.exports_of(stmts);
            self.exports.insert(number, exports);
        }
        Ok(())
    }

    /// Runs `check` like inside a new function: no variables from outside, slots from 0,
    /// and `return` must give `return_type` (None: `return` is not allowed).
    fn in_function<T>(
        &mut self,
        return_type: Option<Type>,
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let outer_scopes = std::mem::replace(&mut self.scopes, Scopes::new());
        let outer_return = std::mem::replace(&mut self.current_return, return_type);
        let outer_next_slot = std::mem::replace(&mut self.next_slot, 0);
        let result = check(self);
        self.scopes = outer_scopes;
        self.current_return = outer_return;
        self.next_slot = outer_next_slot;
        result
    }

    /// Declares the parameters of a function, method or constructor as const variables.
    fn declare_params(&mut self, params: &[Param], types: &[Type]) -> Result<(), SomoniError> {
        for (param, ty) in params.iter().zip(types) {
            self.declare(&param.name, param.name_span, ty.clone(), false)?;
        }
        Ok(())
    }

    /// Runs `check` inside a new block. Afterwards the variables of the block are gone.
    fn in_scope<T>(&mut self, check: impl FnOnce(&mut Self) -> T) -> T {
        self.scopes.push();
        let first_slot = self.next_slot;
        let result = check(self);
        self.next_slot = first_slot;
        self.scopes.pop();
        result
    }

    /// Declares a new variable and returns its slot. A name may exist only once per block.
    fn declare(
        &mut self,
        name: &str,
        name_span: Span,
        ty: Type,
        mutable: bool,
    ) -> Result<usize, SomoniError> {
        let slot = self.next_slot;
        let info = VarInfo { ty, mutable, slot };
        if !self.scopes.declare(name, info) {
            return Err(SomoniError::at(
                name_span,
                ErrorCode::AlreadyDeclared {
                    name: name.to_string(),
                },
            ));
        }
        self.next_slot += 1;
        Ok(slot)
    }

    /// Checks that `value` has the type `expected`.
    fn check_assignable(&mut self, value: &mut Expr, expected: &Type) -> Result<(), SomoniError> {
        let found = self.check_expr_expecting(value, expected)?;
        if !self.fits(&found, expected) {
            return Err(SomoniError::at(
                value.span,
                ErrorCode::CannotAssign {
                    found,
                    expected: expected.clone(),
                },
            ));
        }
        Ok(())
    }

    /// Checks the arguments of a call of `name` against the types of its parameters.
    fn check_args(
        &mut self,
        name: &str,
        params: &[Type],
        args: &mut [Expr],
        call_span: Span,
    ) -> Result<(), SomoniError> {
        check_arg_count(name, params.len(), args, call_span)?;
        for (arg, param_type) in args.iter_mut().zip(params) {
            let arg_type = self.check_expr_expecting(arg, param_type)?;
            if !self.fits(&arg_type, param_type) {
                return Err(wrong_arg_type(name, arg_type, arg.span));
            }
        }
        Ok(())
    }

    /// Returns true if a value of type `found` may be used where `expected` is wanted:
    /// the same type, `T` where `T?` is wanted, or an object of a class where an interface
    /// it implements is wanted (also inside `?`: a `Kreis` fits a `Form?`).
    fn fits(&self, found: &Type, expected: &Type) -> bool {
        if found == expected {
            return true;
        }
        match (found, expected) {
            (_, Type::Optional(inner)) => self.fits(found, inner),
            (Type::Class(class), Type::Class(ancestor)) => self.is_subclass(class, ancestor),
            (Type::Class(layout), Type::Interface(interface)) => self
                .classes
                .get(layout)
                .is_some_and(|info| info.implements.contains(interface)),
            _ => false,
        }
    }
}

/// Returns the position and type of a field, or T027 if the struct has no such field.
fn field_of<'a>(
    fields: &'a [(String, Type)],
    struct_name: &str,
    name: &str,
    span: Span,
) -> Result<(usize, &'a Type), SomoniError> {
    fields
        .iter()
        .enumerate()
        .find(|(_, (field, _))| field == name)
        .map(|(position, (_, ty))| (position, ty))
        .ok_or_else(|| {
            SomoniError::at(
                span,
                ErrorCode::UnknownField {
                    struct_name: struct_name.to_string(),
                    name: name.to_string(),
                },
            )
        })
}

/// Checks that a call has the right number of arguments.
fn check_arg_count(
    name: &str,
    expected: usize,
    args: &[Expr],
    call_span: Span,
) -> Result<(), SomoniError> {
    if args.len() != expected {
        return Err(SomoniError::at(
            call_span,
            ErrorCode::WrongArgCount {
                name: name.to_string(),
                expected,
                found: args.len(),
            },
        ));
    }
    Ok(())
}

/// The error for an argument with the wrong type.
fn wrong_arg_type(name: &str, found: Type, span: Span) -> SomoniError {
    SomoniError::at(
        span,
        ErrorCode::WrongArgType {
            name: name.to_string(),
            found,
        },
    )
}
