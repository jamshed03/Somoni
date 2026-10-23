//! Reading declarations: `fn`, `struct`, `enum`, `class`, `interface`, `import`, `export`.

use std::rc::Rc;

use crate::ast::{
    EnumDecl, Field, Function, Import, ImportName, InterfaceDecl, InterfaceLayout, InterfaceMethod,
    Stmt, StructDecl,
};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::Token;

use super::Parser;

impl Parser {
    /// Reads `import { quadrat, Punkt } from "mathe.smn";`.
    pub(super) fn parse_import(&mut self) -> Result<Stmt, SomoniError> {
        let start = self.current_span();
        self.advance();
        self.expect(Token::LBrace, "'{' after 'import'")?;
        let names = self.parse_comma_list(Token::RBrace, "'}'", Self::parse_import_name)?;
        self.expect_word("from", "'from' after '}'")?;
        let path_span = self.current_span();
        let Token::Str(path) = self.peek().clone() else {
            return Err(self.expected("a file name like \"mathe.smn\" after 'from'"));
        };
        self.advance();
        let span = self.since(start);
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::Import(Import {
            names,
            path,
            path_span,
            file: None,
            span,
        }))
    }

    /// Reads one name in the `{ ... }` of an import.
    fn parse_import_name(&mut self) -> Result<ImportName, SomoniError> {
        let (name, span) = self.expect_ident("name to import")?;
        Ok(ImportName { name, span })
    }

    /// Reads `export` and the declaration after it.
    pub(super) fn parse_export(&mut self) -> Result<Stmt, SomoniError> {
        self.advance();
        match self.peek() {
            Token::Fn => self.parse_fn(true),
            Token::Struct => self.parse_struct(true),
            Token::Enum => self.parse_enum(true),
            Token::Class => self.parse_class(true),
            Token::Interface => self.parse_interface(true),
            _ => {
                Err(self.expected("'fn', 'struct', 'enum', 'class' or 'interface' after 'export'"))
            }
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
            return_type,
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

    /// Reads `enum Farbe { Rot, Gruen, Blau }`.
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
