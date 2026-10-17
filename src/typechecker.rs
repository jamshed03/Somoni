use std::collections::{HashMap, HashSet};

use crate::ast::{
    self, BinaryOp, CallTarget, Expr, ExprKind, FieldInit, Function, IterKind, Param, Stmt,
    TypeName, UnaryOp,
};
use crate::builtins::Builtin;
use crate::error::{ErrorCode, SomoniError};
use crate::scope::Scopes;
use crate::span::Span;
use crate::types::Type;

/// What the type checker knows about a variable.
struct VarInfo {
    ty: Type,
    mutable: bool,
    slot: usize,
}

/// The number, parameter types and return type of a function.
#[derive(Clone)]
struct Signature {
    index: usize,
    params: Vec<Type>,
    return_type: Type,
}

/// The number of a struct and the name and type of each field, in declaration order.
#[derive(Clone)]
struct StructInfo {
    index: usize,
    fields: Vec<(String, Type)>,
}

/// Checks the types of a program before it runs.
/// It also writes into the tree what the compiler needs: slots, call targets, field positions.
pub struct TypeChecker {
    scopes: Scopes<VarInfo>,
    loop_depth: usize,
    functions: HashMap<String, Signature>,
    structs: HashMap<String, StructInfo>,
    current_return: Option<Type>,
    next_slot: usize,
}

impl TypeChecker {
    pub fn new() -> TypeChecker {
        TypeChecker {
            scopes: Scopes::new(),
            loop_depth: 0,
            functions: HashMap::new(),
            structs: HashMap::new(),
            current_return: None,
            next_slot: 0,
        }
    }

    /// Checks the whole program.
    pub fn check(&mut self, stmts: &mut [Stmt]) -> Result<(), SomoniError> {
        self.declare_structs(stmts)?;
        self.declare_functions(stmts)?;
        self.check_stmts(stmts)
    }

