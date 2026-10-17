//! Reading declarations: `fn`, `struct`, `enum`, `class`, `interface`, `import`, `export`.

use std::rc::Rc;

use crate::ast::{
    ClassDecl, ClassField, ClassLayout, ClassPlan, Constructor, EnumDecl, Function, Import,
    ImportName, InterfaceDecl, InterfaceLayout, InterfaceMethod, Method, Stmt, StructDecl,
    Visibility,
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
        let names = self.parse_comma_list(Token::RBrace, "'}'", |this| {
            let (name, span) = this.expect_ident("name to import")?;
            Ok(ImportName { name, span })
        })?;
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

    /// Reads `export` and the `fn`, `struct` or `enum` after it.
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
    fn parse_function_rest(
        &mut self,
        exported: bool,
        name: String,
        name_span: Span,
    ) -> Result<Function, SomoniError> {
        self.expect(Token::LParen, "'(' after function name")?;
        let params =
            self.parse_comma_list(Token::RParen, "')'", |this| this.parse_param("parameter"))?;
        self.expect(Token::Colon, "':' and a return type after ')'")?;
        let return_type = self.parse_type("return type")?;
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
        let fields = self.parse_comma_list(Token::RBrace, "'}'", Self::parse_field)?;
        Ok(Stmt::StructDecl(Rc::new(StructDecl {
            exported,
            name,
            name_span,
            fields,
        })))
    }

    /// Reads `class Name { field: type; field: type = start; }`.
    pub(super) fn parse_class(&mut self, exported: bool) -> Result<Stmt, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("class name")?;
        let mut extends = None;
        if *self.peek() == Token::Extends {
            self.advance();
            extends = Some(self.expect_ident("class name after 'extends'")?);
        }
        let mut implements = Vec::new();
        if *self.peek() == Token::Implements {
            self.advance();
            loop {
                implements.push(self.expect_ident("interface name after 'implements'")?);
                if *self.peek() != Token::Comma {
                    break;
                }
                self.advance();
            }
        }
        let open = self.current_span();
        self.expect(Token::LBrace, "'{' after class name")?;
        let mut fields = Vec::new();
        let mut constructor = None;
        let mut methods = Vec::new();
        while *self.peek() != Token::RBrace {
            if *self.peek() == Token::Eof {
                return Err(SomoniError::at(open, ErrorCode::BlockNotClosed));
            }
            let visibility = self.parse_visibility()?;
            let is_override = *self.peek() == Token::Override;
            if is_override {
                self.advance();
            }
            if matches!(self.peek(), Token::Ident(name) if name == "constructor") {
                let span = self.current_span();
                if visibility != Visibility::Public {
                    return Err(SomoniError::at(
                        span,
                        ErrorCode::NotYetSupported {
                            what: "'private constructor'",
                            comes_with: "static",
                        },
                    ));
                }
                if constructor.is_some() {
                    return Err(SomoniError::at(span, ErrorCode::DuplicateConstructor));
                }
                constructor = Some(self.parse_constructor(span)?);
                continue;
            }
            if let Token::Ident(name) = self.peek().clone()
                && *self.peek_next() == Token::LParen
            {
                let name_span = self.current_span();
                self.advance();
                let function = self.parse_function_rest(false, name, name_span)?;
                methods.push(Method {
                    visibility,
                    is_override,
                    function,
                });
                continue;
            }
            let field = self.parse_field()?;
            let start = if *self.peek() == Token::Equal {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };
            self.expect(Token::Semicolon, "';' after the field")?;
            fields.push(ClassField {
                visibility,
                field,
                start,
            });
        }
        self.advance();
        let layout = ClassLayout {
            name,
            fields: fields.iter().map(|f| f.field.name.clone()).collect(),
        };
        Ok(Stmt::ClassDecl(ClassDecl {
            exported,
            layout: Rc::new(layout),
            name_span,
            fields,
            constructor,
            methods,
            extends,
            implements,
            plan: ClassPlan::default(),
        }))
    }

    /// Reads `interface Name { method(a: int): type; ... }`.
    pub(super) fn parse_interface(&mut self, exported: bool) -> Result<Stmt, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("interface name")?;
        let open = self.current_span();
        self.expect(Token::LBrace, "'{' after interface name")?;
        let mut methods = Vec::new();
        while *self.peek() != Token::RBrace {
            if *self.peek() == Token::Eof {
                return Err(SomoniError::at(open, ErrorCode::BlockNotClosed));
            }
            let (method, method_span) = self.expect_ident("method name")?;
            self.expect(Token::LParen, "'(' after method name")?;
            let params =
                self.parse_comma_list(Token::RParen, "')'", |this| this.parse_param("parameter"))?;
            self.expect(Token::Colon, "':' and a return type after ')'")?;
            let return_type = self.parse_type("return type")?;
            self.expect(
                Token::Semicolon,
                "';' after the method (an interface has no bodies)",
            )?;
            methods.push(InterfaceMethod {
                name: method,
                name_span: method_span,
                params,
                return_type,
            });
        }
        self.advance();
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

    /// Reads `public`, `protected` or `private` in front of a field or method; without it,
    /// it is public.
    fn parse_visibility(&mut self) -> Result<Visibility, SomoniError> {
        let visibility = match self.peek() {
            Token::Public => Visibility::Public,
            Token::Private => Visibility::Private,
            Token::Protected => Visibility::Protected,
            _ => return Ok(Visibility::Public),
        };
        self.advance();
        Ok(visibility)
    }

    /// Reads `constructor(a: int) { ... }`. The current token is `constructor`.
    fn parse_constructor(&mut self, span: Span) -> Result<Constructor, SomoniError> {
        self.advance();
        self.expect(Token::LParen, "'(' after 'constructor'")?;
        let params =
            self.parse_comma_list(Token::RParen, "')'", |this| this.parse_param("parameter"))?;
        let body = self.expect_block_stmts("after the constructor parameters")?;
        Ok(Constructor { params, body, span })
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
