//! Reading classes: header with `extends` and `implements`, fields, constructor and methods.

use std::rc::Rc;

use crate::ast::{
    CONSTRUCTOR, ClassDecl, ClassField, ClassLayout, ClassPlan, Constructor, Method, Stmt,
    Visibility,
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

/// The words in front of a member: its visibility, `static` and `readonly` with their places, and `override`.
struct Modifiers {
    visibility: Visibility,
    is_static: Option<Span>,
    readonly: Option<Span>,
    is_override: bool,
}

impl Modifiers {
    /// Returns P015 if `readonly` stands in front of a member that is no field.
    fn forbid_readonly(&self) -> Result<(), SomoniError> {
        forbid(self.readonly, ErrorCode::ReadonlyNotAllowed)
    }

    /// Returns P016 if `static` stands in front of a member that is no method, or together with `override`.
    fn forbid_static(&self) -> Result<(), SomoniError> {
        forbid(self.is_static, ErrorCode::StaticNotAllowed)
    }
}

/// Returns the error `code` at the place of a word in front of a member, if the word is there.
fn forbid(word: Option<Span>, code: ErrorCode) -> Result<(), SomoniError> {
    match word {
        Some(span) => Err(SomoniError::at(span, code)),
        None => Ok(()),
    }
}

impl Parser {
    /// Reads `class Name extends Parent implements A, B { ... }`.
    pub(super) fn parse_class(&mut self, exported: bool) -> Result<Stmt, SomoniError> {
        self.advance();
        let (name, name_span) = self.expect_ident("class name")?;
        let extends = self.parse_extends()?;
        let implements = self.parse_implements()?;
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

    /// Reads `extends Parent` after the class name, if it is there.
    fn parse_extends(&mut self) -> Result<Option<(String, Span)>, SomoniError> {
        if !self.eat(Token::Extends) {
            return Ok(None);
        }
        Ok(Some(self.expect_ident("class name after 'extends'")?))
    }

    /// Reads `implements A, B` after the class name, if it is there.
    fn parse_implements(&mut self) -> Result<Vec<(String, Span)>, SomoniError> {
        if !self.eat(Token::Implements) {
            return Ok(Vec::new());
        }
        self.parse_names("interface name after 'implements'")
    }

    /// Reads the `{ ... }` of a class.
    fn parse_class_body(&mut self) -> Result<ClassBody, SomoniError> {
        let open = self.current_span();
        self.expect(Token::LBrace, "'{' after class name")?;
        let mut body = ClassBody::default();
        self.parse_until_brace(open, |this| this.parse_class_member(&mut body))?;
        Ok(body)
    }

    /// Reads one field, the constructor or one method of a class into `body`.
    fn parse_class_member(&mut self, body: &mut ClassBody) -> Result<(), SomoniError> {
        let modifiers = self.parse_modifiers()?;
        if self.at_word(CONSTRUCTOR) {
            modifiers.forbid_static()?;
            modifiers.forbid_readonly()?;
            return self.parse_class_constructor(modifiers.visibility, body);
        }
        if let Token::Ident(name) = self.peek().clone()
            && *self.peek_next() == Token::LParen
        {
            if modifiers.is_override {
                modifiers.forbid_static()?;
            }
            modifiers.forbid_readonly()?;
            body.methods.push(self.parse_method(name, &modifiers)?);
            return Ok(());
        }
        modifiers.forbid_static()?;
        body.fields.push(self.parse_class_field(&modifiers)?);
        Ok(())
    }

    /// Reads the visibility, `static`, `readonly` and `override` in front of a member, in this order.
    fn parse_modifiers(&mut self) -> Result<Modifiers, SomoniError> {
        let visibility = self.parse_visibility()?;
        let is_static = self.eat_word_at(Token::Static);
        let readonly = self.eat_word_at(Token::Readonly);
        let is_override = self.eat(Token::Override);
        Ok(Modifiers {
            visibility,
            is_static,
            readonly,
            is_override,
        })
    }

    /// Reads `token` if it comes next and returns its place, or None if it is not there.
    fn eat_word_at(&mut self, token: Token) -> Option<Span> {
        let span = self.current_span();
        self.eat(token).then_some(span)
    }

    /// Reads a method `name(a: int): type { ... }` whose name is the current token.
    fn parse_method(&mut self, name: String, modifiers: &Modifiers) -> Result<Method, SomoniError> {
        let name_span = self.current_span();
        self.advance();
        let function = self.parse_function_rest(false, name, name_span)?;
        Ok(Method {
            visibility: modifiers.visibility,
            is_static: modifiers.is_static.is_some(),
            is_override: modifiers.is_override,
            function,
        })
    }

    /// Reads the constructor of a class, which must be the only one.
    fn parse_class_constructor(
        &mut self,
        visibility: Visibility,
        body: &mut ClassBody,
    ) -> Result<(), SomoniError> {
        let span = self.current_span();
        if body.constructor.is_some() {
            return Err(SomoniError::at(span, ErrorCode::DuplicateConstructor));
        }
        body.constructor = Some(self.parse_constructor(visibility, span)?);
        Ok(())
    }

    /// Reads `name: type;` or `name: type = start;` of a class.
    fn parse_class_field(&mut self, modifiers: &Modifiers) -> Result<ClassField, SomoniError> {
        let field = self.parse_field()?;
        let start = if self.eat(Token::Equal) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(Token::Semicolon, "';' after the field")?;
        Ok(ClassField {
            visibility: modifiers.visibility,
            readonly: modifiers.readonly.is_some(),
            field,
            start,
        })
    }

    /// Reads `public`, `protected` or `private` in front of a field or method, public without it.
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

    /// Reads `constructor(a: int) { ... }` whose `constructor` is the current token.
    fn parse_constructor(
        &mut self,
        visibility: Visibility,
        span: Span,
    ) -> Result<Constructor, SomoniError> {
        self.advance();
        self.expect(Token::LParen, "'(' after 'constructor'")?;
        let params = self.parse_params()?;
        let body = self.expect_block_stmts("after the constructor parameters")?;
        Ok(Constructor {
            visibility,
            params,
            body,
            span,
        })
    }
}
