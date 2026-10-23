//! Classes: inheritance, fields, constructors, method tables and who may use which member.

use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::{
    self, Assign, CONSTRUCTOR, ClassDecl, ClassField, ClassLayout, ClassPlan, Constructor, Expr,
    ExprKind, Method, Stmt, THIS, Visibility,
};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::types::{Decl, Type};

use super::members::ClassMembers;
use super::{ClassInfo, MemberKind, ObjectField, Signature, TypeChecker};

impl TypeChecker {
    /// Finds the parent class of every class of a file and rejects cycles.
    pub(super) fn resolve_parents(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
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
            self.check_no_inheritance_cycle(class)?;
        }
        Ok(())
    }

    /// Returns an error if a class inherits from itself, also over several steps.
    fn check_no_inheritance_cycle(&self, class: &ClassDecl) -> Result<(), SomoniError> {
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
        Ok(())
    }

    /// Returns the parent class of a class, if it has one.
    fn parent_of(&self, layout: &Decl<ClassLayout>) -> Option<Decl<ClassLayout>> {
        self.classes
            .get(layout)
            .and_then(|info| info.parent.clone())
    }

    /// Returns true if `class` is `ancestor` or inherits from it, also over several steps.
    pub(super) fn is_subclass(
        &self,
        class: &Decl<ClassLayout>,
        ancestor: &Decl<ClassLayout>,
    ) -> bool {
        let mut current = Some(class.clone());
        while let Some(layout) = current {
            if layout == *ancestor {
                return true;
            }
            current = self.parent_of(&layout);
        }
        false
    }

    /// Returns the classes of a file, every parent class before its subclasses.
    pub(super) fn parents_first<'a>(&self, stmts: &'a [Stmt]) -> Vec<&'a ClassDecl> {
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

    /// Collects the fields of a class, those of its parent class first.
    pub(super) fn declare_class_fields(&mut self, class: &ClassDecl) -> Result<(), SomoniError> {
        let layout = Decl(Rc::clone(&class.layout));
        let mut fields = match self.parent_of(&layout) {
            Some(parent) => self.class_info(&parent)?.fields.clone(),
            None => Vec::new(),
        };
        let first_own_field = fields.len();
        fields.extend(self.resolve_fields(class.fields.iter().map(|f| &f.field))?);
        if let Some(info) = self.classes.get_mut(&layout) {
            info.fields = fields;
            info.first_own_field = first_own_field;
        }
        Ok(())
    }

    /// Numbers the functions of every class like the compiler and builds their method tables.
    pub(super) fn declare_class_functions(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
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

    /// Builds members, methods, method table and vtables of a class whose parent class is done.
    fn declare_class_methods(
        &mut self,
        class: &ClassDecl,
        init: usize,
        functions: &[usize],
    ) -> Result<(), SomoniError> {
        let layout = Decl(Rc::clone(&class.layout));
        let info = self.class_info(&layout)?.clone();
        let parent = self.parent_info(&info)?;
        let params = self.constructor_params(class, parent.as_ref())?;
        let mut built = ClassMembers::inherit(layout.clone(), parent.as_ref());
        for field in &class.fields {
            built.add_field(field)?;
        }
        let mut statics = Vec::new();
        for (method, &function) in class.methods.iter().zip(functions) {
            let signature = self.signature_of(&method.function, function)?;
            if method.is_static {
                statics.push((method, signature));
            } else {
                built.add_method(method, signature)?;
            }
        }
        for (method, signature) in statics {
            built.add_static(method, signature)?;
        }
        check_no_abstract_left(class, &built)?;
        let implements = self.all_interfaces(class, &built, parent.as_ref())?;
        let mut vtables = Vec::new();
        for interface in &implements {
            vtables.push(self.vtable(interface, &built)?);
        }
        if let Some(info) = self.classes.get_mut(&layout) {
            info.constructor.index = init;
            info.constructor.params = params;
            info.methods = built.methods;
            info.method_table = built.table;
            info.members = built.members;
            info.implements = implements;
            info.vtables = vtables;
            info.statics = built.statics;
            info.constructor_visibility = class
                .constructor
                .as_ref()
                .map_or(Visibility::Public, |c| c.visibility);
            info.is_abstract = class.is_abstract;
        }
        Ok(())
    }

    /// Returns the parameter types of the own constructor of a class, or else of the inherited one.
    fn constructor_params(
        &self,
        class: &ClassDecl,
        parent: Option<&ClassInfo>,
    ) -> Result<Vec<Type>, SomoniError> {
        let Some(constructor) = &class.constructor else {
            return Ok(parent.map_or(Vec::new(), |p| p.constructor.params.clone()));
        };
        self.resolve_params(&constructor.params)
    }

    /// Checks a method like a function, with `this` in slot 0 unless it is static.
    fn check_method(
        &mut self,
        method: &mut Method,
        layout: &Decl<ClassLayout>,
        signature: &Signature,
    ) -> Result<(), SomoniError> {
        let name = format!("{}.{}", layout.name, method.function.name);
        let this_type = (!method.is_static).then(|| Type::Class(layout.clone()));
        self.check_body(&mut method.function, &name, signature, this_type)
    }

    /// Checks a class with access to its private members.
    pub(super) fn check_class(&mut self, class: &mut ClassDecl) -> Result<(), SomoniError> {
        let layout = Decl(Rc::clone(&class.layout));
        let outer_class = self.current_class.replace(layout.clone());
        let result = self.check_class_parts(class, layout);
        self.current_class = outer_class;
        result
    }

    /// Checks the start values, the constructor and the methods of a class.
    fn check_class_parts(
        &mut self,
        class: &mut ClassDecl,
        layout: Decl<ClassLayout>,
    ) -> Result<(), SomoniError> {
        let info = self.class_info(&layout)?.clone();
        let parent = self.parent_info(&info)?;
        check_parent_constructor(class, parent.as_ref())?;
        class.plan = class_plan(&info, parent.as_ref());
        self.check_field_starts(class, &info)?;
        if let Some(constructor) = &mut class.constructor {
            self.check_constructor(constructor, &layout, &info.constructor, parent.is_some())?;
        }
        self.check_methods(class, &layout, &info)?;
        check_fields_set(class)
    }

    /// Checks the start values of the own fields of a class.
    fn check_field_starts(
        &mut self,
        class: &mut ClassDecl,
        info: &ClassInfo,
    ) -> Result<(), SomoniError> {
        let own_fields = &info.fields[info.first_own_field..];
        self.in_function(None, |this| {
            for (field, (_, ty)) in class.fields.iter_mut().zip(own_fields) {
                if let Some(start) = &mut field.start {
                    this.check_assignable(start, ty)?;
                }
            }
            Ok(())
        })
    }

    /// Checks a constructor, which must start with `super(...)` in a subclass (T069).
    fn check_constructor(
        &mut self,
        constructor: &mut Constructor,
        layout: &Decl<ClassLayout>,
        signature: &Signature,
        has_parent: bool,
    ) -> Result<(), SomoniError> {
        if has_parent && !starts_with_super(&constructor.body) {
            return Err(SomoniError::at(
                constructor.span,
                ErrorCode::MissingSuper {
                    class: layout.name.clone(),
                },
            ));
        }
        self.in_constructor = true;
        let result = self.in_function(Some(Type::Void), |this| {
            let this_type = Type::Class(layout.clone());
            this.declare(THIS, constructor.span, this_type, false)?;
            this.declare_params(&constructor.params, &signature.params)?;
            this.check_constructor_body(&mut constructor.body, has_parent)
        });
        self.in_constructor = false;
        result
    }

    /// Checks the statements of a constructor, where only the first may be `super(...)` in a subclass.
    fn check_constructor_body(
        &mut self,
        body: &mut [Stmt],
        has_parent: bool,
    ) -> Result<(), SomoniError> {
        for (i, stmt) in body.iter_mut().enumerate() {
            self.super_init_allowed = has_parent && i == 0;
            let result = self.check_stmt(stmt);
            self.super_init_allowed = false;
            result?;
        }
        Ok(())
    }

    /// Checks every method of a class with its signature.
    fn check_methods(
        &mut self,
        class: &mut ClassDecl,
        layout: &Decl<ClassLayout>,
        info: &ClassInfo,
    ) -> Result<(), SomoniError> {
        for method in class.methods.iter_mut().filter(|m| !m.is_abstract) {
            let signature = method_signature(info, method)?;
            self.check_method(method, layout, signature)?;
        }
        Ok(())
    }

    /// Returns an error if `name` is private or protected and not visible here.
    pub(super) fn check_access(
        &self,
        layout: &Decl<ClassLayout>,
        name: &str,
        span: Span,
    ) -> Result<(), SomoniError> {
        let Some(member) = self.class_info(layout)?.members.get(name) else {
            return Ok(());
        };
        self.check_visible(member.visibility, &member.owner, name, span)
    }

    /// Returns T058 or T072 if something called `name` of class `owner` with this visibility is not visible here.
    pub(super) fn check_visible(
        &self,
        visibility: Visibility,
        owner: &Decl<ClassLayout>,
        name: &str,
        span: Span,
    ) -> Result<(), SomoniError> {
        let inside = |allowed: &dyn Fn(&Decl<ClassLayout>) -> bool| {
            self.current_class.as_ref().is_some_and(allowed)
        };
        let code = match visibility {
            Visibility::Public => return Ok(()),
            Visibility::Private if inside(&|current| current == owner) => return Ok(()),
            Visibility::Protected if inside(&|current| self.is_subclass(current, owner)) => {
                return Ok(());
            }
            Visibility::Private => ErrorCode::PrivateMember {
                class: owner.name.clone(),
                name: name.to_string(),
            },
            Visibility::Protected => ErrorCode::ProtectedMember {
                class: owner.name.clone(),
                name: name.to_string(),
            },
        };
        Err(SomoniError::at(span, code))
    }

    /// Returns T073 if `field` is readonly, unless the constructor of its own class sets it through `this`.
    pub(super) fn check_not_readonly(&self, field: &ObjectField) -> Result<(), SomoniError> {
        let Some(member) = self.class_info(&field.layout)?.members.get(&field.name) else {
            return Ok(());
        };
        if member.kind != (MemberKind::Field { readonly: true }) {
            return Ok(());
        }
        let in_own_constructor =
            self.in_constructor && self.current_class.as_ref() == Some(&member.owner);
        if field.through_this && in_own_constructor {
            return Ok(());
        }
        Err(SomoniError::at(
            field.span,
            ErrorCode::ReadonlyField {
                class: member.owner.name.clone(),
                name: field.name.clone(),
            },
        ))
    }

    /// Returns the info of the parent class, if there is one.
    fn parent_info(&self, info: &ClassInfo) -> Result<Option<ClassInfo>, SomoniError> {
        match &info.parent {
            Some(parent) => Ok(Some(self.class_info(parent)?.clone())),
            None => Ok(None),
        }
    }

    /// Returns the parent of the class being checked, for `super`.
    pub(super) fn current_parent(&self, span: Span) -> Result<Decl<ClassLayout>, SomoniError> {
        self.current_class
            .as_ref()
            .and_then(|class| self.parent_of(class))
            .ok_or_else(|| SomoniError::at(span, ErrorCode::SuperOutsideSubclass))
    }

    /// Returns what the type checker knows about a class.
    pub(super) fn class_info(&self, layout: &Decl<ClassLayout>) -> Result<&ClassInfo, SomoniError> {
        self.classes
            .get(layout)
            .ok_or_else(|| SomoniError::internal("class without fields"))
    }

    /// Returns the class with this name: T052 if it is a struct, T001 if there is none.
    pub(super) fn class_named(
        &self,
        name: &str,
        span: Span,
    ) -> Result<Decl<ClassLayout>, SomoniError> {
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
}

/// Returns the signature of a method, from the static or the other methods of the class.
fn method_signature<'a>(
    info: &'a ClassInfo,
    method: &Method,
) -> Result<&'a Signature, SomoniError> {
    let name = &method.function.name;
    let signature = if method.is_static {
        info.statics.get(name).map(|m| &m.signature)
    } else {
        info.methods.get(name).map(|m| &m.signature)
    };
    signature.ok_or_else(|| SomoniError::internal("method without signature"))
}

