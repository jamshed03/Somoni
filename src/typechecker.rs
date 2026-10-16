use std::collections::HashMap;

use crate::ast::{BinaryOp, Expr, ExprKind, Function, Stmt, UnaryOp};
use crate::builtins::Builtin;
use crate::error::{ErrorCode, SomoniError};
use crate::scope::Scopes;
use crate::span::Span;
use crate::types::Type;

struct VarInfo {
    ty: Type,
    mutable: bool,
}

struct Signature {
    params: Vec<Type>,
    return_type: Type,
}

pub struct TypeChecker {
    scopes: Scopes<VarInfo>,
    loop_depth: usize,
    functions: HashMap<String, Signature>,
    // the return type of the function being checked, None at the top level
    current_return: Option<Type>,
}

impl TypeChecker {
    pub fn new() -> TypeChecker {
        TypeChecker {
            scopes: Scopes::new(),
            loop_depth: 0,
            functions: HashMap::new(),
            current_return: None,
        }
    }

    pub fn check(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        self.declare_functions(stmts)?;
        self.check_stmts(stmts)
    }

    fn check_stmts(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for stmt in stmts {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    fn declare_functions(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for stmt in stmts {
            let Stmt::FnDecl(function) = stmt else {
                continue;
            };
            let name = &function.name;
            if Builtin::from_name(name).is_some() {
                return Err(SomoniError::at(
                    function.name_span,
                    ErrorCode::BuiltinRedeclared { name: name.clone() },
                ));
            }
            if self.functions.contains_key(name) {
                return Err(SomoniError::at(
                    function.name_span,
                    ErrorCode::DuplicateFunction { name: name.clone() },
                ));
            }
            let mut params = Vec::new();
            for param in &function.params {
                params.push(type_from_name(&param.type_name, param.type_span)?);
            }
            let return_type = Type::from_return_name(&function.return_type)
                .ok_or_else(|| unknown_type(&function.return_type, function.return_type_span))?;
            self.functions.insert(
                name.clone(),
                Signature {
                    params,
                    return_type,
                },
            );
        }
        Ok(())
    }

    fn check_function(&mut self, function: &Function) -> Result<(), SomoniError> {
        let signature = &self.functions[&function.name];
        let param_types = signature.params.clone();
        let return_type = signature.return_type;

        let outer_scopes = std::mem::replace(&mut self.scopes, Scopes::new());
        let outer_return = self.current_return.replace(return_type);
        let result = self.check_function_body(function, &param_types);
        self.scopes = outer_scopes;
        self.current_return = outer_return;
        result?;

        if return_type != Type::Void && !always_returns(&function.body) {
            return Err(SomoniError::at(
                function.return_type_span,
                ErrorCode::MissingReturn {
                    name: function.name.clone(),
                    expected: return_type,
                },
            ));
        }
        Ok(())
    }

    fn check_function_body(
        &mut self,
        function: &Function,
        param_types: &[Type],
    ) -> Result<(), SomoniError> {
        for (param, ty) in function.params.iter().zip(param_types) {
            let info = VarInfo {
                ty: *ty,
                mutable: false,
            };
            if !self.scopes.declare(&param.name, info) {
                return Err(SomoniError::at(
                    param.name_span,
                    ErrorCode::AlreadyDeclared {
                        name: param.name.clone(),
                    },
                ));
            }
        }
        self.check_stmts(&function.body)
    }

    fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), SomoniError> {
        match stmt {
            Stmt::VarDecl {
                name,
                name_span,
                type_name,
                type_span,
                value,
                mutable,
                ..
            } => {
                let declared = type_from_name(type_name, *type_span)?;
                let actual = self.check_expr(value)?;
                if declared != actual {
                    return Err(SomoniError::at(
                        value.span,
                        ErrorCode::CannotAssign {
                            found: actual,
                            expected: declared,
                        },
                    ));
                }
                let info = VarInfo {
                    ty: declared,
                    mutable: *mutable,
                };
                if !self.scopes.declare(name, info) {
                    return Err(SomoniError::at(
                        *name_span,
                        ErrorCode::AlreadyDeclared { name: name.clone() },
                    ));
                }
                Ok(())
            }
            Stmt::Expr { expr, .. } => {
                self.check_expr(expr)?;
                Ok(())
            }
            Stmt::Assign {
                name,
                name_span,
                value,
                ..
            } => {
                let info = self.scopes.lookup(name).ok_or_else(|| {
                    SomoniError::at(
                        *name_span,
                        ErrorCode::UnknownVariable { name: name.clone() },
                    )
                })?;
                if !info.mutable {
                    return Err(SomoniError::at(
                        *name_span,
                        ErrorCode::AssignToConst { name: name.clone() },
                    ));
                }
                let expected = info.ty;
                let actual = self.check_expr(value)?;
                if expected != actual {
                    return Err(SomoniError::at(
                        value.span,
                        ErrorCode::CannotAssign {
                            found: actual,
                            expected,
                        },
                    ));
                }
                Ok(())
            }
            Stmt::Block(stmts) => {
                self.scopes.push();
                let result = self.check_stmts(stmts);
                self.scopes.pop();
                result
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.check_condition(condition)?;
                // both branches are checked, even the one that will not run
                self.check_stmt(then_branch)?;
                if let Some(else_branch) = else_branch {
                    self.check_stmt(else_branch)?;
                }
                Ok(())
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.check_condition(condition)?;
                // checked once, even if it runs many times (or never)
                self.check_loop_body(body)
            }
            Stmt::For {
                init,
                condition,
                step,
                body,
                ..
            } => {
                // the counter lives in its own scope around the whole loop, gone afterwards
                self.scopes.push();
                let result = self.check_for(init, condition, step, body);
                self.scopes.pop();
                result
            }
            Stmt::FnDecl(function) => self.check_function(function),
            Stmt::Return { value, span } => {
                let Some(expected) = self.current_return else {
                    return Err(SomoniError::at(*span, ErrorCode::ReturnOutsideFunction));
                };
                let (found, place) = match value {
                    Some(value) => (self.check_expr(value)?, value.span),
                    None => (Type::Void, *span),
                };
                if found != expected {
                    return Err(SomoniError::at(
                        place,
                        ErrorCode::ReturnType { found, expected },
                    ));
                }
                Ok(())
            }
            Stmt::Break { span } => self.check_inside_loop("break", *span),
            Stmt::Continue { span } => self.check_inside_loop("continue", *span),
        }
    }

    fn check_for(
        &mut self,
        init: &Stmt,
        condition: &Expr,
        step: &Stmt,
        body: &Stmt,
    ) -> Result<(), SomoniError> {
        self.check_stmt(init)?;
        self.check_condition(condition)?;
        self.check_stmt(step)?;
        self.check_loop_body(body)
    }

    fn check_loop_body(&mut self, body: &Stmt) -> Result<(), SomoniError> {
        self.loop_depth += 1;
        let result = self.check_stmt(body);
        self.loop_depth -= 1;
        result
    }

    fn check_inside_loop(&self, keyword: &'static str, span: Span) -> Result<(), SomoniError> {
        if self.loop_depth == 0 {
            return Err(SomoniError::at(span, ErrorCode::OutsideLoop { keyword }));
        }
        Ok(())
    }

    // conditions of if and while must be bool, `if (1)` is an error
    fn check_condition(&self, condition: &Expr) -> Result<(), SomoniError> {
        let ty = self.check_expr(condition)?;
        if ty != Type::Bool {
            return Err(SomoniError::at(
                condition.span,
                ErrorCode::ConditionNotBool { found: ty },
            ));
        }
        Ok(())
    }

    fn check_expr(&self, expr: &Expr) -> Result<Type, SomoniError> {
        match &expr.kind {
            ExprKind::Int(_) => Ok(Type::Int),
            ExprKind::Float(_) => Ok(Type::Float),
            ExprKind::Bool(_) => Ok(Type::Bool),
            ExprKind::Str(_) => Ok(Type::Str),
            ExprKind::Ident(name) => {
                self.scopes.lookup(name).map(|info| info.ty).ok_or_else(|| {
                    SomoniError::at(expr.span, ErrorCode::UnknownVariable { name: name.clone() })
                })
            }
            ExprKind::Binary {
                left,
                op,
                op_span,
                right,
            } => {
                let l = self.check_expr(left)?;
                let r = self.check_expr(right)?;
                if l != r {
                    return Err(SomoniError::at(
                        *op_span,
                        ErrorCode::OperandTypesDiffer { left: l, right: r },
                    ));
                }
                match (l, op) {
                    (Type::Void, _) => Err(SomoniError::at(
                        *op_span,
                        ErrorCode::OperatorNotAllowed { found: l },
                    )),
                    (_, BinaryOp::Eq | BinaryOp::NotEq) => Ok(Type::Bool),
                    (Type::Int, BinaryOp::Mod) => Ok(Type::Int),
                    (
                        Type::Int | Type::Float,
                        BinaryOp::Less | BinaryOp::Greater | BinaryOp::LessEq | BinaryOp::GreaterEq,
                    ) => Ok(Type::Bool),
                    (
                        Type::Int | Type::Float,
                        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div,
                    ) => Ok(l),
                    (Type::Str, BinaryOp::Add) => Ok(Type::Str),
                    (Type::Bool, BinaryOp::And | BinaryOp::Or) => Ok(Type::Bool),
                    _ => Err(SomoniError::at(
                        *op_span,
                        ErrorCode::OperatorNotAllowed { found: l },
                    )),
                }
            }
            ExprKind::Call {
                name,
                name_span,
                args,
            } => {
                if let Some(builtin) = Builtin::from_name(name) {
                    check_arg_count(name, builtin.arity(), args, expr.span)?;
                    let arg = &args[0];
                    let arg_type = self.check_expr(arg)?;
                    return builtin
                        .result_type(arg_type)
                        .ok_or_else(|| wrong_arg_type(name, arg_type, arg.span));
                }
                let Some(signature) = self.functions.get(name) else {
                    return Err(SomoniError::at(
                        *name_span,
                        ErrorCode::UnknownFunction { name: name.clone() },
                    ));
                };
                check_arg_count(name, signature.params.len(), args, expr.span)?;
                for (arg, param_type) in args.iter().zip(&signature.params) {
                    let arg_type = self.check_expr(arg)?;
                    if arg_type != *param_type {
                        return Err(wrong_arg_type(name, arg_type, arg.span));
                    }
                }
                Ok(signature.return_type)
            }
            ExprKind::Unary { op, operand } => {
                let t = self.check_expr(operand)?;
                match (op, t) {
                    (UnaryOp::Not, Type::Bool) => Ok(Type::Bool),
                    (UnaryOp::Not, _) => Err(SomoniError::at(
                        expr.span,
                        ErrorCode::NotNeedsBool { found: t },
                    )),
                    (UnaryOp::Neg, Type::Int | Type::Float) => Ok(t),
                    (UnaryOp::Neg, _) => Err(SomoniError::at(
                        expr.span,
                        ErrorCode::NegNeedsNumber { found: t },
                    )),
                }
            }
        }
    }
}

fn type_from_name(name: &str, span: Span) -> Result<Type, SomoniError> {
    Type::from_name(name).ok_or_else(|| unknown_type(name, span))
}

fn unknown_type(name: &str, span: Span) -> SomoniError {
    SomoniError::at(
        span,
        ErrorCode::UnknownType {
            name: name.to_string(),
        },
    )
}

fn check_arg_count(
    name: &str,
    expected: usize,
    args: &[Expr],
    call_span: Span,
) -> Result<(), SomoniError> {
    if args.len() != expected {
        return Err(SomoniError::at(
            call_span,
            ErrorCode::WrongArgCount {
                name: name.to_string(),
                expected,
                found: args.len(),
            },
        ));
    }
    Ok(())
}

fn wrong_arg_type(name: &str, found: Type, span: Span) -> SomoniError {
    SomoniError::at(
        span,
        ErrorCode::WrongArgType {
            name: name.to_string(),
            found,
        },
    )
}

// Does every path through these statements end with `return`?
// One statement that always returns is enough (the rest after it never runs).
fn always_returns(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_always_returns)
}

fn stmt_always_returns(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return { .. } => true,
        Stmt::Block(stmts) => always_returns(stmts),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => match else_branch {
            Some(else_branch) => {
                stmt_always_returns(then_branch) && stmt_always_returns(else_branch)
            }
            None => false,
        },
        Stmt::While { .. } | Stmt::For { .. } => false,
        Stmt::VarDecl { .. }
        | Stmt::Assign { .. }
        | Stmt::Expr { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::FnDecl(_) => false,
    }
}
