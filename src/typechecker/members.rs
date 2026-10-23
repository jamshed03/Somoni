//! The members of a class, inherited ones first, with the rules for `override`.

use std::collections::HashMap;

use crate::ast::{ClassField, ClassLayout, Method, Visibility};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::types::{Decl, Type};

use super::classes::describe_method;
use super::{ClassInfo, Member, MemberKind, MethodInfo, Signature, StaticField, StaticMethod};

/// The members, methods and method table of a class while the type checker builds them.
pub(super) struct ClassMembers {
    pub(super) layout: Decl<ClassLayout>,
    pub(super) members: HashMap<String, Member>,
    pub(super) methods: HashMap<String, MethodInfo>,
    pub(super) table: Vec<usize>,
    pub(super) statics: HashMap<String, StaticMethod>,
    pub(super) static_fields: HashMap<String, StaticField>,
}

impl ClassMembers {
    /// Starts with what the class inherits from its parent class, if it has one.
    pub(super) fn inherit(layout: Decl<ClassLayout>, parent: Option<&ClassInfo>) -> ClassMembers {
        match parent {
            Some(parent) => ClassMembers {
                layout,
                members: parent.members.clone(),
                methods: parent.methods.clone(),
                table: parent.method_table.clone(),
                statics: HashMap::new(),
                static_fields: HashMap::new(),
            },
            None => ClassMembers {
                layout,
                members: HashMap::new(),
                methods: HashMap::new(),
                table: Vec::new(),
                statics: HashMap::new(),
                static_fields: HashMap::new(),
            },
        }
    }

    /// Adds an own field whose name is not taken yet.
    pub(super) fn add_field(&mut self, field: &ClassField) -> Result<(), SomoniError> {
        let name = &field.field.name;
        if self.members.contains_key(name) {
            return Err(self.duplicate(name, field.field.name_span));
        }
        self.add_member(
            name,
            field.visibility,
            MemberKind::Field {
                readonly: field.readonly,
            },
        );
        Ok(())
    }

    /// Adds an own method in the slot of the method it overrides or in a new slot.
    pub(super) fn add_method(
        &mut self,
        method: &Method,
        signature: Signature,
    ) -> Result<(), SomoniError> {
        let name = &method.function.name;
        let span = method.function.name_span;
        let slot = match self.members.get(name) {
            Some(member)
                if member.kind == MemberKind::Method
                    && member.visibility != Visibility::Private
                    && member.owner != self.layout =>
            {
                self.overridden_slot(method, &signature, member)?
            }
            Some(_) => return Err(self.duplicate(name, span)),
            None if method.is_override => {
                return Err(SomoniError::at(
                    span,
                    ErrorCode::NothingToOverride {
                        method: name.clone(),
                    },
                ));
            }
            None => {
                self.table.push(signature.index);
                self.table.len() - 1
            }
        };
        self.table[slot] = signature.index;
        self.methods.insert(
            name.clone(),
            MethodInfo {
                slot,
                signature,
                is_abstract: method.is_abstract,
            },
        );
        self.add_member(name, method.visibility, MemberKind::Method);
        Ok(())
    }

    /// Adds an own static method, whose name no field or other method may have; call it after all other members.
    pub(super) fn add_static(
        &mut self,
        method: &Method,
        signature: Signature,
    ) -> Result<(), SomoniError> {
        let name = &method.function.name;
        if self.has_name(name) {
            return Err(self.duplicate(name, method.function.name_span));
        }
        let visibility = method.visibility;
        let method = StaticMethod {
            visibility,
            signature,
        };
        self.statics.insert(name.clone(), method);
        Ok(())
    }

    /// Adds an own static field with its type and global number; call it after all other members.
    pub(super) fn add_static_field(
        &mut self,
        field: &ClassField,
        ty: Type,
        global: usize,
    ) -> Result<(), SomoniError> {
        let name = &field.field.name;
        if self.has_name(name) {
            return Err(self.duplicate(name, field.field.name_span));
        }
        let info = StaticField {
            visibility: field.visibility,
            ty,
            global,
        };
        self.static_fields.insert(name.clone(), info);
        Ok(())
    }

    /// Returns true if a field, method, static method or static field of this class has the name.
    fn has_name(&self, name: &str) -> bool {
        self.members.contains_key(name)
            || self.statics.contains_key(name)
            || self.static_fields.contains_key(name)
    }

    /// Returns the slot of the inherited method that `method` overrides with the same types.
    fn overridden_slot(
        &self,
        method: &Method,
        signature: &Signature,
        inherited: &Member,
    ) -> Result<usize, SomoniError> {
        let name = &method.function.name;
        let span = method.function.name_span;
        let old = self
            .methods
            .get(name)
            .ok_or_else(|| SomoniError::internal("inherited method without signature"))?;
        if !method.is_override {
            return Err(SomoniError::at(
                span,
                ErrorCode::MissingOverride {
                    method: name.clone(),
                    parent: inherited.owner.name.clone(),
                },
            ));
        }
        if !old.signature.same_types(signature) || inherited.visibility != method.visibility {
            return Err(SomoniError::at(
                span,
                ErrorCode::OverrideMismatch {
                    parent: inherited.owner.name.clone(),
                    method: describe_method(name, inherited.visibility, &old.signature),
                },
            ));
        }
        Ok(old.slot)
    }

    /// Records a field or method name of this class.
    fn add_member(&mut self, name: &str, visibility: Visibility, kind: MemberKind) {
        let member = Member {
            visibility,
            owner: self.layout.clone(),
            kind,
        };
        self.members.insert(name.to_string(), member);
    }

    /// Returns the error for a name that this class already has.
    fn duplicate(&self, name: &str, span: Span) -> SomoniError {
        SomoniError::at(
            span,
            ErrorCode::DuplicateMember {
                class: self.layout.name.clone(),
                name: name.to_string(),
            },
        )
    }
}