/// Returns T077 for the first abstract method that a class which is not abstract inherits without overriding it.
fn check_no_abstract_left(class: &ClassDecl, built: &ClassMembers) -> Result<(), SomoniError> {
    if class.is_abstract {
        return Ok(());
    }
    let left = built
        .methods
        .iter()
        .filter(|(_, method)| method.is_abstract)
        .min_by_key(|(_, method)| method.slot);
    let Some((name, _)) = left else {
        return Ok(());
    };
    let owner = built
        .members
        .get(name)
        .map_or_else(String::new, |member| member.owner.name.clone());
    Err(SomoniError::at(
        class.name_span,
        ErrorCode::AbstractNotOverridden {
            class: owner,
            method: name.clone(),
        },
    ))
}

/// Returns T058 if a class extends a class whose constructor is private.
fn check_parent_constructor(
    class: &ClassDecl,
    parent: Option<&ClassInfo>,
) -> Result<(), SomoniError> {
    let (Some(parent), Some((name, span))) = (parent, &class.extends) else {
        return Ok(());
    };
    if parent.constructor_visibility != Visibility::Private {
        return Ok(());
    }
    Err(SomoniError::at(
        *span,
        ErrorCode::PrivateMember {
            class: name.clone(),
            name: CONSTRUCTOR.to_string(),
        },
    ))
}