    fn check_stmts(&mut self, stmts: &mut [Stmt]) -> Result<(), SomoniError> {
        for stmt in stmts {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    /// Collects all structs: first their names, then their field types,
    /// then checks that no struct contains itself.
    fn declare_structs(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for (index, decl) in ast::structs(stmts).enumerate() {
            if Type::is_builtin_name(&decl.name) || self.structs.contains_key(&decl.name) {
                return Err(SomoniError::at(
                    decl.name_span,
                    ErrorCode::DuplicateType {
                        name: decl.name.clone(),
                    },
                ));
            }
            let info = StructInfo {
                index,
                fields: Vec::new(),
            };
            self.structs.insert(decl.name.clone(), info);
        }

        for decl in ast::structs(stmts) {
            let mut fields: Vec<(String, Type)> = Vec::new();
            for field in &decl.fields {
                if fields.iter().any(|(name, _)| *name == field.name) {
                    return Err(SomoniError::at(
                        field.name_span,
                        ErrorCode::DuplicateField {
                            name: field.name.clone(),
                        },
                    ));
                }
                fields.push((
                    field.name.clone(),
                    self.resolve_type(&field.type_name, false)?,
                ));
            }
            if let Some(info) = self.structs.get_mut(&decl.name) {
                info.fields = fields;
            }
        }

        for decl in ast::structs(stmts) {
            let info = &self.structs[&decl.name];
            for (field, (_, ty)) in decl.fields.iter().zip(&info.fields) {
                if let Type::Struct(inner) = ty
                    && self.struct_contains(inner, &decl.name, &mut HashSet::new())
                {
                    return Err(SomoniError::at(
                        field.type_name.span,
                        ErrorCode::StructContainsItself {
                            name: decl.name.clone(),
                        },
                    ));
                }
            }
        }
        Ok(())
    }

    /// Returns true if struct `from` contains struct `target` in a field, also through other structs.
    /// Fields of type `list<...>` do not count.
    fn struct_contains(&self, from: &str, target: &str, seen: &mut HashSet<String>) -> bool {
        if from == target {
            return true;
        }
        if !seen.insert(from.to_string()) {
            return false;
        }
        let Some(info) = self.structs.get(from) else {
            return false;
        };
        info.fields.iter().any(|(_, ty)| match ty {
            Type::Struct(inner) => self.struct_contains(inner, target, seen),
            _ => false,
        })
    }

    /// Collects the signatures of all functions, so they can be called before they are declared.
    fn declare_functions(&mut self, stmts: &[Stmt]) -> Result<(), SomoniError> {
        for (index, function) in ast::functions(stmts).enumerate() {
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
                params.push(self.resolve_type(&param.type_name, false)?);
            }
            let return_type = self.resolve_type(&function.return_type, true)?;
            self.functions.insert(
                name.clone(),
                Signature {
                    index,
                    params,
                    return_type,
                },
            );
        }
        Ok(())
    }

    /// Checks a function body with its own variables, and that it returns on every path.
    fn check_function(&mut self, function: &mut Function) -> Result<(), SomoniError> {
        let signature = &self.functions[&function.name];
        let param_types = signature.params.clone();
        let return_type = signature.return_type.clone();

        let outer_scopes = std::mem::replace(&mut self.scopes, Scopes::new());
        let outer_return = self.current_return.replace(return_type.clone());
        let outer_next_slot = std::mem::replace(&mut self.next_slot, 0);
        let result = self.check_function_body(function, &param_types);
        self.scopes = outer_scopes;
        self.current_return = outer_return;
        self.next_slot = outer_next_slot;
        result?;

        if return_type != Type::Void && !always_returns(&function.body) {
            return Err(SomoniError::at(
                function.return_type.span,
                ErrorCode::MissingReturn {
                    name: function.name.clone(),
                    expected: return_type,
                },
            ));
        }
        Ok(())
    }

    /// Declares the parameters as const variables and checks the body.
    fn check_function_body(
        &mut self,
        function: &mut Function,
        param_types: &[Type],
    ) -> Result<(), SomoniError> {
        for (param, ty) in function.params.iter().zip(param_types) {
            self.declare(&param.name, param.name_span, ty.clone(), false)?;
        }
        self.check_stmts(&mut function.body)
    }

    fn check_stmt(&mut self, stmt: &mut Stmt) -> Result<(), SomoniError> {
        match stmt {
            Stmt::VarDecl {
                name,
                name_span,
                type_name,
                value,
                mutable,
                slot,
                ..
            } => {
                let declared = self.resolve_type(type_name, false)?;
                self.check_assignable(value, &declared)?;
                *slot = Some(self.declare(name, *name_span, declared, *mutable)?);
                Ok(())
            }
            Stmt::Assign { target, value, .. } => self.check_assign(target, value),
            Stmt::Expr { expr, .. } => {
                self.check_expr(expr)?;
                Ok(())
            }
            Stmt::Block(stmts) => self.in_scope(|this| this.check_stmts(stmts)),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.check_condition(condition)?;
                let then_known = self.known_not_null(condition, true);
                let else_known = self.known_not_null(condition, false);
                self.with_not_null(then_known, |this| this.check_stmt(then_branch))?;
                if let Some(else_branch) = else_branch {
                    self.with_not_null(else_known, |this| this.check_stmt(else_branch))?;
                }
                Ok(())
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.check_condition(condition)?;
                let known = self.known_not_null(condition, true);
                self.with_not_null(known, |this| this.check_loop_body(body))
            }
            Stmt::For {
                init,
                condition,
                step,
                body,
                ..
            } => self.in_scope(|this| this.check_for(init, condition, step, body)),
            Stmt::ForEach {
                vars,
                iterable,
                body,
                slots,
                seq_slot,
                over,
                ..
            } => {
                let ty = self.check_expr(iterable)?;
                let Some((kind, expected)) = loop_var_types(&ty, vars.len()) else {
                    return Err(SomoniError::at(
                        iterable.span,
                        ErrorCode::NotIterable { found: ty },
                    ));
                };
                *over = Some(kind);
                let span = iterable.span;
                self.in_scope(|this| {
                    *seq_slot = Some(this.declare("#seq", span, ty, false)?);
                    this.declare("#index", span, Type::Int, true)?;
                    *slots = this.declare_loop_vars(vars, expected)?;
                    this.check_loop_body(body)
                })
            }
            Stmt::FnDecl(function) => self.check_function(function),
            Stmt::StructDecl(_) => Ok(()),
            Stmt::Return { value, span } => self.check_return(value.as_mut(), *span),
            Stmt::Break { span } => self.check_inside_loop("break", *span),
            Stmt::Continue { span } => self.check_inside_loop("continue", *span),
        }
    }

