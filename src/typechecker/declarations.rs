//! Collecting the declarations of a file (enums, structs, functions) and turning written types into types.

use std::rc::Rc;

use crate::ast::{self, Field, Function, Param, Stmt, StructDecl, TypeName};
use crate::builtins::Builtin;
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::types::{Decl, Type};

use super::{ClassInfo, InterfaceInfo, Signature, StructInfo, TypeChecker};

impl TypeChecker {
    /// Collects all enums of a file.
    pub(super) fn declare_enums(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for decl in ast::enums(stmts) {
            self.check_new_type_name(&decl.name, decl.name_span)?;
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

    /// Returns true if `name` is already a type.
    pub(super) fn is_type_name(&self, name: &str) -> bool {
        Type::is_builtin_name(name) || self.names.type_named(name).is_some()
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

    /// Collects all structs, classes and interfaces of a file: names first, then their types.
    pub(super) fn declare_structs(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        self.declare_type_names(stmts)?;
        self.declare_interface_methods(stmts)?;
        self.declare_struct_fields(stmts)?;
        self.resolve_parents(stmts)?;
        for class in self.parents_first(stmts) {
            self.declare_class_fields(class)?;
        }
        self.check_no_type_contains_itself(stmts)
    }

    /// Registers the names of all structs, classes and interfaces of a file, still without fields and methods.
    fn declare_type_names(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
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
            let info = ClassInfo::new(layout.clone(), self.classes.len());
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
        Ok(())
    }

    /// Collects the fields of all structs of a file.
    fn declare_struct_fields(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for decl in ast::structs(stmts) {
            let fields = self.resolve_fields(&decl.fields)?;
            if let Some(info) = self.structs.get_mut(&Decl(Rc::clone(decl))) {
                info.fields = fields;
            }
        }
        Ok(())
    }

    /// Returns T030 if a struct or class of a file contains itself.
    fn check_no_type_contains_itself(&self, stmts: &[Stmt]) -> Result<(), SomoniError> {
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

    /// Returns the name and type of each field.
    pub(super) fn resolve_fields<'a>(
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

    /// Returns true if a value of type `from` contains a `target`, also through other fields.
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

    /// Returns the fields of a struct or class type.
    pub(super) fn fields_of(&self, ty: &Type) -> Option<&[(String, Type)]> {
        match ty {
            Type::Struct(decl) => self.structs.get(decl).map(|info| info.fields.as_slice()),
            Type::Class(layout) => self.classes.get(layout).map(|info| info.fields.as_slice()),
            _ => None,
        }
    }

    /// Collects the signatures of all functions of a file.
    pub(super) fn declare_functions(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
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

    /// Returns the signature of a function or method.
    pub(super) fn signature_of(
        &self,
        function: &Function,
        index: usize,
    ) -> Result<Signature, SomoniError> {
        self.signature_from(index, &function.params, &function.return_type)
    }

    /// Returns the signature with this number, parameters and return type.
    pub(super) fn signature_from(
        &self,
        index: usize,
        params: &[Param],
        return_type: &TypeName,
    ) -> Result<Signature, SomoniError> {
        Ok(Signature {
            index,
            params: self.resolve_params(params)?,
            return_type: self.resolve_type(return_type, true)?,
        })
    }

    /// Returns the types of the parameters.
    pub(super) fn resolve_params(&self, params: &[Param]) -> Result<Vec<Type>, SomoniError> {
        params
            .iter()
            .map(|param| self.resolve_type(&param.type_name, false))
            .collect()
    }

    /// Returns the struct with this name, or T001 if there is none.
    pub(super) fn struct_named(
        &self,
        name: &str,
        span: Span,
    ) -> Result<Decl<StructDecl>, SomoniError> {
        self.names.structs.get(name).cloned().ok_or_else(|| {
            SomoniError::at(
                span,
                ErrorCode::UnknownType {
                    name: name.to_string(),
                },
            )
        })
    }

    /// Returns what the type checker knows about a struct.
    pub(super) fn struct_info(&self, decl: &Decl<StructDecl>) -> Result<&StructInfo, SomoniError> {
        self.structs
            .get(decl)
            .ok_or_else(|| SomoniError::internal("struct without fields"))
    }

    /// Turns a written type like `list<int>?` into a `Type`.
    pub(super) fn resolve_type(
        &self,
        written: &TypeName,
        allow_void: bool,
    ) -> Result<Type, SomoniError> {
        let ty = self.resolve_without_optional(written, allow_void)?;
        if !written.optional {
            return Ok(ty);
        }
        if ty == Type::Void {
            return Err(SomoniError::at(written.span, ErrorCode::OptionalVoid));
        }
        Ok(Type::Optional(Box::new(ty)))
    }

    /// Turns a written type into a `Type` without its `?`.
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
        let simple = simple.or_else(|| self.names.type_named(&written.name));
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
}

/// Checks that a type may be the key of a map.
pub(super) fn check_key_type(ty: &Type, span: Span) -> Result<(), SomoniError> {
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
