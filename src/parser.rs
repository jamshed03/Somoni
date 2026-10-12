use crate::ast::{BinaryOp, Expr, Stmt};
use crate::token::{SpannedToken, Token};

pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Parser {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
    }

    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        while *self.peek() != Token::Eof {
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    fn expect(&mut self, expected: Token, what: &str) -> Result<(), String> {
        let line = self.tokens[self.pos].line;
        if *self.peek() == expected {
            self.advance();
            Ok(())
        } else {
            Err(format!("Line {}: expected {}", line, what))
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<String, String> {
        let line = self.tokens[self.pos].line;
        match self.peek().clone() {
            Token::Ident(name) => {
                self.advance();
                Ok(name)
            }
            _ => Err(format!("Line {}: expected {}", line, what)),
        }
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        let line = self.tokens[self.pos].line;
        match self.peek().clone() {
            Token::Var => self.parse_var_decl(true, line),
            Token::Const => self.parse_var_decl(false, line),
            Token::Ident(name) if name == "print" => self.parse_print(line),
            other => Err(format!("Line {}: unexpected token {:?}", line, other)),
        }
    }

    fn parse_var_decl(&mut self, mutable: bool, line: usize) -> Result<Stmt, String> {
        self.advance(); // var or const
        let name = self.expect_ident("variable name")?;
        self.expect(Token::Colon, "':'")?;
        let type_name = self.expect_ident("type name")?;
        self.expect(Token::Equal, "'='")?;
        let value = self.parse_expr()?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::VarDecl { name, type_name, value, mutable, line })
    }

    fn parse_print(&mut self, line: usize) -> Result<Stmt, String> {
        self.advance(); // print
        self.expect(Token::LParen, "'('")?;
        let value = self.parse_expr()?;
        self.expect(Token::RParen, "')'")?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::Print { value, line })
    }

    pub fn parse_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_term()?;

        loop {
            let op = match self.peek() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_term()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_term(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_primary()?;

        loop {
            let op = match self.peek() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                _ => break,
            };
            self.advance();
            let right = self.parse_primary()?;
            left = Expr::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        let line = self.tokens[self.pos].line;
        match self.peek().clone() {
            Token::Int(n) => {
                self.advance();
                Ok(Expr::Int(n))
            }
            Token::Ident(name) => {
                self.advance();
                Ok(Expr::Ident(name))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                if *self.peek() != Token::RParen {
                    return Err(format!("Line {}: expected ')'", line));
                }
                self.advance();
                Ok(expr)
            }
            other => Err(format!("Line {}: unexpected token {:?}", line, other)),
        }
    }
}