    /// Runs `check` while the variables in `known` have the type without `?`, e.g. `int` for an
    /// `int?` after `a != null`. They keep their slot, so they are the same variables.
    fn with_not_null<T>(
        &mut self,
        known: Vec<(String, Type)>,
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.scopes.push();
        for (name, ty) in known {
            if let Some(info) = self.scopes.lookup(&name) {
                let narrowed = VarInfo {
                    ty,
                    mutable: info.mutable,
                    slot: info.slot,
                };
                self.scopes.declare(&name, narrowed);
            }
        }
        let result = check(self);
        self.scopes.pop();
        result
    }

    /// Returns the variables that cannot be null when `condition` is `when`, each with its type
    /// without `?`. Example: `a != null && b != null` gives `a` and `b` when it is true.
    fn known_not_null(&self, condition: &Expr, when: bool) -> Vec<(String, Type)> {
        match &condition.kind {
            ExprKind::Binary {
                left, op, right, ..
            } => match (op, when) {
                (BinaryOp::NotEq, true) | (BinaryOp::Eq, false) => {
                    self.null_checked(left, right).into_iter().collect()
                }
                (BinaryOp::And, true) | (BinaryOp::Or, false) => {
                    let mut known = self.known_not_null(left, when);
                    known.extend(self.known_not_null(right, when));
                    known
                }
                _ => Vec::new(),
            },
            ExprKind::Unary {
                op: UnaryOp::Not,
                operand,
            } => self.known_not_null(operand, !when),
            _ => Vec::new(),
        }
    }

    /// For `a != null` or `null != a` with `a: T?`: returns `a` and `T`.
    fn null_checked(&self, left: &Expr, right: &Expr) -> Option<(String, Type)> {
        let name = match (&left.kind, &right.kind) {
            (ExprKind::Ident { name, .. }, ExprKind::Null)
            | (ExprKind::Null, ExprKind::Ident { name, .. }) => name,
            _ => return None,
        };
        let Type::Optional(inner) = &self.scopes.lookup(name)?.ty else {
            return None;
        };
        Some((name.clone(), (**inner).clone()))
    }

    /// Runs `check` inside a new block. Afterwards the variables of the block are gone.
    fn in_scope<T>(&mut self, check: impl FnOnce(&mut Self) -> T) -> T {
        self.scopes.push();
        let first_slot = self.next_slot;
        let result = check(self);
        self.next_slot = first_slot;
        self.scopes.pop();
        result
    }

    /// Declares a new variable and returns its slot. A name may exist only once per block.
    fn declare(
        &mut self,
        name: &str,
        name_span: Span,
        ty: Type,
        mutable: bool,
    ) -> Result<usize, SomoniError> {
        let slot = self.next_slot;
        let info = VarInfo { ty, mutable, slot };
        if !self.scopes.declare(name, info) {
            return Err(SomoniError::at(
                name_span,
                ErrorCode::AlreadyDeclared {
                    name: name.to_string(),
                },
            ));
        }
        self.next_slot += 1;
        Ok(slot)
    }

    /// Checks that `value` has the type `expected`.
    fn check_assignable(&mut self, value: &mut Expr, expected: &Type) -> Result<(), SomoniError> {
        let found = self.check_expr_expecting(value, expected)?;
        if !fits(&found, expected) {
            return Err(SomoniError::at(
                value.span,
                ErrorCode::CannotAssign {
                    found,
                    expected: expected.clone(),
                },
            ));
        }
        Ok(())
    }

