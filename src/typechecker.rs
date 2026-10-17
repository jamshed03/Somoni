use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{
    self, BinaryOp, CallTarget, ClassDecl, ClassField, ClassLayout, ClassPlan, EnumDecl, Expr,
    ExprKind, Field, FieldInit, Function, Import, InterfaceLayout, IterKind, MatchArm,
    MethodTarget, Param, Pattern, Stmt, StructDecl, THIS, TypeName, UnaryOp, Visibility,
};
use crate::builtins::Builtin;
use crate::error::{ErrorCode, SomoniError};
use crate::loader::SourceFile;
use crate::scope::Scopes;
use crate::span::Span;
use crate::types::{Decl, Type};

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

    /// Adds the names of all `import`s of a file to `names`.
    fn import_names(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for stmt in stmts {
            if let Stmt::Import(import) = stmt {
                self.import(import)?;
            }
        }
        Ok(())
    }

    /// Adds the names of one `import { ... } from "..."`: each must be exported by that file
    /// and must not be imported twice.
    fn import(&mut self, import: &Import) -> Result<(), SomoniError> {
        let exports = import
            .file
            .and_then(|file| self.exports.get(&file))
            .ok_or_else(|| SomoniError::internal("import of a file that is not checked"))?;
        for item in &import.names {
            if self.names.contains(&item.name) {
                return Err(SomoniError::at(
                    item.span,
                    ErrorCode::ImportConflict {
                        name: item.name.clone(),
                    },
                ));
            }
            let from = &exports.names;
            if !from.contains(&item.name) {
                let code = if exports.private.contains(&item.name) {
                    ErrorCode::NotExported {
                        name: item.name.clone(),
                        path: import.path.clone(),
                    }
                } else {
                    ErrorCode::NotInFile {
                        name: item.name.clone(),
                        path: import.path.clone(),
                    }
                };
                return Err(SomoniError::at(item.span, code));
            }
            self.names.add_from(from, &item.name);
        }
        Ok(())
    }

    /// Collects what a checked file exports: its own declarations with `export`.
    fn exports_of(&self, stmts: &[Stmt]) -> Exports {
        let mut exports = Exports::default();
        for stmt in stmts {
            let (name, exported) = match stmt {
                Stmt::FnDecl(function) => (&function.name, function.exported),
                Stmt::StructDecl(decl) => (&decl.name, decl.exported),
                Stmt::EnumDecl(decl) => (&decl.name, decl.exported),
                Stmt::ClassDecl(class) => (&class.layout.name, class.exported),
                Stmt::InterfaceDecl(decl) => (&decl.layout.name, decl.exported),
                _ => continue,
            };
            if !exported {
                exports.private.insert(name.clone());
                continue;
            }
            exports.names.add_from(&self.names, name);
        }
        exports
    }

    fn check_stmts(&mut self, stmts: &mut [Stmt]) -> Result<(), SomoniError> {
        for stmt in stmts {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    /// Collects all enums. Each needs a free name and different variants.
    fn declare_enums(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for decl in ast::enums(stmts) {
            if self.is_type_name(&decl.name) {
                return Err(SomoniError::at(
                    decl.name_span,
                    ErrorCode::DuplicateType {
                        name: decl.name.clone(),
                    },
                ));
            }
            if decl.variants.is_empty() {
                return Err(SomoniError::at(decl.name_span, ErrorCode::EmptyEnum));
            }
            for (i, (name, span)) in decl.variants.iter().enumerate() {
                if decl.variants[..i].iter().any(|(other, _)| other == name) {
                    return Err(SomoniError::at(
                        *span,
                        ErrorCode::DuplicateVariant { name: name.clone() },
                    ));
                }
            }
            self.names
                .enums
                .insert(decl.name.clone(), Decl(Rc::clone(decl)));
        }
        Ok(())
    }

    /// Returns true if `name` is already a type: built in, a struct, an enum or a class.
    fn is_type_name(&self, name: &str) -> bool {
        Type::is_builtin_name(name)
            || self.names.structs.contains_key(name)
            || self.names.enums.contains_key(name)
            || self.names.classes.contains_key(name)
            || self.names.interfaces.contains_key(name)
    }

    /// Returns T025 if `name` is already a type.
    fn check_new_type_name(&self, name: &str, span: Span) -> Result<(), SomoniError> {
        if self.is_type_name(name) {
            return Err(SomoniError::at(
                span,
                ErrorCode::DuplicateType {
                    name: name.to_string(),
                },
            ));
        }
        Ok(())
    }

    /// Collects all structs, classes and interfaces: first their names, then their field and
    /// method types, then checks that no struct or class contains itself.
    fn declare_structs(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for decl in ast::structs(stmts) {
            self.check_new_type_name(&decl.name, decl.name_span)?;
            let info = StructInfo {
                index: self.structs.len(),
                fields: Vec::new(),
            };
            let decl = Decl(Rc::clone(decl));
            self.names.structs.insert(decl.name.clone(), decl.clone());
            self.structs.insert(decl, info);
        }
        for class in ast::classes(stmts) {
            self.check_new_type_name(&class.layout.name, class.name_span)?;
            let layout = Decl(Rc::clone(&class.layout));
            let info = ClassInfo {
                fields: Vec::new(),
                field_offset: 0,
                parent: None,
                constructor: Signature {
                    index: 0,
                    params: Vec::new(),
                    return_type: Type::Class(layout.clone()),
                },
                methods: HashMap::new(),
                method_table: Vec::new(),
                index: self.classes.len(),
                members: HashMap::new(),
                implements: Vec::new(),
                vtables: Vec::new(),
            };
            self.names
                .classes
                .insert(layout.name.clone(), layout.clone());
            self.classes.insert(layout, info);
        }

        for decl in ast::interfaces(stmts) {
            self.check_new_type_name(&decl.layout.name, decl.name_span)?;
            let layout = Decl(Rc::clone(&decl.layout));
            let info = InterfaceInfo {
                index: self.interfaces.len(),
                methods: Vec::new(),
            };
            self.names
                .interfaces
                .insert(layout.name.clone(), layout.clone());
            self.interfaces.insert(layout, info);
        }

        for decl in ast::interfaces(stmts) {
            let mut methods: Vec<(String, Signature)> = Vec::new();
            for (position, method) in decl.methods.iter().enumerate() {
                if methods.iter().any(|(name, _)| *name == method.name) {
                    return Err(SomoniError::at(
                        method.name_span,
                        ErrorCode::DuplicateMember {
                            class: decl.layout.name.clone(),
                            name: method.name.clone(),
                        },
                    ));
                }
                let mut params = Vec::new();
                for param in &method.params {
                    params.push(self.resolve_type(&param.type_name, false)?);
                }
                let signature = Signature {
                    index: position,
                    params,
                    return_type: self.resolve_type(&method.return_type, true)?,
                };
                methods.push((method.name.clone(), signature));
            }
            if let Some(info) = self.interfaces.get_mut(&Decl(Rc::clone(&decl.layout))) {
                info.methods = methods;
            }
        }

        for decl in ast::structs(stmts) {
            let fields = self.resolve_fields(&decl.fields)?;
            if let Some(info) = self.structs.get_mut(&Decl(Rc::clone(decl))) {
                info.fields = fields;
            }
        }
        self.resolve_parents(stmts)?;
        for class in self.parents_first(stmts) {
            self.declare_class_fields(class)?;
        }

        for decl in ast::structs(stmts) {
            let ty = Type::Struct(Decl(Rc::clone(decl)));
            self.check_not_contains_itself(&ty, &decl.fields)?;
        }
        for class in ast::classes(stmts) {
            let ty = Type::Class(Decl(Rc::clone(&class.layout)));
            self.check_not_contains_itself(&ty, class.fields.iter().map(|f| &f.field))?;
        }
        Ok(())
    }

    /// Finds the parent class after `extends` of every class of a file: it must be a class
    /// (T064), and no class may inherit from itself (T065).
    fn resolve_parents(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for class in ast::classes(stmts) {
            let Some((name, span)) = &class.extends else {
                continue;
            };
            let Some(parent) = self.names.classes.get(name).cloned() else {
                let code = if self.is_type_name(name) {
                    ErrorCode::ExtendsNotClass { name: name.clone() }
                } else {
                    ErrorCode::UnknownType { name: name.clone() }
                };
                return Err(SomoniError::at(*span, code));
            };
            let layout = Decl(Rc::clone(&class.layout));
            if let Some(info) = self.classes.get_mut(&layout) {
                info.parent = Some(parent);
            }
        }
        for class in ast::classes(stmts) {
            let layout = Decl(Rc::clone(&class.layout));
            let mut current = self.parent_of(&layout);
            for _ in 0..self.classes.len() {
                let Some(ancestor) = current else {
                    break;
                };
                if ancestor == layout {
                    let span = class.extends.as_ref().map_or(class.name_span, |(_, s)| *s);
                    return Err(SomoniError::at(
                        span,
                        ErrorCode::InheritanceCycle {
                            name: layout.name.clone(),
                        },
                    ));
                }
                current = self.parent_of(&ancestor);
            }
        }
        Ok(())
    }

    /// Returns the parent class of a class, if it has one.
    fn parent_of(&self, layout: &Decl<ClassLayout>) -> Option<Decl<ClassLayout>> {
        self.classes
            .get(layout)
            .and_then(|info| info.parent.clone())
    }

    /// Returns true if `class` is `ancestor` or inherits from it, also over several steps.
    fn is_subclass(&self, class: &Decl<ClassLayout>, ancestor: &Decl<ClassLayout>) -> bool {
        let mut current = Some(class.clone());
        while let Some(layout) = current {
            if layout == *ancestor {
                return true;
            }
            current = self.parent_of(&layout);
        }
        false
    }

    /// Returns the classes of a file so that every class comes after its parent class
    /// (sorted by the number of parent classes above it).
    fn parents_first<'a>(&self, stmts: &'a [Stmt]) -> Vec<&'a ClassDecl> {
        let depth = |class: &&ClassDecl| {
            let mut count = 0;
            let mut current = self.parent_of(&Decl(Rc::clone(&class.layout)));
            while let Some(layout) = current {
                count += 1;
                current = self.parent_of(&layout);
            }
            count
        };
        let mut classes: Vec<&ClassDecl> = ast::classes(stmts).collect();
        classes.sort_by_key(depth);
        classes
    }

    /// Collects the fields of a class: those of its parent class first, then its own.
    fn declare_class_fields(&mut self, class: &ClassDecl) -> Result<(), SomoniError> {
        let layout = Decl(Rc::clone(&class.layout));
        let mut fields = match self.parent_of(&layout) {
            Some(parent) => self.class_info(&parent)?.fields.clone(),
            None => Vec::new(),
        };
        let field_offset = fields.len();
        fields.extend(self.resolve_fields(class.fields.iter().map(|f| &f.field))?);
        if let Some(info) = self.classes.get_mut(&layout) {
            info.fields = fields;
            info.field_offset = field_offset;
        }
        Ok(())
    }

    /// Returns the name and type of each field. A name may appear only once (T026).
    fn resolve_fields<'a>(
        &self,
        fields: impl IntoIterator<Item = &'a Field>,
    ) -> Result<Vec<(String, Type)>, SomoniError> {
        let mut resolved: Vec<(String, Type)> = Vec::new();
        for field in fields {
            if resolved.iter().any(|(name, _)| *name == field.name) {
                return Err(SomoniError::at(
                    field.name_span,
                    ErrorCode::DuplicateField {
                        name: field.name.clone(),
                    },
                ));
            }
            let ty = self.resolve_type(&field.type_name, false)?;
            resolved.push((field.name.clone(), ty));
        }
        Ok(resolved)
    }

    /// Returns T030 if a field of the struct or class `ty` contains `ty` itself.
    fn check_not_contains_itself<'a>(
        &self,
        ty: &Type,
        fields: impl IntoIterator<Item = &'a Field>,
    ) -> Result<(), SomoniError> {
        let Some(types) = self.fields_of(ty) else {
            return Ok(());
        };
        for (field, (_, field_type)) in fields.into_iter().zip(types) {
            if self.contains(field_type, ty, &mut Vec::new()) {
                return Err(SomoniError::at(
                    field.type_name.span,
                    ErrorCode::StructContainsItself {
                        name: ty.to_string(),
                    },
                ));
            }
        }
        Ok(())
    }

    /// Returns true if a value of type `from` contains a `target` in a field, also through other
    /// structs and classes. Fields of type `list<...>` or `T?` do not count.
    fn contains(&self, from: &Type, target: &Type, seen: &mut Vec<Type>) -> bool {
        if from == target {
            return true;
        }
        if seen.contains(from) {
            return false;
        }
        seen.push(from.clone());
        let Some(fields) = self.fields_of(from) else {
            return false;
        };
        fields.iter().any(|(_, ty)| self.contains(ty, target, seen))
    }

    /// Returns the name and type of each field of a struct or class type, None for other types.
    fn fields_of(&self, ty: &Type) -> Option<&[(String, Type)]> {
        match ty {
            Type::Struct(decl) => self.structs.get(decl).map(|info| info.fields.as_slice()),
            Type::Class(layout) => self.classes.get(layout).map(|info| info.fields.as_slice()),
            _ => None,
        }
    }

    /// Collects the signatures of all functions, so they can be called before they are declared.
    fn declare_functions(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for function in ast::functions(stmts) {
            let name = &function.name;
            if Builtin::from_name(name).is_some() {
                return Err(SomoniError::at(
                    function.name_span,
                    ErrorCode::BuiltinRedeclared { name: name.clone() },
                ));
            }
            if self.names.functions.contains_key(name) {
                return Err(SomoniError::at(
                    function.name_span,
                    ErrorCode::DuplicateFunction { name: name.clone() },
                ));
            }
            let signature = self.signature_of(function, self.function_count)?;
            self.names.functions.insert(name.clone(), signature);
            self.function_count += 1;
        }
        Ok(())
    }

    /// Gives every class its function numbers, after the functions of the file (the compiler
    /// writes them in the same order): `init Name` (the constructor), then each own method.
    /// Then, parents first, builds the method table of each class: the inherited one, with
    /// `override` methods in the slot of the method they replace and new methods at the end.
    fn declare_class_functions(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        let mut numbers: HashMap<Decl<ClassLayout>, (usize, Vec<usize>)> = HashMap::new();
        for class in ast::classes(stmts) {
            let init = self.function_count;
            let methods = (1..=class.methods.len())
                .map(|i| self.function_count + i)
                .collect();
            self.function_count += 1 + class.methods.len();
            numbers.insert(Decl(Rc::clone(&class.layout)), (init, methods));
        }
        for class in self.parents_first(stmts) {
            let layout = Decl(Rc::clone(&class.layout));
            let (init, functions) = numbers
                .remove(&layout)
                .ok_or_else(|| SomoniError::internal("class without numbers"))?;
            self.declare_class_methods(class, init, &functions)?;
        }
        Ok(())
    }

    /// Builds the constructor signature, the members, the methods and the method table of one
    /// class whose parent class is already done. `init` and `functions` are its function numbers.
    /// A name of the parent class may not come again as a field or method (T055), except as
    /// `override` of a method that is not private.
    fn declare_class_methods(
        &mut self,
        class: &ClassDecl,
        init: usize,
        functions: &[usize],
    ) -> Result<(), SomoniError> {
        let layout = Decl(Rc::clone(&class.layout));
        let info = self.class_info(&layout)?.clone();
        let parent = self.parent_info(&info)?;
        let params = match &class.constructor {
            Some(constructor) => {
                let mut params = Vec::new();
                for param in &constructor.params {
                    params.push(self.resolve_type(&param.type_name, false)?);
                }
                params
            }
            None => parent
                .as_ref()
                .map_or(Vec::new(), |p| p.constructor.params.clone()),
        };
        let (mut methods, mut table) = match &parent {
            Some(parent) => (parent.methods.clone(), parent.method_table.clone()),
            None => (HashMap::new(), Vec::new()),
        };
        let mut members = parent
            .as_ref()
            .map_or(HashMap::new(), |p| p.members.clone());
        for field in &class.fields {
            if members.contains_key(&field.field.name) {
                return Err(SomoniError::at(
                    field.field.name_span,
                    ErrorCode::DuplicateMember {
                        class: layout.name.clone(),
                        name: field.field.name.clone(),
                    },
                ));
            }
            let member = Member {
                visibility: field.visibility,
                owner: layout.clone(),
                is_method: false,
            };
            members.insert(field.field.name.clone(), member);
        }
        for (method, &function) in class.methods.iter().zip(functions) {
            let name = &method.function.name;
            let span = method.function.name_span;
            let signature = self.signature_of(&method.function, function)?;
            let duplicate = || {
                SomoniError::at(
                    span,
                    ErrorCode::DuplicateMember {
                        class: layout.name.clone(),
                        name: name.clone(),
                    },
                )
            };
            let slot = match members.get(name) {
                Some(member)
                    if !member.is_method
                        || member.visibility == Visibility::Private
                        || member.owner == layout =>
                {
                    return Err(duplicate());
                }
                Some(member) => {
                    let inherited = methods.get(name).ok_or_else(duplicate)?;
                    let same_types = inherited.signature.params == signature.params
                        && inherited.signature.return_type == signature.return_type;
                    if !method.is_override {
                        return Err(SomoniError::at(
                            span,
                            ErrorCode::MissingOverride {
                                method: name.clone(),
                                parent: member.owner.name.clone(),
                            },
                        ));
                    }
                    if !same_types || member.visibility != method.visibility {
                        return Err(SomoniError::at(
                            span,
                            ErrorCode::OverrideMismatch {
                                parent: member.owner.name.clone(),
                                method: describe_method(
                                    name,
                                    member.visibility,
                                    &inherited.signature,
                                ),
                            },
                        ));
                    }
                    inherited.slot
                }
                None if method.is_override => {
                    return Err(SomoniError::at(
                        span,
                        ErrorCode::NothingToOverride {
                            method: name.clone(),
                        },
                    ));
                }
                None => {
                    table.push(function);
                    table.len() - 1
                }
            };
            table[slot] = function;
            methods.insert(name.clone(), MethodInfo { slot, signature });
            let member = Member {
                visibility: method.visibility,
                owner: layout.clone(),
                is_method: true,
            };
            members.insert(name.clone(), member);
        }
        let mut implements = parent.as_ref().map_or(Vec::new(), |p| p.implements.clone());
        for interface in self.check_implements(class, &methods, &members)? {
            if !implements.contains(&interface) {
                implements.push(interface);
            }
        }
        let mut vtables = Vec::new();
        for interface in &implements {
            vtables.push(self.vtable(interface, &methods, &table)?);
        }
        if let Some(info) = self.classes.get_mut(&layout) {
            info.constructor.index = init;
            info.constructor.params = params;
            info.methods = methods;
            info.method_table = table;
            info.members = members;
            info.implements = implements;
            info.vtables = vtables;
        }
        Ok(())
    }

    /// Checks that a class has every method of every interface after `implements`: public and
    /// with the same types (rule 22 of `Klassen.md`); inherited methods count. Returns the
    /// interfaces.
    fn check_implements(
        &self,
        class: &ClassDecl,
        methods: &HashMap<String, MethodInfo>,
        members: &HashMap<String, Member>,
    ) -> Result<Vec<Decl<InterfaceLayout>>, SomoniError> {
        let mut implements = Vec::new();
        for (name, span) in &class.implements {
            let Some(interface) = self.names.interfaces.get(name).cloned() else {
                let code = if self.is_type_name(name) {
                    ErrorCode::NotAnInterface { name: name.clone() }
                } else {
                    ErrorCode::UnknownType { name: name.clone() }
                };
                return Err(SomoniError::at(*span, code));
            };
            for (method_name, wanted) in &self.interface_info(&interface)?.methods {
                let (Some(found), Some(member)) =
                    (methods.get(method_name), members.get(method_name))
                else {
                    return Err(SomoniError::at(
                        *span,
                        ErrorCode::MissingInterfaceMethod {
                            interface: interface.name.clone(),
                            method: method_name.clone(),
                        },
                    ));
                };
                let place = class
                    .methods
                    .iter()
                    .find(|m| m.function.name == *method_name)
                    .map_or(*span, |m| m.function.name_span);
                if member.visibility != Visibility::Public {
                    return Err(SomoniError::at(
                        place,
                        ErrorCode::PrivateInterfaceMethod {
                            interface: interface.name.clone(),
                            method: method_name.clone(),
                        },
                    ));
                }
                let found = &found.signature;
                if found.params != wanted.params || found.return_type != wanted.return_type {
                    return Err(SomoniError::at(
                        place,
                        ErrorCode::InterfaceMethodType {
                            interface: interface.name.clone(),
                            method: describe_method(method_name, Visibility::Public, wanted)
                                .trim_start_matches("public ")
                                .to_string(),
                        },
                    ));
                }
            }
            implements.push(interface);
        }
        Ok(implements)
    }

    /// Returns the vtable of a class for an interface: the number of the interface and, for each
    /// of its methods, the function the class runs for it.
    fn vtable(
        &self,
        interface: &Decl<InterfaceLayout>,
        methods: &HashMap<String, MethodInfo>,
        table: &[usize],
    ) -> Result<(usize, Vec<usize>), SomoniError> {
        let info = self.interface_info(interface)?;
        let mut functions = Vec::new();
        for (name, _) in &info.methods {
            let method = methods
                .get(name)
                .ok_or_else(|| SomoniError::internal("interface method missing"))?;
            functions.push(table[method.slot]);
        }
        Ok((info.index, functions))
    }

    /// Returns the number and the methods of an interface.
    fn interface_info(
        &self,
        layout: &Decl<InterfaceLayout>,
    ) -> Result<&InterfaceInfo, SomoniError> {
        self.interfaces
            .get(layout)
            .ok_or_else(|| SomoniError::internal("interface without methods"))
    }

    /// Returns the signature of a function or method with the number `index`.
    fn signature_of(&self, function: &Function, index: usize) -> Result<Signature, SomoniError> {
        let mut params = Vec::new();
        for param in &function.params {
            params.push(self.resolve_type(&param.type_name, false)?);
        }
        Ok(Signature {
            index,
            params,
            return_type: self.resolve_type(&function.return_type, true)?,
        })
    }

    /// Checks a function body with its own variables, and that it returns on every path.
    fn check_function(&mut self, function: &mut Function) -> Result<(), SomoniError> {
        let signature = self.names.functions[&function.name].clone();
        let name = function.name.clone();
        self.check_body(function, &name, &signature, None)
    }

    /// Checks a method of a class: like a function, with `this` as slot 0 before the parameters.
    fn check_method(
        &mut self,
        method: &mut Function,
        layout: &Decl<ClassLayout>,
        signature: &Signature,
    ) -> Result<(), SomoniError> {
        let name = format!("{}.{}", layout.name, method.name);
        self.check_body(method, &name, signature, Some(Type::Class(layout.clone())))
    }

    /// Checks the body of a function or method with its own variables, and that it returns on
    /// every path. With `this_type`, the hidden variable `this` comes first.
    fn check_body(
        &mut self,
        function: &mut Function,
        name: &str,
        signature: &Signature,
        this_type: Option<Type>,
    ) -> Result<(), SomoniError> {
        let return_type = signature.return_type.clone();
        self.in_function(Some(return_type.clone()), |this| {
            if let Some(ty) = this_type {
                this.declare(THIS, function.name_span, ty, false)?;
            }
            this.declare_params(&function.params, &signature.params)?;
            this.check_stmts(&mut function.body)
        })?;

        if return_type != Type::Void && !always_returns(&function.body) {
            return Err(SomoniError::at(
                function.return_type.span,
                ErrorCode::MissingReturn {
                    name: name.to_string(),
                    expected: return_type,
                },
            ));
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

    /// Checks the start values and the constructor of a class, and that every field gets a value.
    /// Start values see no variables, like the body of a function without parameters.
    /// Private fields and methods may be used inside, see `check_access`.
    fn check_class(&mut self, class: &mut ClassDecl) -> Result<(), SomoniError> {
        let layout = Decl(Rc::clone(&class.layout));
        let outer_class = self.current_class.replace(layout.clone());
        let result = self.check_class_parts(class, layout);
        self.current_class = outer_class;
        result
    }

    /// Checks the parts of a class for `check_class`.
    fn check_class_parts(
        &mut self,
        class: &mut ClassDecl,
        layout: Decl<ClassLayout>,
    ) -> Result<(), SomoniError> {
        let info = self.class_info(&layout)?.clone();
        let parent = self.parent_info(&info)?;
        class.plan = ClassPlan {
            fields: info.fields.iter().map(|(name, _)| name.clone()).collect(),
            field_offset: info.field_offset,
            params: info.constructor.params.len(),
            parent_init: parent.as_ref().map(|p| p.constructor.index),
            methods: info.method_table.clone(),
            vtables: info.vtables.clone(),
        };
        let own_fields = &info.fields[info.field_offset..];
        self.in_function(None, |this| {
            for (field, (_, ty)) in class.fields.iter_mut().zip(own_fields) {
                if let Some(start) = &mut field.start {
                    this.check_assignable(start, ty)?;
                }
            }
            Ok::<(), SomoniError>(())
        })?;
        if let Some(constructor) = &mut class.constructor {
            let params = &info.constructor.params;
            let has_parent = parent.is_some();
            if has_parent && !starts_with_super(&constructor.body) {
                return Err(SomoniError::at(
                    constructor.span,
                    ErrorCode::MissingSuper {
                        class: layout.name.clone(),
                    },
                ));
            }
            self.in_function(Some(Type::Void), |this| {
                let this_type = Type::Class(layout.clone());
                this.declare(THIS, constructor.span, this_type, false)?;
                this.declare_params(&constructor.params, params)?;
                for (i, stmt) in constructor.body.iter_mut().enumerate() {
                    this.super_init_allowed = has_parent && i == 0;
                    let result = this.check_stmt(stmt);
                    this.super_init_allowed = false;
                    result?;
                }
                Ok::<(), SomoniError>(())
            })?;
        }
        for method in class.methods.iter_mut().map(|m| &mut m.function) {
            let signature = info
                .methods
                .get(&method.name)
                .map(|m| &m.signature)
                .ok_or_else(|| SomoniError::internal("method without signature"))?;
            self.check_method(method, &layout, signature)?;
        }
        let body = class
            .constructor
            .as_ref()
            .map_or(&[][..], |constructor| &constructor.body);
        check_fields_set(class, body)
    }

    /// Returns T058 if `name` is a private field or method of the class `layout` (or one it
    /// inherits) and the code being checked is not inside the class that declares it
    /// (rule 17 of `Klassen.md`); T072 if it is protected and the code is not in that class or a
    /// subclass (rule 34).
    fn check_access(
        &self,
        layout: &Decl<ClassLayout>,
        name: &str,
        span: Span,
    ) -> Result<(), SomoniError> {
        let Some(member) = self.class_info(layout)?.members.get(name) else {
            return Ok(());
        };
        let inside = |allowed: &dyn Fn(&Decl<ClassLayout>) -> bool| {
            self.current_class.as_ref().is_some_and(allowed)
        };
        let code = match member.visibility {
            Visibility::Public => return Ok(()),
            Visibility::Private if inside(&|current| *current == member.owner) => return Ok(()),
            Visibility::Protected
                if inside(&|current| self.is_subclass(current, &member.owner)) =>
            {
                return Ok(());
            }
            Visibility::Private => ErrorCode::PrivateMember {
                class: member.owner.name.clone(),
                name: name.to_string(),
            },
            Visibility::Protected => ErrorCode::ProtectedMember {
                class: member.owner.name.clone(),
                name: name.to_string(),
            },
        };
        Err(SomoniError::at(span, code))
    }

    /// Returns what the type checker knows about the parent class of a class, if it has one.
    fn parent_info(&self, info: &ClassInfo) -> Result<Option<ClassInfo>, SomoniError> {
        match &info.parent {
            Some(parent) => Ok(Some(self.class_info(parent)?.clone())),
            None => Ok(None),
        }
    }

    /// Returns the parent class of the class being checked, or T071 if there is none
    /// (`super` outside of a subclass).
    fn current_parent(&self, span: Span) -> Result<Decl<ClassLayout>, SomoniError> {
        self.current_class
            .as_ref()
            .and_then(|class| self.parent_of(class))
            .ok_or_else(|| SomoniError::at(span, ErrorCode::SuperOutsideSubclass))
    }

    /// Returns the field types and the constructor of a class.
    fn class_info(&self, layout: &Decl<ClassLayout>) -> Result<&ClassInfo, SomoniError> {
        self.classes
            .get(layout)
            .ok_or_else(|| SomoniError::internal("class without fields"))
    }

    /// Returns the class with this name: T052 if it is a struct, T001 if there is none.
    fn class_named(&self, name: &str, span: Span) -> Result<Decl<ClassLayout>, SomoniError> {
        if let Some(layout) = self.names.classes.get(name) {
            return Ok(layout.clone());
        }
        let code = if self.names.interfaces.contains_key(name) {
            ErrorCode::InterfaceNew {
                name: name.to_string(),
            }
        } else if self.names.structs.contains_key(name) {
            ErrorCode::NotAClass {
                name: name.to_string(),
            }
        } else {
            ErrorCode::UnknownType {
                name: name.to_string(),
            }
        };
        Err(SomoniError::at(span, code))
    }

    /// Declares the parameters of a function, method or constructor as const variables.
    fn declare_params(&mut self, params: &[Param], types: &[Type]) -> Result<(), SomoniError> {
        for (param, ty) in params.iter().zip(types) {
            self.declare(&param.name, param.name_span, ty.clone(), false)?;
        }
        Ok(())
    }

    fn check_stmt(&mut self, stmt: &mut Stmt) -> Result<(), SomoniError> {
        match stmt {
            Stmt::VarDecl {
                name,
                name_span,
                type_name,
                value,
                mutable,
                slot,
                ..
            } => {
                let declared = self.resolve_type(type_name, false)?;
                self.check_assignable(value, &declared)?;
                *slot = Some(self.declare(name, *name_span, declared, *mutable)?);
                Ok(())
            }
            Stmt::Assign {
                target,
                op,
                value,
                in_object,
                ..
            } => {
                *in_object = self.check_assign(target, *op, value)?;
                Ok(())
            }
            Stmt::Expr { expr, .. } => {
                self.check_expr(expr)?;
                Ok(())
            }
            Stmt::Block(stmts) => self.in_scope(|this| this.check_stmts(stmts)),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.check_condition(condition)?;
                let then_known = self.known_not_null(condition, true);
                let else_known = self.known_not_null(condition, false);
                self.with_not_null(then_known, |this| this.check_stmt(then_branch))?;
                if let Some(else_branch) = else_branch {
                    self.with_not_null(else_known, |this| this.check_stmt(else_branch))?;
                }
                Ok(())
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.check_condition(condition)?;
                let known = self.known_not_null(condition, true);
                self.with_not_null(known, |this| this.check_loop_body(body))
            }
            Stmt::For {
                init,
                condition,
                step,
                body,
                ..
            } => self.in_scope(|this| this.check_for(init, condition, step, body)),
            Stmt::ForEach {
                vars,
                iterable,
                body,
                slots,
                seq_slot,
                over,
                ..
            } => {
                let ty = self.check_expr(iterable)?;
                let Some((kind, expected)) = loop_var_types(&ty, vars.len()) else {
                    return Err(SomoniError::at(
                        iterable.span,
                        ErrorCode::NotIterable { found: ty },
                    ));
                };
                *over = Some(kind);
                let span = iterable.span;
                self.in_scope(|this| {
                    *seq_slot = Some(this.declare("#seq", span, ty, false)?);
                    this.declare("#index", span, Type::Int, true)?;
                    *slots = this.declare_bound_vars(vars, expected)?;
                    this.check_loop_body(body)
                })
            }
            Stmt::FnDecl(function) => self.check_function(function),
            Stmt::StructDecl(_) | Stmt::EnumDecl(_) | Stmt::InterfaceDecl(_) | Stmt::Import(_) => {
                Ok(())
            }
            Stmt::ClassDecl(class) => self.check_class(class),
            Stmt::Match {
                subject,
                arms,
                arm_of_variant,
                unwraps,
                span,
            } => {
                let ty = self.check_expr(subject)?;
                *arm_of_variant = self.check_match(&ty, subject.span, arms, *span)?;
                *unwraps = matches!(ty, Type::Result(_, _));
                for arm in arms {
                    self.check_arm(arm, &ty)?;
                }
                Ok(())
            }
            Stmt::Return { value, span } => self.check_return(value.as_mut(), *span),
            Stmt::Break { span } => self.check_inside_loop("break", *span),
            Stmt::Continue { span } => self.check_inside_loop("continue", *span),
        }
    }

    /// Runs `check` while the variables in `known` have the type without `?`, e.g. `int` for an
    /// `int?` after `a != null`. They keep their slot, so they are the same variables.
    fn with_not_null<T>(
        &mut self,
        known: Vec<(String, Type)>,
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.scopes.push();
        for (name, ty) in known {
            if let Some(info) = self.scopes.lookup(&name) {
                let narrowed = VarInfo {
                    ty,
                    mutable: info.mutable,
                    slot: info.slot,
                };
                self.scopes.declare(&name, narrowed);
            }
        }
        let result = check(self);
        self.scopes.pop();
        result
    }

    /// Returns the variables that cannot be null when `condition` is `when`, each with its type
    /// without `?`. Example: `a != null && b != null` gives `a` and `b` when it is true.
    fn known_not_null(&self, condition: &Expr, when: bool) -> Vec<(String, Type)> {
        match &condition.kind {
            ExprKind::Binary {
                left, op, right, ..
            } => match (op, when) {
                (BinaryOp::NotEq, true) | (BinaryOp::Eq, false) => {
                    self.null_checked(left, right).into_iter().collect()
                }
                (BinaryOp::And, true) | (BinaryOp::Or, false) => {
                    let mut known = self.known_not_null(left, when);
                    known.extend(self.known_not_null(right, when));
                    known
                }
                _ => Vec::new(),
            },
            ExprKind::Unary {
                op: UnaryOp::Not,
                operand,
            } => self.known_not_null(operand, !when),
            _ => Vec::new(),
        }
    }

    /// For `a != null` or `null != a` with `a: T?`: returns `a` and `T`.
    fn null_checked(&self, left: &Expr, right: &Expr) -> Option<(String, Type)> {
        let name = match (&left.kind, &right.kind) {
            (ExprKind::Ident { name, .. }, ExprKind::Null)
            | (ExprKind::Null, ExprKind::Ident { name, .. }) => name,
            _ => return None,
        };
        let Type::Optional(inner) = &self.scopes.lookup(name)?.ty else {
            return None;
        };
        Some((name.clone(), (**inner).clone()))
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

    /// Checks `target = value;` or `target += value;`: the target must exist and must not be
    /// const. The value must fit the target, or `target op value` must have the target's type.
    /// Returns true if the target lies inside an object.
    fn check_assign(
        &mut self,
        target: &mut Expr,
        op: Option<(BinaryOp, Span)>,
        value: &mut Expr,
    ) -> Result<bool, SomoniError> {
        if let Some((decl, variant)) = self.enum_variant(target)? {
            return Err(SomoniError::at(
                target.span,
                ErrorCode::AssignToConst {
                    name: format!("{}.{}", decl.name, decl.variants[variant].0),
                },
            ));
        }
        let target_type = self.check_expr(target)?;
        let in_object = self.inside_object(target)?;
        let Some((name, name_span)) = target.root_variable() else {
            return Err(SomoniError::internal("assignment to a non-variable"));
        };
        if !in_object && !self.scopes.lookup(name).is_some_and(|info| info.mutable) {
            return Err(SomoniError::at(
                name_span,
                ErrorCode::AssignToConst {
                    name: name.to_string(),
                },
            ));
        }
        let Some((op, op_span)) = op else {
            self.check_assignable(value, &target_type)?;
            return Ok(in_object);
        };
        let value_type = self.check_expr_expecting(value, &target_type)?;
        let result = self.binary_type(op, target_type.clone(), value_type, op_span)?;
        if result != target_type {
            return Err(SomoniError::at(
                op_span,
                ErrorCode::CannotAssign {
                    found: result,
                    expected: target_type,
                },
            ));
        }
        Ok(in_object)
    }

    /// Returns true if the target of an assignment lies inside an object (`u.age`, `l[0].age`):
    /// then it changes the object and not the variable, so `const` does not protect it.
    fn inside_object(&mut self, target: &mut Expr) -> Result<bool, SomoniError> {
        let (ExprKind::Field { target: inner, .. } | ExprKind::Index { target: inner, .. }) =
            &mut target.kind
        else {
            return Ok(false);
        };
        if let Type::Class(_) = self.check_expr(inner)? {
            return Ok(true);
        }
        self.inside_object(inner)
    }

    /// Checks that `return` is inside a function and returns the right type.
    fn check_return(&mut self, value: Option<&mut Expr>, span: Span) -> Result<(), SomoniError> {
        let Some(expected) = &self.current_return.clone() else {
            return Err(SomoniError::at(span, ErrorCode::ReturnOutsideFunction));
        };
        let (found, place) = match value {
            Some(value) => (self.check_expr_expecting(value, expected)?, value.span),
            None => (Type::Void, span),
        };
        if !self.fits(&found, expected) {
            return Err(SomoniError::at(
                place,
                ErrorCode::ReturnType {
                    found,
                    expected: expected.clone(),
                },
            ));
        }
        Ok(())
    }

    fn check_for(
        &mut self,
        init: &mut Stmt,
        condition: &mut Expr,
        step: &mut Stmt,
        body: &mut Stmt,
    ) -> Result<(), SomoniError> {
        self.check_stmt(init)?;
        self.check_condition(condition)?;
        self.check_stmt(step)?;
        let known = self.known_not_null(condition, true);
        self.with_not_null(known, |this| this.check_loop_body(body))
    }

    /// Checks the branches of a `match` over a value of type `ty`: every variant needs one,
    /// either its own or `_`. Returns, for each variant, the number of its branch.
    fn check_match(
        &self,
        ty: &Type,
        subject_span: Span,
        arms: &[MatchArm],
        span: Span,
    ) -> Result<Vec<usize>, SomoniError> {
        let Some(variants) = variant_names(ty) else {
            return Err(SomoniError::at(
                subject_span,
                ErrorCode::MatchNotEnum { found: ty.clone() },
            ));
        };
        let mut branch_of: Vec<Option<usize>> = vec![None; variants.len()];
        let mut rest: Option<(usize, Span)> = None;
        for (i, arm) in arms.iter().enumerate() {
            if let Some((_, rest_span)) = rest {
                return Err(SomoniError::at(rest_span, ErrorCode::RestNotLast));
            }
            let (variant, pattern_span) = match &arm.pattern {
                Pattern::Rest(rest_span) => {
                    rest = Some((i, *rest_span));
                    continue;
                }
                pattern => pattern_variant(ty, pattern)?,
            };
            if branch_of[variant].is_some() {
                return Err(SomoniError::at(
                    pattern_span,
                    ErrorCode::DuplicateBranch {
                        name: variants[variant].clone(),
                    },
                ));
            }
            branch_of[variant] = Some(i);
        }
        let missing: Vec<String> = variants
            .into_iter()
            .zip(&branch_of)
            .filter(|(_, branch)| branch.is_none())
            .map(|(name, _)| name)
            .collect();
        match rest {
            Some((_, rest_span)) if missing.is_empty() => {
                Err(SomoniError::at(rest_span, ErrorCode::RestUnreachable))
            }
            Some((rest_branch, _)) => Ok(branch_of
                .into_iter()
                .map(|branch| branch.unwrap_or(rest_branch))
                .collect()),
            None if !missing.is_empty() => Err(SomoniError::at(
                span,
                ErrorCode::MissingBranch {
                    missing: missing.join(", "),
                },
            )),
            None => Ok(branch_of.into_iter().flatten().collect()),
        }
    }

    /// Checks the body of one branch of a `match` over `subject`.
    /// The variable of `ok(n: int)` exists only inside its branch.
    fn check_arm(&mut self, arm: &mut MatchArm, subject: &Type) -> Result<(), SomoniError> {
        let Pattern::Result { ok, var, slot, .. } = &mut arm.pattern else {
            return self.check_stmt(&mut arm.body);
        };
        let Type::Result(value, error) = subject else {
            return Err(SomoniError::internal(
                "ok or err in a match over a non-result",
            ));
        };
        let given = if *ok { value } else { error };
        self.in_scope(|this| {
            let slots =
                this.declare_bound_vars(std::slice::from_ref(var), vec![(**given).clone()])?;
            *slot = slots.first().copied();
            this.check_stmt(&mut arm.body)
        })
    }

    /// If `expr` is `Farbe.Rot` and `Farbe` is an enum (and no variable), returns the enum
    /// and the number of the variant.
    fn enum_variant(&self, expr: &Expr) -> Result<Option<(Rc<EnumDecl>, usize)>, SomoniError> {
        let ExprKind::Field {
            target,
            name,
            name_span,
            ..
        } = &expr.kind
        else {
            return Ok(None);
        };
        let ExprKind::Ident {
            name: enum_name, ..
        } = &target.kind
        else {
            return Ok(None);
        };
        if self.scopes.lookup(enum_name).is_some() {
            return Ok(None);
        }
        let Some(decl) = self.names.enums.get(enum_name) else {
            return Ok(None);
        };
        let variant = variant_of(decl, name, *name_span)?;
        Ok(Some((Rc::clone(&decl.0), variant)))
    }

    /// Declares variables whose value is given by the language as const and returns their slots:
    /// the variables of a `for`-each loop or of `ok(n: int)`.
    /// Each must have exactly the type in `expected`.
    fn declare_bound_vars(
        &mut self,
        vars: &[Param],
        expected: Vec<Type>,
    ) -> Result<Vec<usize>, SomoniError> {
        let mut slots = Vec::new();
        for (var, expected) in vars.iter().zip(expected) {
            let declared = self.resolve_type(&var.type_name, false)?;
            if declared != expected {
                return Err(SomoniError::at(
                    var.type_name.span,
                    ErrorCode::BoundVarType {
                        name: var.name.clone(),
                        expected,
                        found: declared,
                    },
                ));
            }
            slots.push(self.declare(&var.name, var.name_span, declared, false)?);
        }
        Ok(slots)
    }

    /// Checks a loop body; inside it, `break` and `continue` are allowed.
    fn check_loop_body(&mut self, body: &mut Stmt) -> Result<(), SomoniError> {
        self.loop_depth += 1;
        let result = self.check_stmt(body);
        self.loop_depth -= 1;
        result
    }

    /// Checks that `break` or `continue` is inside a loop.
    fn check_inside_loop(&self, keyword: &'static str, span: Span) -> Result<(), SomoniError> {
        if self.loop_depth == 0 {
            return Err(SomoniError::at(span, ErrorCode::OutsideLoop { keyword }));
        }
        Ok(())
    }

    /// Checks that a condition of `if`, `while` or `for` is a bool.
    fn check_condition(&mut self, condition: &mut Expr) -> Result<(), SomoniError> {
        let ty = self.check_expr(condition)?;
        if ty != Type::Bool {
            return Err(SomoniError::at(
                condition.span,
                ErrorCode::ConditionNotBool { found: ty },
            ));
        }
        Ok(())
    }

    /// Like `check_expr`, but knows which type is expected.
    /// This lets `[]` and `null` get their type: `var l: list<int> = [];`, `var a: int? = null;`.
    fn check_expr_expecting(
        &mut self,
        expr: &mut Expr,
        expected: &Type,
    ) -> Result<Type, SomoniError> {
        let wanted = match expected {
            Type::Optional(inner) => inner,
            other => other,
        };
        match (&mut expr.kind, wanted) {
            (ExprKind::List(elements), Type::List(element_type)) => {
                self.check_list(elements, expr.span, Some(element_type))
            }
            (ExprKind::Map(entries), Type::Map(key, value)) => {
                self.check_map(entries, expr.span, Some((key, value)))
            }
            (ExprKind::Null, _) if matches!(expected, Type::Optional(_)) => Ok(expected.clone()),
            (ExprKind::Null, _) => Err(SomoniError::at(
                expr.span,
                ErrorCode::NullNotAllowed {
                    expected: expected.clone(),
                },
            )),
            (
                ExprKind::Call {
                    name, args, target, ..
                },
                Type::Result(value, error),
            ) if Builtin::result_builder(name).is_some() => {
                let builtin = Builtin::result_builder(name)
                    .ok_or_else(|| SomoniError::internal("ok or err is no built-in"))?;
                *target = Some(CallTarget::Builtin(builtin));
                check_arg_count(name, builtin.arity(), args, expr.span)?;
                let inner = if builtin == Builtin::Ok { value } else { error };
                let found = self.check_expr_expecting(&mut args[0], inner)?;
                if !self.fits(&found, inner) {
                    return Err(wrong_arg_type(name, found, args[0].span));
                }
                Ok(wanted.clone())
            }
            _ => self.check_expr(expr),
        }
    }

    /// Checks both sides of an operator. `[]` and `null` get the type of the other side:
    /// `l + []`, `a == null`. Right of `&&` and `||`, the null checks on the left count.
    fn check_operands(
        &mut self,
        op: BinaryOp,
        left: &mut Expr,
        right: &mut Expr,
    ) -> Result<(Type, Type), SomoniError> {
        if needs_type_hint(left) {
            let r = self.check_expr(right)?;
            let l = self.check_expr_expecting(left, &r)?;
            return Ok((l, r));
        }
        let l = self.check_expr(left)?;
        let known = match op {
            BinaryOp::And => self.known_not_null(left, true),
            BinaryOp::Or => self.known_not_null(left, false),
            _ => Vec::new(),
        };
        let r = self.with_not_null(known, |this| this.check_expr_expecting(right, &l))?;
        Ok((l, r))
    }

    /// Checks a list literal: all elements must have the same type.
    /// `hint` is the expected element type; without it, the first element gives the type.
    fn check_list(
        &mut self,
        elements: &mut [Expr],
        span: Span,
        hint: Option<&Type>,
    ) -> Result<Type, SomoniError> {
        let (element_type, rest) = match hint {
            Some(hint) => (hint.clone(), elements),
            None => {
                let Some((first, rest)) = elements.split_first_mut() else {
                    return Err(SomoniError::at(span, ErrorCode::EmptyListType));
                };
                (self.check_expr(first)?, rest)
            }
        };
        for element in rest {
            let found = self.check_expr_expecting(element, &element_type)?;
            if !self.fits(&found, &element_type) {
                return Err(SomoniError::at(
                    element.span,
                    ErrorCode::ListElementType {
                        expected: element_type,
                        found,
                    },
                ));
            }
        }
        Ok(Type::List(Box::new(element_type)))
    }

    /// Checks a map literal: all keys must have one type, and all values too.
    /// `hint` is the expected key and value type; without it, the first entry gives them.
    fn check_map(
        &mut self,
        entries: &mut [(Expr, Expr)],
        span: Span,
        hint: Option<(&Type, &Type)>,
    ) -> Result<Type, SomoniError> {
        let (key_type, value_type, rest) = match hint {
            Some((key, value)) => (key.clone(), value.clone(), entries),
            None => {
                let Some(((key, value), rest)) = entries.split_first_mut() else {
                    return Err(SomoniError::at(span, ErrorCode::EmptyMapType));
                };
                let key_type = self.check_expr(key)?;
                check_key_type(&key_type, key.span)?;
                (key_type, self.check_expr(value)?, rest)
            }
        };
        for (key, value) in rest {
            self.check_map_part("keys", key, &key_type)?;
            self.check_map_part("values", value, &value_type)?;
        }
        Ok(Type::Map(Box::new(key_type), Box::new(value_type)))
    }

    /// Checks that a key or a value in a map literal has the type of the others.
    fn check_map_part(
        &mut self,
        part: &'static str,
        expr: &mut Expr,
        expected: &Type,
    ) -> Result<(), SomoniError> {
        let found = self.check_expr_expecting(expr, expected)?;
        if !self.fits(&found, expected) {
            return Err(SomoniError::at(
                expr.span,
                ErrorCode::MapEntryType {
                    part,
                    expected: expected.clone(),
                    found,
                },
            ));
        }
        Ok(())
    }

    /// Checks a struct literal: every field must be given exactly once, in any order.
    fn check_struct_literal(
        &mut self,
        name: &str,
        name_span: Span,
        fields: &mut [FieldInit],
        index: &mut Option<usize>,
    ) -> Result<Type, SomoniError> {
        if self.names.classes.contains_key(name) {
            return Err(SomoniError::at(
                name_span,
                ErrorCode::ClassLiteral {
                    name: name.to_string(),
                },
            ));
        }
        let decl = self.struct_named(name, name_span)?;
        let info = &self.struct_info(&decl)?.clone();
        *index = Some(info.index);
        let mut given = vec![false; info.fields.len()];
        for init in fields {
            let (position, ty) = field_of(&info.fields, name, &init.name, init.name_span)?;
            if given[position] {
                return Err(SomoniError::at(
                    init.name_span,
                    ErrorCode::DuplicateField {
                        name: init.name.clone(),
                    },
                ));
            }
            given[position] = true;
            init.index = Some(position);
            self.check_assignable(&mut init.value, ty)?;
        }
        let missing = info.fields.iter().zip(&given).find(|(_, given)| !**given);
        if let Some(((field, _), _)) = missing {
            return Err(SomoniError::at(
                name_span,
                ErrorCode::MissingField {
                    struct_name: name.to_string(),
                    name: field.clone(),
                },
            ));
        }
        Ok(Type::Struct(decl))
    }

    /// Returns the struct with this name, or T001 if there is none.
    fn struct_named(&self, name: &str, span: Span) -> Result<Decl<StructDecl>, SomoniError> {
        self.names.structs.get(name).cloned().ok_or_else(|| {
            SomoniError::at(
                span,
                ErrorCode::UnknownType {
                    name: name.to_string(),
                },
            )
        })
    }

    /// Returns the number and the field types of a struct.
    fn struct_info(&self, decl: &Decl<StructDecl>) -> Result<&StructInfo, SomoniError> {
        self.structs
            .get(decl)
            .ok_or_else(|| SomoniError::internal("struct without fields"))
    }

    /// Turns a written type like `list<int>?` into a `Type`. `void` is only allowed if `allow_void`.
    fn resolve_type(&self, written: &TypeName, allow_void: bool) -> Result<Type, SomoniError> {
        let ty = self.resolve_without_optional(written, allow_void)?;
        if !written.optional {
            return Ok(ty);
        }
        if ty == Type::Void {
            return Err(SomoniError::at(written.span, ErrorCode::OptionalVoid));
        }
        Ok(Type::Optional(Box::new(ty)))
    }

    /// Like `resolve_type`, but without the `?`: `list<int>?` gives `list<int>`.
    fn resolve_without_optional(
        &self,
        written: &TypeName,
        allow_void: bool,
    ) -> Result<Type, SomoniError> {
        let simple = if allow_void {
            Type::from_return_name(&written.name)
        } else {
            Type::from_name(&written.name)
        };
        let simple = simple.or_else(|| {
            if let Some(decl) = self.names.structs.get(&written.name) {
                Some(Type::Struct(decl.clone()))
            } else if let Some(layout) = self.names.classes.get(&written.name) {
                Some(Type::Class(layout.clone()))
            } else if let Some(layout) = self.names.interfaces.get(&written.name) {
                Some(Type::Interface(layout.clone()))
            } else {
                self.names.enums.get(&written.name).cloned().map(Type::Enum)
            }
        });
        let expected_args = match (Type::arg_count(&written.name), &simple) {
            (Some(count), _) => count,
            (None, Some(_)) => 0,
            (None, None) => {
                return Err(SomoniError::at(
                    written.span,
                    ErrorCode::UnknownType {
                        name: written.name.clone(),
                    },
                ));
            }
        };
        if written.args.len() != expected_args {
            return Err(SomoniError::at(
                written.span,
                ErrorCode::TypeArguments {
                    name: written.name.clone(),
                    expected: expected_args,
                    found: written.args.len(),
                },
            ));
        }
        if let Some(ty) = simple {
            return Ok(ty);
        }
        let mut args = Vec::new();
        for arg in &written.args {
            args.push(self.resolve_type(arg, false)?);
        }
        let ty = Type::with_args(&written.name, args)
            .ok_or_else(|| SomoniError::internal("type arguments do not fit"))?;
        if let Type::Map(key, _) = &ty {
            check_key_type(key, written.args[0].span)?;
        }
        Ok(ty)
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

    /// Returns the type of an expression, or a type error.
    fn check_expr(&mut self, expr: &mut Expr) -> Result<Type, SomoniError> {
        if let Some((decl, variant)) = self.enum_variant(expr)? {
            let ty = Type::Enum(Decl(Rc::clone(&decl)));
            expr.kind = ExprKind::EnumValue { decl, variant };
            return Ok(ty);
        }
        match &mut expr.kind {
            ExprKind::Int(_) => Ok(Type::Int),
            ExprKind::EnumValue { decl, .. } => Ok(Type::Enum(Decl(Rc::clone(decl)))),
            ExprKind::Float(_) => Ok(Type::Float),
            ExprKind::Bool(_) => Ok(Type::Bool),
            ExprKind::Str(_) => Ok(Type::Str),
            ExprKind::Null => Err(SomoniError::at(expr.span, ErrorCode::NullTypeUnknown)),
            ExprKind::Ident { name, slot } => {
                let info = self.scopes.lookup(name).ok_or_else(|| {
                    let code = if name == THIS {
                        ErrorCode::ThisOutsideClass
                    } else {
                        ErrorCode::UnknownVariable { name: name.clone() }
                    };
                    SomoniError::at(expr.span, code)
                })?;
                *slot = Some(info.slot);
                Ok(info.ty.clone())
            }
            ExprKind::Binary {
                left,
                op,
                op_span,
                right,
            } => {
                let (l, r) = self.check_operands(*op, left, right)?;
                self.binary_type(*op, l, r, *op_span)
            }
            ExprKind::List(elements) => self.check_list(elements, expr.span, None),
            ExprKind::Map(entries) => self.check_map(entries, expr.span, None),
            ExprKind::Index { target, index } => {
                let target_type = self.check_expr(target)?;
                match target_type {
                    Type::List(element_type) => {
                        let index_type = self.check_expr(index)?;
                        if index_type != Type::Int {
                            return Err(SomoniError::at(
                                index.span,
                                ErrorCode::IndexNotInt { found: index_type },
                            ));
                        }
                        Ok(*element_type)
                    }
                    Type::Map(key_type, value_type) => {
                        let found = self.check_expr(index)?;
                        if found != *key_type {
                            return Err(SomoniError::at(
                                index.span,
                                ErrorCode::MapIndexType {
                                    expected: *key_type,
                                    found,
                                },
                            ));
                        }
                        Ok(*value_type)
                    }
                    other => Err(SomoniError::at(
                        target.span,
                        ErrorCode::NotIndexable { found: other },
                    )),
                }
            }
            ExprKind::StructLiteral {
                name,
                name_span,
                fields,
                index,
            } => self.check_struct_literal(name, *name_span, fields, index),
            ExprKind::Field {
                target,
                name,
                name_span,
                index,
            } => {
                let target_type = self.check_expr(target)?;
                let Some(fields) = self.fields_of(&target_type) else {
                    return Err(SomoniError::at(
                        target.span,
                        ErrorCode::NoFields { found: target_type },
                    ));
                };
                let owner = target_type.to_string();
                let (position, ty) = field_of(fields, &owner, name, *name_span)?;
                *index = Some(position);
                let ty = ty.clone();
                if let Type::Class(layout) = &target_type {
                    self.check_access(layout, name, *name_span)?;
                }
                Ok(ty)
            }
            ExprKind::Call {
                name,
                name_span,
                args,
                target,
            } => {
                if let Some(builtin) = Builtin::from_name(name) {
                    if builtin.builds_result() {
                        return Err(SomoniError::at(
                            expr.span,
                            ErrorCode::ResultTypeUnknown {
                                name: builtin.name(),
                            },
                        ));
                    }
                    *target = Some(CallTarget::Builtin(builtin));
                    check_arg_count(name, builtin.arity(), args, expr.span)?;
                    let mut arg_types = Vec::new();
                    for arg in args.iter_mut() {
                        arg_types.push(self.check_expr(arg)?);
                    }
                    return builtin.result_type(&arg_types).map_err(|bad| {
                        wrong_arg_type(name, arg_types[bad].clone(), args[bad].span)
                    });
                }
                let Some(signature) = &self.names.functions.get(name).cloned() else {
                    return Err(SomoniError::at(
                        *name_span,
                        ErrorCode::UnknownFunction { name: name.clone() },
                    ));
                };
                *target = Some(CallTarget::Function(signature.index));
                self.check_args(name, &signature.params, args, expr.span)?;
                Ok(signature.return_type.clone())
            }
            ExprKind::MethodCall {
                target,
                name,
                name_span,
                args,
                call,
            } => {
                let target_type = self.check_expr(target)?;
                if let Type::Interface(layout) = &target_type {
                    let info = self.interface_info(layout)?;
                    let Some((_, signature)) =
                        info.methods.iter().find(|(m, _)| m == name).cloned()
                    else {
                        return Err(SomoniError::at(
                            *name_span,
                            ErrorCode::UnknownMethod {
                                class: layout.name.clone(),
                                name: name.clone(),
                            },
                        ));
                    };
                    *call = Some(MethodTarget::Interface {
                        interface: info.index,
                        method: signature.index,
                    });
                    self.check_args(name, &signature.params, args, expr.span)?;
                    return Ok(signature.return_type);
                }
                let Type::Class(layout) = &target_type else {
                    return Err(SomoniError::at(
                        target.span,
                        ErrorCode::NoMethods { found: target_type },
                    ));
                };
                let method = self
                    .class_info(layout)?
                    .methods
                    .get(name)
                    .cloned()
                    .ok_or_else(|| {
                        SomoniError::at(
                            *name_span,
                            ErrorCode::UnknownMethod {
                                class: layout.name.clone(),
                                name: name.clone(),
                            },
                        )
                    })?;
                self.check_access(layout, name, *name_span)?;
                *call = Some(MethodTarget::Virtual { slot: method.slot });
                self.check_args(name, &method.signature.params, args, expr.span)?;
                Ok(method.signature.return_type)
            }
            ExprKind::SuperInit { args, target } => {
                let parent = self.current_parent(expr.span)?;
                if !self.super_init_allowed {
                    return Err(SomoniError::at(expr.span, ErrorCode::SuperInitMisplaced));
                }
                let constructor = self.class_info(&parent)?.constructor.clone();
                *target = Some(constructor.index);
                self.check_args(&parent.name, &constructor.params, args, expr.span)?;
                Ok(Type::Void)
            }
            ExprKind::SuperCall {
                name,
                name_span,
                args,
                target,
            } => {
                let parent = self.current_parent(expr.span)?;
                let info = self.class_info(&parent)?;
                let Some(method) = info.methods.get(name).cloned() else {
                    return Err(SomoniError::at(
                        *name_span,
                        ErrorCode::UnknownMethod {
                            class: parent.name.clone(),
                            name: name.clone(),
                        },
                    ));
                };
                *target = Some(info.method_table[method.slot]);
                self.check_access(&parent, name, *name_span)?;
                self.check_args(name, &method.signature.params, args, expr.span)?;
                Ok(method.signature.return_type)
            }
            ExprKind::New {
                name,
                name_span,
                args,
                class,
                target,
            } => {
                let layout = self.class_named(name, *name_span)?;
                let info = self.class_info(&layout)?;
                let constructor = info.constructor.clone();
                *class = Some(info.index);
                *target = Some(constructor.index);
                self.check_args(name, &constructor.params, args, expr.span)?;
                Ok(constructor.return_type)
            }
            ExprKind::Unary { op, operand } => {
                let t = self.check_expr(operand)?;
                match (op, &t) {
                    (UnaryOp::Not, Type::Bool) => Ok(Type::Bool),
                    (UnaryOp::Not, _) => Err(SomoniError::at(
                        expr.span,
                        ErrorCode::NotNeedsBool { found: t.clone() },
                    )),
                    (UnaryOp::Neg, Type::Int | Type::Float) => Ok(t.clone()),
                    (UnaryOp::Neg, _) => Err(SomoniError::at(
                        expr.span,
                        ErrorCode::NegNeedsNumber { found: t.clone() },
                    )),
                }
            }
        }
    }

    /// Returns the type of `l op r`, or an error if the operator does not fit the types.
    /// `op_span` is the place of the operator, for the error.
    fn binary_type(
        &self,
        op: BinaryOp,
        l: Type,
        r: Type,
        op_span: Span,
    ) -> Result<Type, SomoniError> {
        let is_equality = matches!(op, BinaryOp::Eq | BinaryOp::NotEq);
        if l != r && !(is_equality && (self.fits(&l, &r) || self.fits(&r, &l))) {
            return Err(SomoniError::at(
                op_span,
                ErrorCode::OperandTypesDiffer { left: l, right: r },
            ));
        }
        match (&l, op) {
            (Type::Void, _) => Err(SomoniError::at(
                op_span,
                ErrorCode::OperatorNotAllowed { found: l },
            )),
            (_, BinaryOp::Eq | BinaryOp::NotEq) => Ok(Type::Bool),
            (Type::Int, BinaryOp::Mod) => Ok(Type::Int),
            (
                Type::Int | Type::Float,
                BinaryOp::Less | BinaryOp::Greater | BinaryOp::LessEq | BinaryOp::GreaterEq,
            ) => Ok(Type::Bool),
            (
                Type::Int | Type::Float,
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div,
            ) => Ok(l),
            (Type::Str, BinaryOp::Add) => Ok(Type::Str),
            (Type::List(_), BinaryOp::Add) => Ok(l),
            (Type::Bool, BinaryOp::And | BinaryOp::Or) => Ok(Type::Bool),
            _ => Err(SomoniError::at(
                op_span,
                ErrorCode::OperatorNotAllowed { found: l },
            )),
        }
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

/// Returns what a `for`-each loop over `ty` runs over and the types of its `count` variables:
/// a list gives the element (or index and element), a map the key (or key and value),
/// a string a character (or index and character). None if `ty` cannot be looped over.
fn loop_var_types(ty: &Type, count: usize) -> Option<(IterKind, Vec<Type>)> {
    let (kind, first, second) = match ty {
        Type::List(element) => (IterKind::List, Type::Int, (**element).clone()),
        Type::Map(key, value) => (IterKind::Map, (**key).clone(), (**value).clone()),
        Type::Str => (IterKind::Str, Type::Int, Type::Str),
        _ => return None,
    };
    let types = match (kind, count) {
        (_, 2) => vec![first, second],
        (IterKind::Map, _) => vec![first],
        (IterKind::List | IterKind::Str, _) => vec![second],
    };
    Some((kind, types))
}

/// Returns true for `[]`, `{}`, `null`, `ok(...)` and `err(...)`:
/// they need the type of the place where they are used.
fn needs_type_hint(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::List(elements) => elements.is_empty(),
        ExprKind::Map(entries) => entries.is_empty(),
        ExprKind::Null => true,
        ExprKind::Call { name, .. } => Builtin::result_builder(name).is_some(),
        _ => false,
    }
}

/// Checks rule 5 of `Klassen.md`: every field without a start value is set in `body`, the
/// constructor, directly with `this.field = value;`. Until all are set, `this` may not be used
/// in any other way (T054). A field that is never set is T050.
fn check_fields_set(class: &ClassDecl, body: &[Stmt]) -> Result<(), SomoniError> {
    let mut missing: Vec<&ClassField> = class.fields.iter().filter(|f| f.start.is_none()).collect();
    for stmt in body {
        if missing.is_empty() {
            return Ok(());
        }
        if let Some(name) = sets_field(stmt) {
            missing.retain(|field| field.field.name != name);
        } else if let Some(span) = stmt.find_this() {
            let names: Vec<String> = missing
                .iter()
                .map(|field| format!("'{}'", field.field.name))
                .collect();
            return Err(SomoniError::at(
                span,
                ErrorCode::ThisBeforeFieldsSet {
                    missing: names.join(", "),
                },
            ));
        }
    }
    match missing.first() {
        Some(field) => Err(SomoniError::at(
            field.field.name_span,
            ErrorCode::FieldNotSet {
                class: class.layout.name.clone(),
                field: field.field.name.clone(),
            },
        )),
        None => Ok(()),
    }
}

/// Returns true if the first statement is `super(...)`.
fn starts_with_super(body: &[Stmt]) -> bool {
    matches!(
        body.first(),
        Some(Stmt::Expr {
            expr: Expr {
                kind: ExprKind::SuperInit { .. },
                ..
            },
            ..
        })
    )
}

/// Describes a method for a message, e.g. `public laut(): string`.
fn describe_method(name: &str, visibility: Visibility, signature: &Signature) -> String {
    let visibility = match visibility {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Private => "private",
    };
    let params: Vec<String> = signature.params.iter().map(Type::to_string).collect();
    format!(
        "{} {}({}): {}",
        visibility,
        name,
        params.join(", "),
        signature.return_type
    )
}

/// For `this.name = value;` where `value` does not use `this`, returns `name`.
fn sets_field(stmt: &Stmt) -> Option<&str> {
    let Stmt::Assign {
        target,
        op: None,
        value,
        ..
    } = stmt
    else {
        return None;
    };
    let ExprKind::Field {
        target: object,
        name,
        ..
    } = &target.kind
    else {
        return None;
    };
    let ExprKind::Ident { name: variable, .. } = &object.kind else {
        return None;
    };
    (variable == THIS && value.find_this().is_none()).then_some(name.as_str())
}

/// Returns the variants a `match` over `ty` must cover, as written in a branch:
/// `Farbe.Rot, ...` for an enum, `ok` and `err` for a result. None if `match` cannot take `ty`.
fn variant_names(ty: &Type) -> Option<Vec<String>> {
    match ty {
        Type::Enum(decl) => Some(
            decl.variants
                .iter()
                .map(|(name, _)| format!("{}.{}", decl.name, name))
                .collect(),
        ),
        Type::Result(_, _) => Some(vec!["ok".to_string(), "err".to_string()]),
        _ => None,
    }
}

/// Returns the number of the variant a branch stands for (`ok` is 0, `err` is 1) and the
/// place of the pattern, or T042 if the pattern does not fit the type of the `match`.
fn pattern_variant(ty: &Type, pattern: &Pattern) -> Result<(usize, Span), SomoniError> {
    let (found, span) = match (ty, pattern) {
        (
            Type::Enum(decl),
            Pattern::Variant {
                enum_name,
                name,
                name_span,
                ..
            },
        ) if *enum_name == decl.name => {
            return Ok((variant_of(decl, name, *name_span)?, *name_span));
        }
        (Type::Result(_, _), Pattern::Result { ok, span, .. }) => {
            return Ok((if *ok { 0 } else { 1 }, *span));
        }
        (
            _,
            Pattern::Variant {
                enum_name,
                enum_span,
                ..
            },
        ) => (enum_name.clone(), *enum_span),
        (_, Pattern::Result { span, .. }) => ("result".to_string(), *span),
        (_, Pattern::Rest(_)) => return Err(SomoniError::internal("'_' has no variant")),
    };
    Err(SomoniError::at(
        span,
        ErrorCode::PatternWrongEnum {
            expected: ty.clone(),
            found,
        },
    ))
}

/// Returns the number of the variant `name` of an enum, or T040 if it has none.
fn variant_of(decl: &EnumDecl, name: &str, span: Span) -> Result<usize, SomoniError> {
    decl.variants
        .iter()
        .position(|(variant, _)| variant == name)
        .ok_or_else(|| {
            SomoniError::at(
                span,
                ErrorCode::UnknownVariant {
                    enum_name: decl.name.clone(),
                    name: name.to_string(),
                },
            )
        })
}

/// Checks that a type may be the key of a map: only `string` and `int`.
fn check_key_type(ty: &Type, span: Span) -> Result<(), SomoniError> {
    match ty {
        Type::Str | Type::Int => Ok(()),
        other => Err(SomoniError::at(
            span,
            ErrorCode::MapKeyType {
                found: other.clone(),
            },
        )),
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

/// Returns true if every path through these statements ends with `return`.
fn always_returns(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_always_returns)
}

/// Returns true if this statement always ends with `return` (an `if` only with `else`).
fn stmt_always_returns(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return { .. } => true,
        Stmt::Block(stmts) => always_returns(stmts),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => match else_branch {
            Some(else_branch) => {
                stmt_always_returns(then_branch) && stmt_always_returns(else_branch)
            }
            None => false,
        },
        Stmt::While { .. } | Stmt::For { .. } | Stmt::ForEach { .. } => false,
        Stmt::VarDecl { .. }
        | Stmt::Assign { .. }
        | Stmt::Expr { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::FnDecl(_)
        | Stmt::StructDecl(_)
        | Stmt::EnumDecl(_)
        | Stmt::ClassDecl(_)
        | Stmt::InterfaceDecl(_)
        | Stmt::Import(_) => false,
        Stmt::Match { arms, .. } => arms.iter().all(|arm| stmt_always_returns(&arm.body)),
    }
}
