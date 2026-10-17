//! The members of a class: inherited ones first, then own fields and methods, with the rules
//! for `override`.

use std::collections::HashMap;

use crate::ast::{ClassField, ClassLayout, Method, Visibility};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::types::Decl;

use super::classes::describe_method;
use super::{ClassInfo, Member, MethodInfo, Signature};

/// The members, methods and method table of a class while the type checker builds them.
pub(super) struct ClassMembers {
    pub(super) layout: Decl<ClassLayout>,
    pub(super) members: HashMap<String, Member>,
    pub(super) methods: HashMap<String, MethodInfo>,
    pub(super) table: Vec<usize>,
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
            },
            None => ClassMembers {
                layout,
                members: HashMap::new(),
                methods: HashMap::new(),
                table: Vec::new(),
            },
        }
    }

    /// Adds an own field whose name is not taken yet.
    pub(super) fn add_field(&mut self, field: &ClassField) -> Result<(), SomoniError> {
        let name = &field.field.name;
        if self.members.contains_key(name) {
            return Err(self.duplicate(name, field.field.name_span));
        }
        self.add_member(name, field.visibility, false);
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
                if member.is_method
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
        self.methods
            .insert(name.clone(), MethodInfo { slot, signature });
        self.add_member(name, method.visibility, true);
        Ok(())
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
        let same_types = old.signature.params == signature.params
            && old.signature.return_type == signature.return_type;
        if !same_types || inherited.visibility != method.visibility {
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
    fn add_member(&mut self, name: &str, visibility: Visibility, is_method: bool) {
        let member = Member {
            visibility,
            owner: self.layout.clone(),
            is_method,
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
