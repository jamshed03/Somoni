//! The parser, which turns the tokens of the lexer into a tree; its parts are spread over this folder.

use crate::ast::{Expr, ExprKind, FN_TYPE, Field, Param, Stmt, TUPLE_TYPE, TypeName};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::token::{SpannedToken, Token};

mod classes;
mod declarations;
mod expressions;
mod statements;

/// Turns the tokens of the lexer into a tree of statements and expressions.
pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}

impl Parser {
    /// Starts at the first token.
    pub fn new(tokens: Vec<SpannedToken>) -> Parser {
        Parser { tokens, pos: 0 }
    }

    /// Reads the whole input as one expression, like `x * 2` in the REPL.
    pub fn parse_lone_expr(&mut self) -> Result<Expr, SomoniError> {
        let expr = self.parse_expr()?;
        if *self.peek() != Token::Eof {
            return Err(self.expected("end of the input after the expression"));
        }
        Ok(expr)
    }

    /// Reads a whole file, which may hold only declarations if it is `imported` (M003).
    pub fn parse_program(&mut self, imported: bool) -> Result<Vec<Stmt>, SomoniError> {
        let mut stmts = Vec::new();
        let mut only_imports = true;
        while *self.peek() != Token::Eof {
            if imported && !starts_declaration(self.peek()) {
                return Err(SomoniError::at(
                    self.current_span(),
                    ErrorCode::OnlyDeclarations,
                ));
            }
            let stmt = self.parse_top_level(only_imports)?;
            only_imports &= matches!(stmt, Stmt::Import(_));
            stmts.push(stmt);
        }
        Ok(stmts)
    }

    /// Reads one statement at the top level, where declarations are allowed and `import` only before all others.
    fn parse_top_level(&mut self, only_imports: bool) -> Result<Stmt, SomoniError> {
        match self.peek() {
            Token::Import if only_imports => self.parse_import(),
            Token::Export => self.parse_export(),
            Token::Fn => self.parse_fn(false),
            Token::Struct => self.parse_struct(false),
            Token::Enum => self.parse_enum(false),
            Token::Class | Token::Abstract => self.parse_class(false),
            Token::Interface => self.parse_interface(false),
            _ => self.parse_stmt(),
        }
    }

    /// Returns the current token.
    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    /// Returns the token after the current one.
    fn peek_next(&self) -> &Token {
        let index = (self.pos + 1).min(self.tokens.len() - 1);
        &self.tokens[index].token
    }

