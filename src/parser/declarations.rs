//! Reading declarations: `fn`, `struct`, `enum`, `class`, `interface`, `import`, `export`.

use std::rc::Rc;

use crate::ast::{
    EnumDecl, Field, Function, Import, ImportName, InterfaceDecl, InterfaceLayout, InterfaceMethod,
    Stmt, StructDecl,
};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::{SpannedToken, Token};

use super::Parser;

impl Parser {
    /// Reads `import { square, Point } from "math.smn";`.
    pub(super) fn parse_import(&mut self) -> Result<Stmt, SomoniError> {
        let start = self.current_span();
        self.advance();
        let (names, namespace) = if self.eat(Token::Star) {
            self.expect_word("as", "'as' after 'import *'")?;
            let namespace = self.expect_ident("module name after 'as'")?;
            self.merge_namespace(&namespace.0);
            (Vec::new(), Some(namespace))
        } else {
            self.expect(Token::LBrace, "'{' or '*' after 'import'")?;
            let names = self.parse_comma_list(Token::RBrace, "'}'", Self::parse_import_name)?;
            (names, None)
        };
        self.expect_word("from", "'from' after the imported names")?;
        let path_span = self.current_span();
        let Token::Str(path) = self.peek().clone() else {
            return Err(self.expected("a file name like \"math.smn\" after 'from'"));
        };
        self.advance();
        let span = self.since(start);
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::Import(Import {
            names,
            namespace,
            path,
            path_span,
            file: None,
            span,
        }))
    }

    /// Reads the tokens as if the modules `namespaces` were imported before, like in an earlier input of the REPL.
    pub fn with_namespaces(mut self, namespaces: &[String]) -> Parser {
        for namespace in namespaces {
            self.merge_namespace(namespace);
        }
        self
    }

    /// Turns `namespace . name` in all later tokens into the one name `namespace.name`, which no one can write by hand.
    /// Imports come first in a file, so every use of the module comes after this.
    fn merge_namespace(&mut self, namespace: &str) {
        let rest = self.tokens.split_off(self.pos);
        let mut i = 0;
        while i < rest.len() {
            let after_dot = self.tokens.last().is_some_and(|t| t.token == Token::Dot);
            match qualified_name(&rest[i..], namespace).filter(|_| !after_dot) {
                Some(token) => {
                    self.tokens.push(token);
                    i += 3;
                }
                None => {
                    self.tokens.push(rest[i].clone());
                    i += 1;
                }
            }
        }
    }

    /// Reads one name in the `{ ... }` of an import, like `square` or `square as q`.
    fn parse_import_name(&mut self) -> Result<ImportName, SomoniError> {
        let (name, span) = self.expect_ident("name to import")?;
        let alias = if self.at_word("as") {
            self.advance();
            Some(self.expect_ident("new name after 'as'")?)
        } else {
            None
        };
        Ok(ImportName { name, span, alias })
    }

    /// Reads `export` and the declaration after it.
    pub(super) fn parse_export(&mut self) -> Result<Stmt, SomoniError> {
        self.advance();
        match self.peek() {
            Token::Fn => self.parse_fn(true),
            Token::Struct => self.parse_struct(true),
            Token::Enum => self.parse_enum(true),
            Token::Class | Token::Abstract => self.parse_class(true),
            Token::Interface => self.parse_interface(true),
            Token::Const => {
                let mut decl = self.parse_var_decl(false, self.current_span())?;
                decl.exported = true;
                Ok(Stmt::VarDecl(Box::new(decl)))
            }
            _ => Err(self.expected(
                "'fn', 'struct', 'enum', 'class', 'abstract class', 'interface' or 'const' after 'export'",
            )),
        }
    }

    /// Reads `fn name(a: int, b: int): int { ... }`.
    pub(super) fn parse_fn(&mut self, exported: bool) -> Result<Stmt, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("function name")?;
        let function = self.parse_function_rest(exported, name, name_span)?;
        Ok(Stmt::FnDecl(function))
    }

    /// Reads the part of a function or method after its name: `(a: int): int { ... }`.
    pub(super) fn parse_function_rest(
        &mut self,
        exported: bool,
        name: String,
        name_span: Span,
    ) -> Result<Function, SomoniError> {
        self.expect(Token::LParen, "'(' after function name")?;
        let params = self.parse_params()?;
        let return_type = self.parse_return_type()?;
        let body = self.expect_block_stmts("after function header")?;
        Ok(Function {
            exported,
            name,
            name_span,
            params,
            return_type: Some(return_type),
            body,
        })
    }

    /// Reads `struct Point { x: int, y: int }`.
    pub(super) fn parse_struct(&mut self, exported: bool) -> Result<Stmt, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("struct name")?;
        self.expect(Token::LBrace, "'{' after struct name")?;
        let fields = self.parse_comma_list(Token::RBrace, "'}'", Self::parse_struct_field)?;
        Ok(Stmt::StructDecl(Rc::new(StructDecl {
            exported,
            name,
            name_span,
            fields,
        })))
    }

    /// Reads one field of a struct, or returns P015 for `readonly`, which a struct (a copy) does not need.
    fn parse_struct_field(&mut self) -> Result<Field, SomoniError> {
        if *self.peek() == Token::Readonly {
            return Err(SomoniError::at(
                self.current_span(),
                ErrorCode::ReadonlyNotAllowed,
            ));
        }
        self.parse_field()
    }

    /// Reads `interface Name { method(a: int): type; ... }`.
    pub(super) fn parse_interface(&mut self, exported: bool) -> Result<Stmt, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("interface name")?;
        let open = self.current_span();
        self.expect(Token::LBrace, "'{' after interface name")?;
        let methods = self.parse_until_brace(open, Self::parse_interface_method)?;
        let layout = InterfaceLayout {
            name,
            file: name_span.file as usize,
            methods: methods.iter().map(|m| m.name.clone()).collect(),
        };
        Ok(Stmt::InterfaceDecl(InterfaceDecl {
            exported,
            layout: Rc::new(layout),
            name_span,
            methods,
        }))
    }

    /// Reads one method `name(a: int): type;` of an interface, which has no body.
    fn parse_interface_method(&mut self) -> Result<InterfaceMethod, SomoniError> {
        let (name, name_span) = self.expect_ident("method name")?;
        self.expect(Token::LParen, "'(' after method name")?;
        let params = self.parse_params()?;
        let return_type = self.parse_return_type()?;
        self.expect(
            Token::Semicolon,
            "';' after the method (an interface has no bodies)",
        )?;
        Ok(InterfaceMethod {
            name,
            name_span,
            params,
            return_type,
        })
    }

    /// Reads `enum Color { Red, Green, Blue }`.
    pub(super) fn parse_enum(&mut self, exported: bool) -> Result<Stmt, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("enum name")?;
        self.expect(Token::LBrace, "'{' after enum name")?;
        let variants = self.parse_comma_list(Token::RBrace, "'}'", |this| {
            this.expect_ident("variant name")
        })?;
        Ok(Stmt::EnumDecl(Rc::new(EnumDecl {
            exported,
            name,
            name_span,
            variants,
        })))
    }
}

/// Returns `namespace.name` as one token if `tokens` start with `namespace . name`.
fn qualified_name(tokens: &[SpannedToken], namespace: &str) -> Option<SpannedToken> {
    let [first, dot, next, ..] = tokens else {
        return None;
    };
    match (&first.token, &dot.token, &next.token) {
        (Token::Ident(module), Token::Dot, Token::Ident(name)) if module == namespace => {
            Some(SpannedToken {
                token: Token::Ident(format!("{}.{}", namespace, name)),
                span: first.span.to(next.span),
            })
        }
        _ => None,
    }
}
