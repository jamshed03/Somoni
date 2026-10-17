//! Reading classes: header with `extends` and `implements`, fields, constructor and methods.

use std::rc::Rc;

use crate::ast::{
    ClassDecl, ClassField, ClassLayout, ClassPlan, Constructor, Method, Stmt, Visibility,
};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::Token;

use super::Parser;

/// The members of a class while the parser reads them.
#[derive(Default)]
struct ClassBody {
    fields: Vec<ClassField>,
    constructor: Option<Constructor>,
    methods: Vec<Method>,
}

impl Parser {
    /// Reads `class Name extends Parent implements A, B { ... }`.
    pub(super) fn parse_class(&mut self, exported: bool) -> Result<Stmt, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("class name")?;
        let extends = if self.eat(Token::Extends) {
            Some(self.expect_ident("class name after 'extends'")?)
        } else {
            None
        };
        let implements = if self.eat(Token::Implements) {
            self.parse_names("interface name after 'implements'")?
        } else {
            Vec::new()
        };
        let body = self.parse_class_body()?;
        let layout = ClassLayout {
            name,
            fields: body.fields.iter().map(|f| f.field.name.clone()).collect(),
        };
        Ok(Stmt::ClassDecl(ClassDecl {
            exported,
            layout: Rc::new(layout),
            name_span,
            fields: body.fields,
            constructor: body.constructor,
            methods: body.methods,
            extends,
            implements,
            plan: ClassPlan::default(),
        }))
    }

    /// Reads the `{ ... }` of a class.
    fn parse_class_body(&mut self) -> Result<ClassBody, SomoniError> {
        let open = self.current_span();
        self.expect(Token::LBrace, "'{' after class name")?;
        let mut body = ClassBody::default();
        while !self.eat(Token::RBrace) {
            if *self.peek() == Token::Eof {
                return Err(SomoniError::at(open, ErrorCode::BlockNotClosed));
            }
            self.parse_class_member(&mut body)?;
        }
        Ok(body)
    }

    /// Reads one field, the constructor or one method of a class into `body`.
    fn parse_class_member(&mut self, body: &mut ClassBody) -> Result<(), SomoniError> {
        let visibility = self.parse_visibility()?;
        let is_override = self.eat(Token::Override);
        if self.at_word("constructor") {
            return self.parse_class_constructor(visibility, body);
        }
        if let Token::Ident(name) = self.peek().clone()
            && *self.peek_next() == Token::LParen
        {
            let name_span = self.current_span();
            self.advance();
            let function = self.parse_function_rest(false, name, name_span)?;
            body.methods.push(Method {
                visibility,
                is_override,
                function,
            });
            return Ok(());
        }
        body.fields.push(self.parse_class_field(visibility)?);
        Ok(())
    }

    /// Reads the constructor of a class, which must be public and the only one.
    fn parse_class_constructor(
        &mut self,
        visibility: Visibility,
        body: &mut ClassBody,
    ) -> Result<(), SomoniError> {
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
        if body.constructor.is_some() {
            return Err(SomoniError::at(span, ErrorCode::DuplicateConstructor));
        }
        body.constructor = Some(self.parse_constructor(span)?);
        Ok(())
    }

    /// Reads `name: type;` or `name: type = start;` of a class.
    fn parse_class_field(&mut self, visibility: Visibility) -> Result<ClassField, SomoniError> {
        let field = self.parse_field()?;
        let start = if self.eat(Token::Equal) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(Token::Semicolon, "';' after the field")?;
        Ok(ClassField {
            visibility,
            field,
            start,
        })
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
}