    /// Returns the place of the current token.
    fn current_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    /// Returns the place of the token that was read last.
    fn last_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1)].span
    }

    /// Returns the place from `start` to the token that was read last.
    fn since(&self, start: Span) -> Span {
        start.to(self.last_span())
    }

    /// Returns an expression of `kind` from `start` to the token that was read last.
    fn expr_since(&self, kind: ExprKind, start: Span) -> Expr {
        Expr {
            kind,
            span: self.since(start),
        }
    }

    /// The error "expected ...", placed right after the token that was read last.
    fn expected(&self, what: impl Into<String>) -> SomoniError {
        SomoniError::at(
            self.last_span().after(),
            ErrorCode::Expected { what: what.into() },
        )
    }

    /// Goes to the next token, but stays at the end of the input (Eof).
    fn advance(&mut self) {
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
    }

    /// Reads the token `expected`, or returns the error "expected `what`".
    fn expect(&mut self, expected: Token, what: &str) -> Result<(), SomoniError> {
        if *self.peek() == expected {
            self.advance();
            Ok(())
        } else {
            Err(self.expected(what))
        }
    }

    /// Reads a name that is a keyword only at this place, like `from` in an import.
    fn expect_word(&mut self, word: &str, what: &str) -> Result<(), SomoniError> {
        if !self.at_word(word) {
            return Err(self.expected(what));
        }
        self.advance();
        Ok(())
    }

    /// Returns true if the next token is the name `word`, like `constructor` in a class.
    fn at_word(&self, word: &str) -> bool {
        matches!(self.peek(), Token::Ident(name) if name == word)
    }

    /// Reads `token` if it comes next and returns whether it did.
    fn eat(&mut self, token: Token) -> bool {
        let found = *self.peek() == token;
        if found {
            self.advance();
        }
        found
    }

    /// Reads names separated by `,`, like `A, B` after `implements`.
    fn parse_names(&mut self, what: &str) -> Result<Vec<(String, Span)>, SomoniError> {
        let mut names = vec![self.expect_ident(what)?];
        while self.eat(Token::Comma) {
            names.push(self.expect_ident(what)?);
        }
        Ok(names)
    }

    /// Reads a name, or returns the error "expected `what`".
    fn expect_ident(&mut self, what: &str) -> Result<(String, Span), SomoniError> {
        let span = self.current_span();
        match self.peek().clone() {
            Token::Ident(name) => {
                self.advance();
                Ok((name, span))
            }
            _ => Err(self.expected(what)),
        }
    }

    /// Reads `a: int`, where `what` names it in error messages, e.g. `parameter`.
    fn parse_param(&mut self, what: &str) -> Result<Param, SomoniError> {
        let (name, name_span) = self.expect_ident(&format!("{} name", what))?;
        self.expect(Token::Colon, &format!("':' after {} name", what))?;
        let type_name = self.parse_type(&format!("{} type", what))?;
        Ok(Param {
            name,
            name_span,
            type_name: Some(type_name),
        })
    }

    /// Reads `x: int` or only `x`, for a lambda, a `for`-each loop or a tuple taken apart; `what` names it in messages.
    fn parse_param_maybe_typed(&mut self, what: &str) -> Result<Param, SomoniError> {
        let (name, name_span) = self.expect_ident(&format!("{} name", what))?;
        let type_name = if self.eat(Token::Colon) {
            Some(self.parse_type(&format!("{} type", what))?)
        } else {
            None
        };
        Ok(Param {
            name,
            name_span,
            type_name,
        })
    }

    /// Reads the parameters of a function, method or constructor up to `)`, after the `(`.
    fn parse_params(&mut self) -> Result<Vec<Param>, SomoniError> {
        self.parse_comma_list(Token::RParen, "')'", |this| this.parse_param("parameter"))
    }

    /// Reads `: type` after the parameters of a function or method.
    fn parse_return_type(&mut self) -> Result<TypeName, SomoniError> {
        self.expect(Token::Colon, "':' and a return type after ')'")?;
        self.parse_type("return type")
    }

    /// Reads one field `x: int` of a struct or class.
    /// It stays apart from `parse_param` because fields and parameters change for different reasons.
    fn parse_field(&mut self) -> Result<Field, SomoniError> {
        let (name, name_span) = self.expect_ident("field name")?;
        self.expect(Token::Colon, "':' after field name")?;
        let type_name = self.parse_type("field type")?;
        Ok(Field {
            name,
            name_span,
            type_name,
        })
    }

    /// Reads items separated by `,` up to `close`, after the opening bracket, e.g. the arguments `a, b)`.
    fn parse_comma_list<T>(
        &mut self,
        close: Token,
        close_text: &str,
        mut parse_item: impl FnMut(&mut Self) -> Result<T, SomoniError>,
    ) -> Result<Vec<T>, SomoniError> {
        let mut items = Vec::new();
        if self.eat(close.clone()) {
            return Ok(items);
        }
        loop {
            items.push(parse_item(self)?);
            if !self.eat(Token::Comma) {
                break;
            }
        }
        self.expect(close, close_text)?;
        Ok(items)
    }

    /// Reads items up to the `}` of the block opened at `open`, or returns P003 if it is not closed.
    fn parse_until_brace<T>(
        &mut self,
        open: Span,
        mut parse_item: impl FnMut(&mut Self) -> Result<T, SomoniError>,
    ) -> Result<Vec<T>, SomoniError> {
        let mut items = Vec::new();
        while !self.eat(Token::RBrace) {
            if *self.peek() == Token::Eof {
                return Err(SomoniError::at(open, ErrorCode::BlockNotClosed));
            }
            items.push(parse_item(self)?);
        }
        Ok(items)
    }

    /// Reads a type like `int`, `int?`, `list<int>` or `map<string, list<int>>`.
    fn parse_type(&mut self, what: &str) -> Result<TypeName, SomoniError> {
        match self.peek() {
            Token::Fn => return self.parse_fn_type(),
            Token::LParen => return self.parse_tuple_type(),
            _ => {}
        }
        let (name, start) = self.expect_ident(what)?;
        let mut args = Vec::new();
        if self.eat(Token::Less) {
            args = self.parse_comma_list(Token::Greater, "'>'", |this| this.parse_type("type"))?;
        }
        let optional = self.eat(Token::Question);
        Ok(TypeName {
            name,
            args,
            optional,
            span: self.since(start),
        })
    }

    /// Reads a tuple type like `(int, string)` or `(int, string)?`, kept as the type `()` with the element types as arguments.
    fn parse_tuple_type(&mut self) -> Result<TypeName, SomoniError> {
        let start = self.current_span();
        self.advance();
        let args = self.parse_comma_list(Token::RParen, "')'", |this| this.parse_type("type"))?;
        let optional = self.eat(Token::Question);
        Ok(TypeName {
            name: TUPLE_TYPE.to_string(),
            args,
            optional,
            span: self.since(start),
        })
    }

    /// Reads a function type like `fn(int, string): bool`, kept as the type `fn` with the parameters and then the result as arguments.
    fn parse_fn_type(&mut self) -> Result<TypeName, SomoniError> {
        let start = self.current_span();
        self.advance();
        self.expect(Token::LParen, "'(' after 'fn'")?;
        let mut args =
            self.parse_comma_list(Token::RParen, "')'", |this| this.parse_type("type"))?;
        self.expect(Token::Colon, "':' and the result type")?;
        args.push(self.parse_type("result type")?);
        Ok(TypeName {
            name: FN_TYPE.to_string(),
            args,
            optional: false,
            span: self.since(start),
        })
    }
}