    /// Checks `target = value;`: the target must exist, must not be const, and the types must match.
    fn check_assign(&mut self, target: &mut Expr, value: &mut Expr) -> Result<(), SomoniError> {
        let target_type = self.check_expr(target)?;
        let Some((name, name_span)) = target.root_variable() else {
            return Err(SomoniError::internal("assignment to a non-variable"));
        };
        if !self.scopes.lookup(name).is_some_and(|info| info.mutable) {
            return Err(SomoniError::at(
                name_span,
                ErrorCode::AssignToConst {
                    name: name.to_string(),
                },
            ));
        }
        self.check_assignable(value, &target_type)
    }

    /// Checks that `return` is inside a function and returns the right type.
    fn check_return(&mut self, value: Option<&mut Expr>, span: Span) -> Result<(), SomoniError> {
        let Some(expected) = &self.current_return.clone() else {
            return Err(SomoniError::at(span, ErrorCode::ReturnOutsideFunction));
        };
        let (found, place) = match value {
            Some(value) => (self.check_expr_expecting(value, expected)?, value.span),
            None => (Type::Void, span),
        };
        if !fits(&found, expected) {
            return Err(SomoniError::at(
                place,
                ErrorCode::ReturnType {
                    found,
                    expected: expected.clone(),
                },
            ));
        }
        Ok(())
    }

    fn check_for(
        &mut self,
        init: &mut Stmt,
        condition: &mut Expr,
        step: &mut Stmt,
        body: &mut Stmt,
    ) -> Result<(), SomoniError> {
        self.check_stmt(init)?;
        self.check_condition(condition)?;
        self.check_stmt(step)?;
        let known = self.known_not_null(condition, true);
        self.with_not_null(known, |this| this.check_loop_body(body))
    }

    /// Declares the variables of a `for`-each loop as const and returns their slots.
    /// Each must have exactly the type in `expected`.
    fn declare_loop_vars(
        &mut self,
        vars: &[Param],
        expected: Vec<Type>,
    ) -> Result<Vec<usize>, SomoniError> {
        let mut slots = Vec::new();
        for (var, expected) in vars.iter().zip(expected) {
            let declared = self.resolve_type(&var.type_name, false)?;
            if declared != expected {
                return Err(SomoniError::at(
                    var.type_name.span,
                    ErrorCode::LoopVarType {
                        name: var.name.clone(),
                        expected,
                        found: declared,
                    },
                ));
            }
            slots.push(self.declare(&var.name, var.name_span, declared, false)?);
        }
        Ok(slots)
    }

    /// Checks a loop body; inside it, `break` and `continue` are allowed.
    fn check_loop_body(&mut self, body: &mut Stmt) -> Result<(), SomoniError> {
        self.loop_depth += 1;
        let result = self.check_stmt(body);
        self.loop_depth -= 1;
        result
    }

    /// Checks that `break` or `continue` is inside a loop.
    fn check_inside_loop(&self, keyword: &'static str, span: Span) -> Result<(), SomoniError> {
        if self.loop_depth == 0 {
            return Err(SomoniError::at(span, ErrorCode::OutsideLoop { keyword }));
        }
        Ok(())
    }

    /// Checks that a condition of `if`, `while` or `for` is a bool.
    fn check_condition(&mut self, condition: &mut Expr) -> Result<(), SomoniError> {
        let ty = self.check_expr(condition)?;
        if ty != Type::Bool {
            return Err(SomoniError::at(
                condition.span,
                ErrorCode::ConditionNotBool { found: ty },
            ));
        }
        Ok(())
    }

    /// Like `check_expr`, but knows which type is expected.
    /// This lets `[]` and `null` get their type: `var l: list<int> = [];`, `var a: int? = null;`.
    fn check_expr_expecting(
        &mut self,
        expr: &mut Expr,
        expected: &Type,
    ) -> Result<Type, SomoniError> {
        let wanted = match expected {
            Type::Optional(inner) => inner,
            other => other,
        };
        match (&mut expr.kind, wanted) {
            (ExprKind::List(elements), Type::List(element_type)) => {
                self.check_list(elements, expr.span, Some(element_type))
            }
            (ExprKind::Map(entries), Type::Map(key, value)) => {
                self.check_map(entries, expr.span, Some((key, value)))
            }
            (ExprKind::Null, _) if matches!(expected, Type::Optional(_)) => Ok(expected.clone()),
            (ExprKind::Null, _) => Err(SomoniError::at(
                expr.span,
                ErrorCode::NullNotAllowed {
                    expected: expected.clone(),
                },
            )),
            _ => self.check_expr(expr),
        }
    }