/// Returns what the compiler needs to know about a class.
fn class_plan(info: &ClassInfo, parent: Option<&ClassInfo>) -> ClassPlan {
    ClassPlan {
        fields: info.fields.iter().map(|(name, _)| name.clone()).collect(),
        field_offset: info.first_own_field,
        params: info.constructor.params.len(),
        parent_init: parent.map(|p| p.constructor.index),
        methods: info.method_table.clone(),
        vtables: info.vtables.clone(),
    }
}

/// Returns T054 if the constructor uses `this` before it set every field without a start value, or T050 for a field it never sets.
fn check_fields_set(class: &ClassDecl) -> Result<(), SomoniError> {
    let body = class.constructor.as_ref().map_or(&[][..], |c| &c.body);
    let mut missing: Vec<&ClassField> = class.fields.iter().filter(|f| f.start.is_none()).collect();
    for stmt in body {
        if missing.is_empty() {
            return Ok(());
        }
        if let Some(name) = sets_field(stmt) {
            missing.retain(|field| field.field.name != name);
        } else if let Some(span) = stmt.find_this() {
            let missing = quoted_names(&missing);
            return Err(SomoniError::at(
                span,
                ErrorCode::ThisBeforeFieldsSet { missing },
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

/// Returns the names of fields in quotes for a message, e.g. `'a', 'b'`.
fn quoted_names(fields: &[&ClassField]) -> String {
    let names: Vec<String> = fields
        .iter()
        .map(|field| format!("'{}'", field.field.name))
        .collect();
    names.join(", ")
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

/// Describes a method with its visibility for a message, e.g. `public laut(): string`.
pub(super) fn describe_method(name: &str, visibility: Visibility, signature: &Signature) -> String {
    format!(
        "{} {}",
        visibility.keyword(),
        describe_signature(name, signature)
    )
}

/// Describes a method for a message, e.g. `laut(): string`.
pub(super) fn describe_signature(name: &str, signature: &Signature) -> String {
    let params: Vec<String> = signature.params.iter().map(Type::to_string).collect();
    format!("{}({}): {}", name, params.join(", "), signature.return_type)
}

/// Returns the field that `this.field = value;` sets, if `value` does not use `this`.
fn sets_field(stmt: &Stmt) -> Option<&str> {
    let Stmt::Assign(Assign {
        target,
        op: None,
        value,
        ..
    }) = stmt
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
