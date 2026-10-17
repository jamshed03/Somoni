//! Imports and exports between the files of a program.

use crate::ast::{Import, Stmt};
use crate::error::{ErrorCode, SomoniError};

use super::{Exports, TypeChecker};

impl TypeChecker {
    /// Adds the names of all `import`s of a file to `names`.
    pub(super) fn import_names(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
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
    pub(super) fn exports_of(&self, stmts: &[Stmt]) -> Exports {
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
}