    /// Checks both sides of an operator. `[]` and `null` get the type of the other side:
    /// `l + []`, `a == null`. Right of `&&` and `||`, the null checks on the left count.
    fn check_operands(
        &mut self,
        op: BinaryOp,
        left: &mut Expr,
        right: &mut Expr,
    ) -> Result<(Type, Type), SomoniError> {
        if needs_type_hint(left) {
            let r = self.check_expr(right)?;
            let l = self.check_expr_expecting(left, &r)?;
            return Ok((l, r));
        }
        let l = self.check_expr(left)?;
        let known = match op {
            BinaryOp::And => self.known_not_null(left, true),
            BinaryOp::Or => self.known_not_null(left, false),
            _ => Vec::new(),
        };
        let r = self.with_not_null(known, |this| this.check_expr_expecting(right, &l))?;
        Ok((l, r))
    }

    /// Checks a list literal: all elements must have the same type.
    /// `hint` is the expected element type; without it, the first element gives the type.
    fn check_list(
        &mut self,
        elements: &mut [Expr],
        span: Span,
        hint: Option<&Type>,
    ) -> Result<Type, SomoniError> {
        let (element_type, rest) = match hint {
            Some(hint) => (hint.clone(), elements),
            None => {
                let Some((first, rest)) = elements.split_first_mut() else {
                    return Err(SomoniError::at(span, ErrorCode::EmptyListType));
                };
                (self.check_expr(first)?, rest)
            }
        };
        for element in rest {
            let found = self.check_expr_expecting(element, &element_type)?;
            if !fits(&found, &element_type) {
                return Err(SomoniError::at(
                    element.span,
                    ErrorCode::ListElementType {
                        expected: element_type,
                        found,
                    },
                ));
            }
        }
        Ok(Type::List(Box::new(element_type)))
    }

    /// Checks a map literal: all keys must have one type, and all values too.
    /// `hint` is the expected key and value type; without it, the first entry gives them.
    fn check_map(
        &mut self,
        entries: &mut [(Expr, Expr)],
        span: Span,
        hint: Option<(&Type, &Type)>,
    ) -> Result<Type, SomoniError> {
        let (key_type, value_type, rest) = match hint {
            Some((key, value)) => (key.clone(), value.clone(), entries),
            None => {
                let Some(((key, value), rest)) = entries.split_first_mut() else {
                    return Err(SomoniError::at(span, ErrorCode::EmptyMapType));
                };
                let key_type = self.check_expr(key)?;
                check_key_type(&key_type, key.span)?;
                (key_type, self.check_expr(value)?, rest)
            }
        };
        for (key, value) in rest {
            self.check_map_part("keys", key, &key_type)?;
            self.check_map_part("values", value, &value_type)?;
        }
        Ok(Type::Map(Box::new(key_type), Box::new(value_type)))
    }

    /// Checks that a key or a value in a map literal has the type of the others.
    fn check_map_part(
        &mut self,
        part: &'static str,
        expr: &mut Expr,
        expected: &Type,
    ) -> Result<(), SomoniError> {
        let found = self.check_expr_expecting(expr, expected)?;
        if !fits(&found, expected) {
            return Err(SomoniError::at(
                expr.span,
                ErrorCode::MapEntryType {
                    part,
                    expected: expected.clone(),
                    found,
                },
            ));
        }
        Ok(())
    }

