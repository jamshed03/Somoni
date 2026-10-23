//! Interfaces: their methods, checking `implements` and building the vtables.

use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::{self, ClassDecl, InterfaceDecl, InterfaceLayout, Stmt, VTable, Visibility};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::types::Decl;

use super::classes::describe_method;
use super::members::ClassMembers;
use super::{ClassInfo, InterfaceInfo, Member, MethodInfo, Signature, TypeChecker};

impl TypeChecker {
    /// Collects the method signatures of all interfaces of a file.
    pub(super) fn declare_interface_methods(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for decl in ast::interfaces(stmts) {
            let methods = self.interface_methods(decl)?;
            if let Some(info) = self.interfaces.get_mut(&Decl(Rc::clone(&decl.layout))) {
                info.methods = methods;
            }
        }
        Ok(())
    }

    /// Returns the name and signature of each method of an interface, in declaration order.
    fn interface_methods(
        &self,
        decl: &InterfaceDecl,
    ) -> Result<Vec<(String, Signature)>, SomoniError> {
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
            let signature = self.signature_from(position, &method.params, &method.return_type)?;
            methods.push((method.name.clone(), signature));
        }
        Ok(methods)
    }

    /// Checks that a class has every method of the interfaces it implements.
    pub(super) fn check_implements(
        &self,
        class: &ClassDecl,
        methods: &HashMap<String, MethodInfo>,
        members: &HashMap<String, Member>,
    ) -> Result<Vec<Decl<InterfaceLayout>>, SomoniError> {
        let mut implements = Vec::new();
        for (name, span) in &class.implements {
            let interface = self.interface_named(name, *span)?;
            self.check_interface_methods(class, &interface, *span, methods, members)?;
            implements.push(interface);
        }
        Ok(implements)
    }

    /// Returns the interface with this name: T062 if it is another type, T001 if there is none.
    fn interface_named(
        &self,
        name: &str,
        span: Span,
    ) -> Result<Decl<InterfaceLayout>, SomoniError> {
        if let Some(layout) = self.names.interfaces.get(name) {
            return Ok(layout.clone());
        }
        let code = if self.is_type_name(name) {
            ErrorCode::NotAnInterface {
                name: name.to_string(),
            }
        } else {
            ErrorCode::UnknownType {
                name: name.to_string(),
            }
        };
        Err(SomoniError::at(span, code))
    }

    /// Checks that a class has every method of one interface, public and with the same types.
    fn check_interface_methods(
        &self,
        class: &ClassDecl,
        interface: &Decl<InterfaceLayout>,
        span: Span,
        methods: &HashMap<String, MethodInfo>,
        members: &HashMap<String, Member>,
    ) -> Result<(), SomoniError> {
        for (method_name, wanted) in &self.interface_info(interface)?.methods {
            let (Some(found), Some(member)) = (methods.get(method_name), members.get(method_name))
            else {
                return Err(SomoniError::at(
                    span,
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
                .map_or(span, |m| m.function.name_span);
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
        Ok(())
    }

    /// Returns the interfaces of a class: those of its parent class and those after `implements`.
    pub(super) fn all_interfaces(
        &self,
        class: &ClassDecl,
        built: &ClassMembers,
        parent: Option<&ClassInfo>,
    ) -> Result<Vec<Decl<InterfaceLayout>>, SomoniError> {
        let mut implements = parent.map_or(Vec::new(), |p| p.implements.clone());
        for interface in self.check_implements(class, &built.methods, &built.members)? {
            if !implements.contains(&interface) {
                implements.push(interface);
            }
        }
        Ok(implements)
    }

    /// Returns the vtable of a class for an interface.
    pub(super) fn vtable(
        &self,
        interface: &Decl<InterfaceLayout>,
        methods: &HashMap<String, MethodInfo>,
        table: &[usize],
    ) -> Result<VTable, SomoniError> {
        let info = self.interface_info(interface)?;
        let mut functions = Vec::new();
        for (name, _) in &info.methods {
            let method = methods
                .get(name)
                .ok_or_else(|| SomoniError::internal("interface method missing"))?;
            functions.push(table[method.slot]);
        }
        Ok(VTable {
            interface: info.index,
            functions,
        })
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
