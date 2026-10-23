//! The type checker, which checks the types before the program runs and writes into the tree what the compiler needs.

use std::collections::{HashMap, HashSet};

use crate::ast::{
    ClassLayout, EnumDecl, Expr, InterfaceLayout, Param, StructDecl, VTable, Visibility,
};
use crate::error::{ErrorCode, SomoniError};
use crate::loader::SourceFile;
use crate::scope::Scopes;
use crate::span::Span;
use crate::types::{Decl, Type};

mod classes;
mod declarations;
mod expressions;
mod interfaces;
mod members;
mod modules;
mod null_checks;
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

impl Signature {
    /// Returns true if both signatures have the same parameter types and return type.
    fn same_types(&self, other: &Signature) -> bool {
        self.params == other.params && self.return_type == other.return_type
    }
}

/// The number and the fields of a struct.
#[derive(Clone)]
struct StructInfo {
    index: usize,
    fields: Vec<(String, Type)>,
}

/// What the type checker knows about a class, inherited members included.
#[derive(Clone)]
struct ClassInfo {
    fields: Vec<(String, Type)>,
    first_own_field: usize,
    parent: Option<Decl<ClassLayout>>,
    constructor: Signature,
    methods: HashMap<String, MethodInfo>,
    method_table: Vec<usize>,
    index: usize,
    members: HashMap<String, Member>,
    implements: Vec<Decl<InterfaceLayout>>,
    vtables: Vec<VTable>,
    statics: HashMap<String, StaticMethod>,
    constructor_visibility: Visibility,
    is_abstract: bool,
}

impl ClassInfo {
    /// Returns the info of a class whose fields and methods are not collected yet.
    fn new(layout: Decl<ClassLayout>, index: usize) -> ClassInfo {
        ClassInfo {
            fields: Vec::new(),
            first_own_field: 0,
            parent: None,
            constructor: Signature {
                index: 0,
                params: Vec::new(),
                return_type: Type::Class(layout),
            },
            methods: HashMap::new(),
            method_table: Vec::new(),
            index,
            members: HashMap::new(),
            implements: Vec::new(),
            vtables: Vec::new(),
            statics: HashMap::new(),
            constructor_visibility: Visibility::Public,
            is_abstract: false,
        }
    }
}

/// Who may use a field or method, which class declares it, and what it is.
#[derive(Clone)]
struct Member {
    visibility: Visibility,
    owner: Decl<ClassLayout>,
    kind: MemberKind,
}

/// What a member of a class is: a field, maybe `readonly`, or a method.
#[derive(Clone, Copy, PartialEq)]
enum MemberKind {
    Field { readonly: bool },
    Method,
}

/// The field of an object that an assignment changes, e.g. `x` in `l[0].x = 1`.
struct ObjectField {
    layout: Decl<ClassLayout>,
    name: String,
    span: Span,
    /// True if the object is `this`, as in `this.x = 1`.
    through_this: bool,
}

/// A static method of a class: who may call it and its signature.
#[derive(Clone)]
struct StaticMethod {
    visibility: Visibility,
    signature: Signature,
}

/// A method of a class with its slot in the method table.
#[derive(Clone)]
struct MethodInfo {
    slot: usize,
    signature: Signature,
    is_abstract: bool,
}

/// The number of an interface and its methods in order.
#[derive(Clone)]
struct InterfaceInfo {
    index: usize,
    methods: Vec<(String, Signature)>,
}

/// The names a file can use: its own declarations and its imports.
#[derive(Default)]
struct Names {
    functions: HashMap<String, Signature>,
    structs: HashMap<String, Decl<StructDecl>>,
    enums: HashMap<String, Decl<EnumDecl>>,
    classes: HashMap<String, Decl<ClassLayout>>,
    interfaces: HashMap<String, Decl<InterfaceLayout>>,
}

impl Names {
    /// Returns true if anything has this name.
    fn contains(&self, name: &str) -> bool {
        self.functions.contains_key(name) || self.type_named(name).is_some()
    }

    /// Returns the struct, class, interface or enum type with this name.
    fn type_named(&self, name: &str) -> Option<Type> {
        if let Some(decl) = self.structs.get(name) {
            Some(Type::Struct(decl.clone()))
        } else if let Some(layout) = self.classes.get(name) {
            Some(Type::Class(layout.clone()))
        } else if let Some(layout) = self.interfaces.get(name) {
            Some(Type::Interface(layout.clone()))
        } else {
            self.enums.get(name).cloned().map(Type::Enum)
        }
    }

    /// Copies everything called `name` from `from`.
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

/// The exported names of a file, and its private ones for clear errors.
#[derive(Default)]
struct Exports {
    names: Names,
    private: HashSet<String>,
}

/// Checks the types of a program and writes into the tree what the compiler needs.
pub struct TypeChecker {
    scopes: Scopes<VarInfo>,
    loop_depth: usize,
    names: Names,
    exports_by_file: HashMap<usize, Exports>,
    structs: HashMap<Decl<StructDecl>, StructInfo>,
    classes: HashMap<Decl<ClassLayout>, ClassInfo>,
    interfaces: HashMap<Decl<InterfaceLayout>, InterfaceInfo>,
    current_class: Option<Decl<ClassLayout>>,
    in_constructor: bool,
    super_init_allowed: bool,
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
            exports_by_file: HashMap::new(),
            structs: HashMap::new(),
            classes: HashMap::new(),
            interfaces: HashMap::new(),
            current_class: None,
            in_constructor: false,
            super_init_allowed: false,
            function_count: 0,
            current_return: None,
            next_slot: 0,
        }
    }

    /// Checks the whole program, every file after the files it imports.
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
            self.exports_by_file.insert(number, exports);
        }
        Ok(())
    }

    /// Runs `check` inside a new function with its own variables and return type.
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

    /// Declares the parameters of a function as const variables.
    fn declare_params(&mut self, params: &[Param], types: &[Type]) -> Result<(), SomoniError> {
        for (param, ty) in params.iter().zip(types) {
            self.declare(&param.name, param.name_span, ty.clone(), false)?;
        }
        Ok(())
    }

    /// Runs `check` inside a new block.
    fn in_scope<T>(&mut self, check: impl FnOnce(&mut Self) -> T) -> T {
        self.scopes.push();
        let first_slot = self.next_slot;
        let result = check(self);
        self.next_slot = first_slot;
        self.scopes.pop();
        result
    }

    /// Declares a new variable and returns its slot.
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
        self.check_fits(value, expected, |found| ErrorCode::CannotAssign {
            found,
            expected: expected.clone(),
        })
    }

    /// Checks `expr` against `expected` and returns the error from `wrong` if its type does not fit.
    fn check_fits(
        &mut self,
        expr: &mut Expr,
        expected: &Type,
        wrong: impl FnOnce(Type) -> ErrorCode,
    ) -> Result<(), SomoniError> {
        let found = self.check_expr_expecting(expr, expected)?;
        if !self.fits(&found, expected) {
            return Err(SomoniError::at(expr.span, wrong(found)));
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

    /// Returns true if a value of type `found` may be used where `expected` is wanted.
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

/// Returns the position and type of a field.
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

/// Returns the error for an argument with the wrong type.
fn wrong_arg_type(name: &str, found: Type, span: Span) -> SomoniError {
    SomoniError::at(
        span,
        ErrorCode::WrongArgType {
            name: name.to_string(),
            found,
        },
    )
}