    /// Checks a struct literal: every field must be given exactly once, in any order.
    fn check_struct_literal(
        &mut self,
        name: &str,
        name_span: Span,
        fields: &mut [FieldInit],
        index: &mut Option<usize>,
    ) -> Result<Type, SomoniError> {
        let info = &self.struct_info(name, name_span)?.clone();
        *index = Some(info.index);
        let mut given = vec![false; info.fields.len()];
        for init in fields {
            let (position, ty) = field_of(info, name, &init.name, init.name_span)?;
            if given[position] {
                return Err(SomoniError::at(
                    init.name_span,
                    ErrorCode::DuplicateField {
                        name: init.name.clone(),
                    },
                ));
            }
            given[position] = true;
            init.index = Some(position);
            self.check_assignable(&mut init.value, ty)?;
        }
        let missing = info.fields.iter().zip(&given).find(|(_, given)| !**given);
        if let Some(((field, _), _)) = missing {
            return Err(SomoniError::at(
                name_span,
                ErrorCode::MissingField {
                    struct_name: name.to_string(),
                    name: field.clone(),
                },
            ));
        }
        Ok(Type::Struct(name.to_string()))
    }

    /// Returns the struct with this name, or T001 if there is none.
    fn struct_info(&self, name: &str, span: Span) -> Result<&StructInfo, SomoniError> {
        self.structs.get(name).ok_or_else(|| {
            SomoniError::at(
                span,
                ErrorCode::UnknownType {
                    name: name.to_string(),
                },
            )
        })
    }

    /// Turns a written type like `list<int>?` into a `Type`. `void` is only allowed if `allow_void`.
    fn resolve_type(&self, written: &TypeName, allow_void: bool) -> Result<Type, SomoniError> {
        let ty = self.resolve_without_optional(written, allow_void)?;
        if !written.optional {
            return Ok(ty);
        }
        if ty == Type::Void {
            return Err(SomoniError::at(written.span, ErrorCode::OptionalVoid));
        }
        Ok(Type::Optional(Box::new(ty)))
    }

    /// Like `resolve_type`, but without the `?`: `list<int>?` gives `list<int>`.
    fn resolve_without_optional(
        &self,
        written: &TypeName,
        allow_void: bool,
    ) -> Result<Type, SomoniError> {
        let simple = if allow_void {
            Type::from_return_name(&written.name)
        } else {
            Type::from_name(&written.name)
        };
        let simple = simple.or_else(|| {
            self.structs
                .contains_key(&written.name)
                .then(|| Type::Struct(written.name.clone()))
        });
        let expected_args = match (Type::arg_count(&written.name), &simple) {
            (Some(count), _) => count,
            (None, Some(_)) => 0,
            (None, None) => {
                return Err(SomoniError::at(
                    written.span,
                    ErrorCode::UnknownType {
                        name: written.name.clone(),
                    },
                ));
            }
        };
        if written.args.len() != expected_args {
            return Err(SomoniError::at(
                written.span,
                ErrorCode::TypeArguments {
                    name: written.name.clone(),
                    expected: expected_args,
                    found: written.args.len(),
                },
            ));
        }
        if let Some(ty) = simple {
            return Ok(ty);
        }
        let mut args = Vec::new();
        for arg in &written.args {
            args.push(self.resolve_type(arg, false)?);
        }
        let ty = Type::with_args(&written.name, args)
            .ok_or_else(|| SomoniError::internal("type arguments do not fit"))?;
        if let Type::Map(key, _) = &ty {
            check_key_type(key, written.args[0].span)?;
        }
        Ok(ty)
    }