/// Returns true for the tokens that start a declaration, the only statements an imported file may have.
fn starts_declaration(token: &Token) -> bool {
    matches!(
        token,
        Token::Import
            | Token::Export
            | Token::Fn
            | Token::Struct
            | Token::Enum
            | Token::Class
            | Token::Abstract
            | Token::Const
            | Token::Interface
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    /// Returns the tokens of `source`.
    fn lex(source: &str) -> Vec<SpannedToken> {
        Lexer::new(source, 0).tokenize().unwrap()
    }

    /// Returns the statements of `source`.
    fn parse(source: &str) -> Vec<Stmt> {
        Parser::new(lex(source)).parse_program(false).unwrap()
    }

    /// Returns the error of parsing `source` as text, like `Parse error [P001]: Line 1: expected ';'`.
    fn error(source: &str) -> String {
        Parser::new(lex(source))
            .parse_program(false)
            .unwrap_err()
            .to_string()
    }

    /// Returns `source` read as one expression in brackets, so `1 + 2 * 3` is `(Add 1 (Mul 2 3))`.
    fn shape(source: &str) -> String {
        show(&Parser::new(lex(source)).parse_lone_expr().unwrap())
    }

    /// Writes an expression in brackets with its operator first, and a call as `name(args)`.
    fn show(expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Int(n) => n.to_string(),
            ExprKind::Ident { name, .. } => name.clone(),
            ExprKind::Binary {
                left, op, right, ..
            } => format!("({:?} {} {})", op, show(left), show(right)),
            ExprKind::Unary { op, operand } => format!("({:?} {})", op, show(operand)),
            ExprKind::Call(call) => {
                let args: Vec<String> = call.args.iter().map(show).collect();
                format!("{}({})", call.name, args.join(", "))
            }
            other => format!("{:?}", other),
        }
    }

    #[test]
    fn multiplication_comes_before_addition() {
        assert_eq!(shape("1 + 2 * 3"), "(Add 1 (Mul 2 3))");
        assert_eq!(shape("(1 + 2) * 3"), "(Mul (Add 1 2) 3)");
    }

    #[test]
    fn the_same_level_goes_from_left_to_right() {
        assert_eq!(shape("10 - 4 - 3"), "(Sub (Sub 10 4) 3)");
    }

    #[test]
    fn and_comes_before_or_and_comparisons_after_arithmetic() {
        assert_eq!(shape("a || b && c"), "(Or a (And b c))");
        assert_eq!(shape("a + 1 < b * 2"), "(Less (Add a 1) (Mul b 2))");
    }

    #[test]
    fn unary_operators_bind_tightest() {
        assert_eq!(shape("-a * b"), "(Mul (Neg a) b)");
        assert_eq!(shape("!a && b"), "(And (Not a) b)");
    }

    #[test]
    fn else_if_is_an_else_with_an_if_inside() {
        let stmts = parse("if (a) { } else if (b) { } else { }");
        let [
            Stmt::If {
                else_branch: Some(inner),
                ..
            },
        ] = stmts.as_slice()
        else {
            panic!("no if: {:?}", stmts);
        };
        assert!(matches!(
            inner.as_ref(),
            Stmt::If { else_branch: Some(last), .. } if matches!(last.as_ref(), Stmt::Block(_))
        ));
    }

    #[test]
    fn a_name_after_for_starts_a_for_each_loop() {
        for source in [
            "for (n in l) { }",
            "for (n: int in l) { }",
            "for (i, n in l) { }",
        ] {
            assert!(
                matches!(parse(source).as_slice(), [Stmt::ForEach(_)]),
                "{}",
                source
            );
        }
        assert!(matches!(
            parse("for (var i = 0; i < 3; i++) { }").as_slice(),
            [Stmt::For { .. }]
        ));
    }

    #[test]
    fn while_var_is_while_true_around_an_if_var() {
        let stmts = parse("while (var line = input()) { }");
        let [
            Stmt::While {
                condition, body, ..
            },
        ] = stmts.as_slice()
        else {
            panic!("no while: {:?}", stmts);
        };
        assert_eq!(condition.kind, ExprKind::Bool(true));
        assert!(
            matches!(body.as_ref(), Stmt::Block(inner) if matches!(inner.as_slice(), [Stmt::IfVar(_)]))
        );
    }

    #[test]
    fn a_tuple_gets_a_written_type_only_when_every_name_has_one() {
        let tuple_typed = |source| match parse(source).as_slice() {
            [Stmt::Destructure(destructure)] => destructure.tuple.type_name.is_some(),
            other => panic!("no destructure: {:?}", other),
        };
        assert!(tuple_typed("var (q: int, r: int) = t;"));
        assert!(!tuple_typed("var (q, r: int) = t;"));
    }

    #[test]
    fn a_lone_expression_must_end_the_input() {
        assert_eq!(shape("x * 2"), "(Mul x 2)");
        assert!(Parser::new(lex("x * 2;")).parse_lone_expr().is_err());
    }

    #[test]
    fn a_module_name_from_an_earlier_import_joins_its_names() {
        let call = |namespaces: &[String]| {
            let expr = Parser::new(lex("math.square(3)"))
                .with_namespaces(namespaces)
                .parse_lone_expr()
                .unwrap();
            show(&expr)
        };
        assert_eq!(call(&["math".to_string()]), "math.square(3)");
        assert_ne!(call(&[]), "math.square(3)");
    }

    #[test]
    fn errors_say_what_is_missing() {
        assert!(error("var x = 1").ends_with("expected ';'"));
        assert!(error("if (a) print(1);").ends_with("expected '{' after if condition"));
        assert!(error("x + 1;").ends_with("expected '='"));
    }
}
