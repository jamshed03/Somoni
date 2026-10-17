//! Interfaces: checking `implements` and building the vtables.

use std::collections::HashMap;

use crate::ast::{ClassDecl, InterfaceLayout, Visibility};
use crate::error::{ErrorCode, SomoniError};
use crate::types::Decl;

use super::classes::describe_method;
use super::{InterfaceInfo, Member, MethodInfo, TypeChecker};

impl TypeChecker {
    /// Checks that a class has every method of the interfaces it implements.
    pub(super) fn check_implements(
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

    /// Returns the vtable of a class for an interface.
    pub(super) fn vtable(
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

    /// Returns what the type checker knows about an interface.
    pub(super) fn interface_info(
        &self,
        layout: &Decl<InterfaceLayout>,
    ) -> Result<&InterfaceInfo, SomoniError> {
        self.interfaces
            .get(layout)
            .ok_or_else(|| SomoniError::internal("interface without methods"))
    }
}