    /// Returns the type of an expression, or a type error.
    fn check_expr(&mut self, expr: &mut Expr) -> Result<Type, SomoniError> {
        match &mut expr.kind {
            ExprKind::Int(_) => Ok(Type::Int),
            ExprKind::Float(_) => Ok(Type::Float),
            ExprKind::Bool(_) => Ok(Type::Bool),
            ExprKind::Str(_) => Ok(Type::Str),
            ExprKind::Null => Err(SomoniError::at(expr.span, ErrorCode::NullTypeUnknown)),
            ExprKind::Ident { name, slot } => {
                let info = self.scopes.lookup(name).ok_or_else(|| {
                    SomoniError::at(expr.span, ErrorCode::UnknownVariable { name: name.clone() })
                })?;
                *slot = Some(info.slot);
                Ok(info.ty.clone())
            }
            ExprKind::Binary {
                left,
                op,
                op_span,
                right,
            } => {
                let (l, r) = self.check_operands(*op, left, right)?;
                binary_type(*op, l, r, *op_span)
            }
            ExprKind::List(elements) => self.check_list(elements, expr.span, None),
            ExprKind::Map(entries) => self.check_map(entries, expr.span, None),
            ExprKind::Index { target, index } => {
                let target_type = self.check_expr(target)?;
                match target_type {
                    Type::List(element_type) => {
                        let index_type = self.check_expr(index)?;
                        if index_type != Type::Int {
                            return Err(SomoniError::at(
                                index.span,
                                ErrorCode::IndexNotInt { found: index_type },
                            ));
                        }
                        Ok(*element_type)
                    }
                    Type::Map(key_type, value_type) => {
                        let found = self.check_expr(index)?;
                        if found != *key_type {
                            return Err(SomoniError::at(
                                index.span,
                                ErrorCode::MapIndexType {
                                    expected: *key_type,
                                    found,
                                },
                            ));
                        }
                        Ok(*value_type)
                    }
                    other => Err(SomoniError::at(
                        target.span,
                        ErrorCode::NotIndexable { found: other },
                    )),
                }
            }
            ExprKind::StructLiteral {
                name,
                name_span,
                fields,
                index,
            } => self.check_struct_literal(name, *name_span, fields, index),
            ExprKind::Field {
                target,
                name,
                name_span,
                index,
            } => {
                let target_type = self.check_expr(target)?;
                let Type::Struct(struct_name) = &target_type else {
                    return Err(SomoniError::at(
                        target.span,
                        ErrorCode::NoFields { found: target_type },
                    ));
                };
                let info = self.struct_info(struct_name, target.span)?;
                let (position, ty) = field_of(info, struct_name, name, *name_span)?;
                *index = Some(position);
                Ok(ty.clone())
            }
            ExprKind::Call {
                name,
                name_span,
                args,
                target,
            } => {
                if let Some(builtin) = Builtin::from_name(name) {
                    *target = Some(CallTarget::Builtin(builtin));
                    check_arg_count(name, builtin.arity(), args, expr.span)?;
                    let mut arg_types = Vec::new();
                    for arg in args.iter_mut() {
                        arg_types.push(self.check_expr(arg)?);
                    }
                    return builtin.result_type(&arg_types).map_err(|bad| {
                        wrong_arg_type(name, arg_types[bad].clone(), args[bad].span)
                    });
                }
                let Some(signature) = &self.functions.get(name).cloned() else {
                    return Err(SomoniError::at(
                        *name_span,
                        ErrorCode::UnknownFunction { name: name.clone() },
                    ));
                };
                *target = Some(CallTarget::Function(signature.index));
                check_arg_count(name, signature.params.len(), args, expr.span)?;
                for (arg, param_type) in args.iter_mut().zip(&signature.params) {
                    let arg_type = self.check_expr_expecting(arg, param_type)?;
                    if !fits(&arg_type, param_type) {
                        return Err(wrong_arg_type(name, arg_type, arg.span));
                    }
                }
                Ok(signature.return_type.clone())
            }
            ExprKind::Unary { op, operand } => {
                let t = self.check_expr(operand)?;
                match (op, &t) {
                    (UnaryOp::Not, Type::Bool) => Ok(Type::Bool),
                    (UnaryOp::Not, _) => Err(SomoniError::at(
                        expr.span,
                        ErrorCode::NotNeedsBool { found: t.clone() },
                    )),
                    (UnaryOp::Neg, Type::Int | Type::Float) => Ok(t.clone()),
                    (UnaryOp::Neg, _) => Err(SomoniError::at(
                        expr.span,
                        ErrorCode::NegNeedsNumber { found: t.clone() },
                    )),
                }
            }
        }
    }
}

