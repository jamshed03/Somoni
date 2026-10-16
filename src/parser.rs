use crate::ast::{BinaryOp, Expr, Stmt, UnaryOp};
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

    fn line_after_last_token(&self) -> usize {
        let index = self.pos.saturating_sub(1);
        self.tokens[index].line
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
        if *self.peek() == expected {
            self.advance();
            Ok(())
        } else {
            Err(format!(
                "Line {}: expected {}",
                self.line_after_last_token(),
                what
            ))
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<String, String> {
        match self.peek().clone() {
            Token::Ident(name) => {
                self.advance();
                Ok(name)
            }
            _ => Err(format!(
                "Line {}: expected {}",
                self.line_after_last_token(),
                what
            )),
        }
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        let line = self.tokens[self.pos].line;
        match self.peek().clone() {
            Token::Var => self.parse_var_decl(true, line),
            Token::Const => self.parse_var_decl(false, line),
            Token::LBrace => self.parse_block(line),
            Token::If => self.parse_if(line),
            Token::Ident(name) if name == "print" => self.parse_print(line),
            Token::Ident(name) => self.parse_assign(name, line),
            other => Err(format!("Line {}: unexpected token {:?}", line, other)),
        }
    }

    // { stmt* } – no ';' after the closing brace
    fn parse_block(&mut self, open_line: usize) -> Result<Stmt, String> {
        self.advance(); // '{'
        let mut stmts = Vec::new();
        while *self.peek() != Token::RBrace {
            if *self.peek() == Token::Eof {
                return Err(format!(
                    "Line {}: block is not closed, expected '}}'",
                    open_line
                ));
            }
            stmts.push(self.parse_stmt()?);
        }
        self.advance(); // '}'
        Ok(Stmt::Block(stmts))
    }

    // a block where the language requires one, e.g. after `if (...)`
    fn expect_block(&mut self, what: &str) -> Result<Stmt, String> {
        if *self.peek() != Token::LBrace {
            return Err(format!(
                "Line {}: expected '{{' {}",
                self.line_after_last_token(),
                what
            ));
        }
        let open_line = self.tokens[self.pos].line;
        self.parse_block(open_line)
    }

    // if (cond) { ... } [else if (cond) { ... }]* [else { ... }]
    fn parse_if(&mut self, line: usize) -> Result<Stmt, String> {
        self.advance(); // 'if'
        self.expect(Token::LParen, "'(' after 'if'")?;
        let condition = self.parse_expr()?;
        self.expect(Token::RParen, "')' after if condition")?;
        let then_branch = self.expect_block("after if condition")?;

        let else_branch = if *self.peek() == Token::Else {
            self.advance(); // 'else'
            let else_line = self.tokens[self.pos].line;
            match self.peek() {
                // `else if` is an else whose only content is another if
                Token::If => Some(Box::new(self.parse_if(else_line)?)),
                Token::LBrace => Some(Box::new(self.parse_block(else_line)?)),
                _ => {
                    return Err(format!(
                        "Line {}: expected '{{' or 'if' after 'else'",
                        self.line_after_last_token()
                    ));
                }
            }
        } else {
            None
        };

        Ok(Stmt::If {
            condition,
            then_branch: Box::new(then_branch),
            else_branch,
            line,
        })
    }

    fn parse_assign(&mut self, name: String, line: usize) -> Result<Stmt, String> {
        self.advance(); // name
        self.expect(Token::Equal, "'='")?;
        let value = self.parse_expr()?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::Assign { name, value, line })
    }

    fn parse_var_decl(&mut self, mutable: bool, line: usize) -> Result<Stmt, String> {
        self.advance(); // var or const
        let name = self.expect_ident("variable name")?;
        self.expect(Token::Colon, "':'")?;
        let type_name = self.expect_ident("type name")?;
        self.expect(Token::Equal, "'='")?;
        let value = self.parse_expr()?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::VarDecl {
            name,
            type_name,
            value,
            mutable,
            line,
        })
    }

    fn parse_print(&mut self, line: usize) -> Result<Stmt, String> {
        self.advance(); // print
        self.expect(Token::LParen, "'('")?;
        let value = self.parse_expr()?;
        self.expect(Token::RParen, "')'")?;
        self.expect(Token::Semicolon, "';'")?;
        Ok(Stmt::Print { value, line })
    }

    fn parse_binary(
        &mut self,
        next: fn(&mut Self) -> Result<Expr, String>,
        ops: &[(Token, BinaryOp)],
    ) -> Result<Expr, String> {
        let mut left = next(self)?;

        while let Some((_, op)) = ops.iter().find(|(token, _)| token == self.peek()) {
            self.advance();
            let right = next(self)?;
            left = Expr::Binary {
                left: Box::new(left),
                op: op.clone(),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    pub fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_binary(Self::parse_and, &[(Token::OrOr, BinaryOp::Or)])
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        self.parse_binary(Self::parse_equality, &[(Token::AndAnd, BinaryOp::And)])
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        self.parse_binary(
            Self::parse_comparison,
            &[
                (Token::EqualEqual, BinaryOp::Eq),
                (Token::BangEqual, BinaryOp::NotEq),
            ],
        )
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        self.parse_binary(
            Self::parse_additive,
            &[
                (Token::Less, BinaryOp::Less),
                (Token::Greater, BinaryOp::Greater),
                (Token::LessEqual, BinaryOp::LessEq),
                (Token::GreaterEqual, BinaryOp::GreaterEq),
            ],
        )
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        self.parse_binary(
            Self::parse_term,
            &[(Token::Plus, BinaryOp::Add), (Token::Minus, BinaryOp::Sub)],
        )
    }

    fn parse_term(&mut self) -> Result<Expr, String> {
        self.parse_binary(
            Self::parse_unary,
            &[
                (Token::Star, BinaryOp::Mul),
                (Token::Slash, BinaryOp::Div),
                (Token::Percent, BinaryOp::Mod),
            ],
        )
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        let op = match self.peek() {
            Token::Bang => UnaryOp::Not,
            Token::Minus => UnaryOp::Neg,
            _ => return self.parse_primary(),
        };
        self.advance();
        let operand = self.parse_unary()?;
        Ok(Expr::Unary {
            op,
            operand: Box::new(operand),
        })
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        let line = self.tokens[self.pos].line;
        match self.peek().clone() {
            Token::Int(n) => {
                self.advance();
                Ok(Expr::Int(n))
            }
            Token::Float(f) => {
                self.advance();
                Ok(Expr::Float(f))
            }
            Token::Str(s) => {
                self.advance();
                Ok(Expr::Str(s))
            }
            Token::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            Token::Ident(name) => {
                self.advance();
                Ok(Expr::Ident(name))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                if *self.peek() != Token::RParen {
                    return Err(format!(
                        "Line {}: expected ')'",
                        self.line_after_last_token()
                    ));
                }
                self.advance();
                Ok(expr)
            }
            other => Err(format!("Line {}: unexpected token {:?}", line, other)),
        }
    }
}