/// Returns what a `for`-each loop over `ty` runs over and the types of its `count` variables:
/// a list gives the element (or index and element), a map the key (or key and value),
/// a string a character (or index and character). None if `ty` cannot be looped over.
fn loop_var_types(ty: &Type, count: usize) -> Option<(IterKind, Vec<Type>)> {
    let (kind, first, second) = match ty {
        Type::List(element) => (IterKind::List, Type::Int, (**element).clone()),
        Type::Map(key, value) => (IterKind::Map, (**key).clone(), (**value).clone()),
        Type::Str => (IterKind::Str, Type::Int, Type::Str),
        _ => return None,
    };
    let types = match (kind, count) {
        (_, 2) => vec![first, second],
        (IterKind::Map, _) => vec![first],
        (IterKind::List | IterKind::Str, _) => vec![second],
    };
    Some((kind, types))
}

/// Returns the type of `l op r`, or an error if the operator does not fit the types.
/// `op_span` is the place of the operator, for the error.
fn binary_type(op: BinaryOp, l: Type, r: Type, op_span: Span) -> Result<Type, SomoniError> {
    let is_equality = matches!(op, BinaryOp::Eq | BinaryOp::NotEq);
    if l != r && !(is_equality && (fits(&l, &r) || fits(&r, &l))) {
        return Err(SomoniError::at(
            op_span,
            ErrorCode::OperandTypesDiffer { left: l, right: r },
        ));
    }
    match (&l, op) {
        (Type::Void, _) => Err(SomoniError::at(
            op_span,
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
        (Type::List(_), BinaryOp::Add) => Ok(l),
        (Type::Bool, BinaryOp::And | BinaryOp::Or) => Ok(Type::Bool),
        _ => Err(SomoniError::at(
            op_span,
            ErrorCode::OperatorNotAllowed { found: l },
        )),
    }
}

/// Returns true for `[]`, `{}` and `null`: they need the type of the place where they are used.
fn needs_type_hint(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::List(elements) => elements.is_empty(),
        ExprKind::Map(entries) => entries.is_empty(),
        ExprKind::Null => true,
        _ => false,
    }
}

/// Checks that a type may be the key of a map: only `string` and `int`.
fn check_key_type(ty: &Type, span: Span) -> Result<(), SomoniError> {
    match ty {
        Type::Str | Type::Int => Ok(()),
        other => Err(SomoniError::at(
            span,
            ErrorCode::MapKeyType {
                found: other.clone(),
            },
        )),
    }
}

/// Returns true if a value of type `found` may be used where `expected` is wanted:
/// the same type, or `T` where `T?` is wanted.
fn fits(found: &Type, expected: &Type) -> bool {
    found == expected || matches!(expected, Type::Optional(inner) if **inner == *found)
}

/// Returns the position and type of a field, or T027 if the struct has no such field.
fn field_of<'a>(
    info: &'a StructInfo,
    struct_name: &str,
    name: &str,
    span: Span,
) -> Result<(usize, &'a Type), SomoniError> {
    info.fields
        .iter()
        .enumerate()
        .find(|(_, (field, _))| field == name)
        .map(|(position, (_, ty))| (position, ty))
        .ok_or_else(|| {
            SomoniError::at(
                span,
                ErrorCode::UnknownField {
                    struct_name: struct_name.to_string(),
                    name: name.to_string(),
                },
            )
        })
}

/// Checks that a call has the right number of arguments.
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

/// The error for an argument with the wrong type.
fn wrong_arg_type(name: &str, found: Type, span: Span) -> SomoniError {
    SomoniError::at(
        span,
        ErrorCode::WrongArgType {
            name: name.to_string(),
            found,
        },
    )
}

/// Returns true if every path through these statements ends with `return`.
fn always_returns(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_always_returns)
}

/// Returns true if this statement always ends with `return` (an `if` only with `else`).
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
        Stmt::While { .. } | Stmt::For { .. } | Stmt::ForEach { .. } => false,
        Stmt::VarDecl { .. }
        | Stmt::Assign { .. }
        | Stmt::Expr { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::FnDecl(_)
        | Stmt::StructDecl(_) => false,
    }
}
