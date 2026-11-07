//! Checking expressions and the types of operators.

use std::rc::Rc;

use crate::ast::{
    BinaryOp, CLOSURE, CONSTRUCTOR, Call, CallTarget, CallValue, ClassLayout, Expr, ExprKind,
    FieldInit, InstanceOf, InstanceTarget, InterfaceLayout, Lambda, MethodCall, MethodTarget, New,
    Param, StaticAccess, StaticCall, SuperCall, THIS, UnaryOp,
};
use crate::builtins::Builtin;
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::types::{Decl, FunctionHint, Type};

use super::declarations::check_key_type;
use crate::scope::Scopes;

use super::{
    Constant, LambdaScope, Returns, Signature, TypeChecker, check_arg_count, field_of,
    wrong_arg_type,
};

impl TypeChecker {
    /// Checks an expression whose type is expected, so `[]`, `null` and `ok(...)` get it.
    pub(super) fn check_expr_expecting(
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
            (ExprKind::Tuple(items), Type::Tuple(types)) if items.len() == types.len() => {
                for (item, ty) in items.iter_mut().zip(types.iter()) {
                    self.check_assignable(item, ty)?;
                }
                Ok(wanted.clone())
            }
            (ExprKind::Lambda(lambda), Type::Function(function)) => {
                let hint = FunctionHint {
                    params: function.params.clone(),
                    result: Some(function.result.clone()),
                };
                self.check_lambda(lambda, Some(hint))
            }
            (ExprKind::Null, _) if matches!(expected, Type::Optional(_)) => Ok(expected.clone()),
            (ExprKind::Null, _) => Err(SomoniError::at(
                expr.span,
                ErrorCode::NullNotAllowed {
                    expected: expected.clone(),
                },
            )),
            (ExprKind::Call(call), Type::Result(value, error))
                if Builtin::result_builder(&call.name).is_some() =>
            {
                self.check_result_builder(call, value, error, expr.span)?;
                Ok(wanted.clone())
            }
            _ => self.check_expr(expr),
        }
    }

    /// Checks `ok(x)` or `err(x)` against the value and error types of the expected result.
    fn check_result_builder(
        &mut self,
        call: &mut Call,
        value: &Type,
        error: &Type,
        span: Span,
    ) -> Result<(), SomoniError> {
        let builtin = Builtin::result_builder(&call.name)
            .ok_or_else(|| SomoniError::internal("ok or err is no built-in"))?;
        call.target = Some(CallTarget::Builtin(builtin));
        check_arg_count(&call.name, builtin.arity(), &call.args, span)?;
        let inner = if builtin == Builtin::Ok { value } else { error };
        let name = &call.name;
        self.check_fits(&mut call.args[0], inner, |found| ErrorCode::WrongArgType {
            name: name.clone(),
            found,
        })?;
        Ok(())
    }

    /// Checks both sides of an operator.
    fn check_operands(
        &mut self,
        op: BinaryOp,
        left: &mut Expr,
        right: &mut Expr,
    ) -> Result<(Type, Type), SomoniError> {
        if needs_type_hint(left) {
            let r = self.check_expr(right)?;
            let r = self.null_compared(op, right, left, r);
            let l = self.check_expr_expecting(left, &r)?;
            return Ok((l, r));
        }
        let l = self.check_expr(left)?;
        let l = self.null_compared(op, left, right, l);
        let known = match op {
            BinaryOp::And => self.known_narrowed(left, true),
            BinaryOp::Or => self.known_narrowed(left, false),
            _ => Vec::new(),
        };
        let r = self.with_narrowed(known, |this| this.check_expr_expecting(right, &l))?;
        Ok((l, r))
    }

    /// Returns the declared type of a variable compared with `null`, so `a != null` works even where `a` is checked.
    fn null_compared(&self, op: BinaryOp, expr: &Expr, other: &Expr, ty: Type) -> Type {
        let is_equality = matches!(op, BinaryOp::Eq | BinaryOp::NotEq);
        match (&expr.kind, &other.kind) {
            (ExprKind::Ident { name, .. }, ExprKind::Null) if is_equality => self
                .scopes
                .lookup(name)
                .map_or(ty, |info| info.declared.clone()),
            _ => ty,
        }
    }

    /// Checks a list literal.
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
            if !self.fits(&found, &element_type) {
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

    /// Checks a map literal.
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

    /// Checks a key or a value of a map literal.
    fn check_map_part(
        &mut self,
        part: &'static str,
        expr: &mut Expr,
        expected: &Type,
    ) -> Result<(), SomoniError> {
        let found = self.check_expr_expecting(expr, expected)?;
        if !self.fits(&found, expected) {
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

    /// Checks a struct literal, or returns T051 for a class written like a struct.
    fn check_struct_literal(
        &mut self,
        name: &str,
        name_span: Span,
        fields: &mut [FieldInit],
        index: &mut Option<usize>,
    ) -> Result<Type, SomoniError> {
        if self.names.classes.contains_key(name) {
            return Err(SomoniError::at(
                name_span,
                ErrorCode::ClassLiteral {
                    name: name.to_string(),
                },
            ));
        }
        let decl = self.struct_named(name, name_span)?;
        let info = self.struct_info(&decl)?.clone();
        *index = Some(info.index);
        let given = self.check_field_inits(name, fields, &info.fields)?;
        check_no_missing_field(name, name_span, &info.fields, &given)?;
        Ok(Type::Struct(decl))
    }

    /// Checks each `field: value` of a literal of struct `name` and returns which fields it gives, or T026 for a field twice.
    fn check_field_inits(
        &mut self,
        name: &str,
        fields: &mut [FieldInit],
        declared: &[(String, Type)],
    ) -> Result<Vec<bool>, SomoniError> {
        let mut given = vec![false; declared.len()];
        for init in fields {
            let (position, ty) = field_of(declared, name, &init.name, init.name_span)?;
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
        Ok(given)
    }

    /// Returns the type of an expression.
    pub(super) fn check_expr(&mut self, expr: &mut Expr) -> Result<Type, SomoniError> {
        if let Some((decl, variant)) = self.enum_variant(expr)? {
            let ty = Type::Enum(Decl(Rc::clone(&decl)));
            expr.kind = ExprKind::EnumValue { decl, variant };
            return Ok(ty);
        }
        if let Some((name, constant)) = self.global_constant(expr) {
            let ty = constant.known_type(&name, expr.span)?;
            expr.kind = ExprKind::Constant {
                name,
                global: constant.global,
            };
            return Ok(ty);
        }
        if let Some((name, signature)) = self.function_value(expr)? {
            expr.kind = ExprKind::FunctionRef {
                name,
                index: signature.index,
            };
            return Ok(signature.function_type());
        }
        if let Some((name, index, ty)) = self.captured_variable(expr) {
            expr.kind = ExprKind::Captured { name, index };
            return Ok(ty);
        }
        self.call_through_variable(expr);
        let span = expr.span;
        match &mut expr.kind {
            ExprKind::Int(_) => Ok(Type::Int),
            ExprKind::EnumValue { decl, .. } => Ok(Type::Enum(Decl(Rc::clone(decl)))),
            ExprKind::Constant { name, .. } => self
                .names
                .constants
                .get(name)
                .ok_or_else(|| SomoniError::internal("unknown constant"))?
                .known_type(name, span),
            ExprKind::Float(_) => Ok(Type::Float),
            ExprKind::Bool(_) => Ok(Type::Bool),
            ExprKind::Str(_) => Ok(Type::Str),
            ExprKind::Null => Err(SomoniError::at(span, ErrorCode::NullTypeUnknown)),
            ExprKind::Ident { name, slot } => self.check_variable(name, slot, span),
            ExprKind::Binary {
                left,
                op,
                op_span,
                right,
            } => {
                let (l, r) = self.check_operands(*op, left, right)?;
                self.binary_type(*op, l, r, *op_span)
            }
            ExprKind::List(elements) => self.check_list(elements, span, None),
            ExprKind::Interpolation(parts) => self.check_interpolation(parts),
            ExprKind::Map(entries) => self.check_map(entries, span, None),
            ExprKind::Tuple(items) => self.check_tuple(items, span),
            ExprKind::TupleIndex { target, index } => self.check_tuple_index(target, *index),
            ExprKind::Index { target, index } => self.check_index(target, index),
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
            } => self.check_field(target, name, *name_span, index),
            ExprKind::Call(call) => self.check_call(call, span),
            ExprKind::MethodCall(call) => self.check_method_call(call, span),
            ExprKind::SuperInit { args, target } => self.check_super_init(args, target, span),
            ExprKind::SuperCall(call) => self.check_super_call(call, span),
            ExprKind::New(new) => self.check_new(new, span),
            ExprKind::StaticCall(call) => self.check_static_call(call, span),
            ExprKind::StaticAccess(access) => self.check_static_access(access),
            ExprKind::InstanceOf(test) => self.check_instanceof(test),
            ExprKind::Try(value) => self.check_try(value, span),
            ExprKind::FunctionRef { name, .. } => self
                .names
                .functions
                .get(name)
                .map(Signature::function_type)
                .ok_or_else(|| SomoniError::internal("function value without signature")),
            ExprKind::CallValue(call) => self.check_call_value(call, span),
            ExprKind::Lambda(lambda) => self.check_lambda(lambda, None),
            ExprKind::Captured { name, .. } => self
                .lambda_scopes
                .last()
                .and_then(|level| level.captures.iter().find(|(c, _)| c.name == *name))
                .map(|(_, ty)| ty.clone())
                .ok_or_else(|| SomoniError::internal("copy without type")),
            ExprKind::Unary { op, operand } => self.check_unary(*op, operand, span),
        }
    }

    /// Returns the `const` of the top level that a name outside the top-level code means, if no variable has the name.
    fn global_constant(&self, expr: &Expr) -> Option<(String, Constant)> {
        let ExprKind::Ident { name, .. } = &expr.kind else {
            return None;
        };
        if self.has_variable(name) {
            return None;
        }
        let constant = self.names.constants.get(name)?;
        if self.in_main && constant.own {
            return None;
        }
        Some((name.clone(), constant.clone()))
    }

    /// Returns the own function that a name means when no variable has the name, or T090 for a built-in function.
    fn function_value(&self, expr: &Expr) -> Result<Option<(String, Signature)>, SomoniError> {
        let ExprKind::Ident { name, .. } = &expr.kind else {
            return Ok(None);
        };
        if self.has_variable(name) {
            return Ok(None);
        }
        if Builtin::from_name(name).is_some() {
            let code = ErrorCode::BuiltinAsValue { name: name.clone() };
            return Err(SomoniError::at(expr.span, code));
        }
        let signature = self.names.functions.get(name).cloned();
        Ok(signature.map(|signature| (name.clone(), signature)))
    }

    /// Turns `f(args)` into a call of a function value if `f` is a variable or a constant, which hide an own function.
    fn call_through_variable(&self, expr: &mut Expr) {
        let ExprKind::Call(call) = &mut expr.kind else {
            return;
        };
        if Builtin::from_name(&call.name).is_some() {
            return;
        }
        let callee = Expr {
            kind: ExprKind::Ident {
                name: call.name.clone(),
                slot: None,
            },
            span: call.name_span,
        };
        if !self.has_variable(&call.name) && self.global_constant(&callee).is_none() {
            return;
        }
        let args = std::mem::take(&mut call.args);
        expr.kind = ExprKind::CallValue(CallValue {
            callee: Box::new(callee),
            args,
        });
    }

    /// Checks a tuple literal without an expected type, or returns T094 for fewer than two elements.
    fn check_tuple(&mut self, items: &mut [Expr], span: Span) -> Result<Type, SomoniError> {
        if items.len() < 2 {
            return Err(SomoniError::at(span, ErrorCode::TupleTooShort));
        }
        let types = items
            .iter_mut()
            .map(|item| self.check_expr(item))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Type::Tuple(types.into()))
    }

    /// Checks `target.0` and returns the type of that element, or T095 or T096.
    fn check_tuple_index(&mut self, target: &mut Expr, index: usize) -> Result<Type, SomoniError> {
        let found = self.check_expr(target)?;
        let Type::Tuple(elements) = &found else {
            return Err(SomoniError::at(target.span, ErrorCode::NotATuple { found }));
        };
        match elements.get(index) {
            Some(element) => Ok(element.clone()),
            None => {
                let code = ErrorCode::TupleIndexOutOfRange {
                    found: found.clone(),
                    index,
                };
                Err(SomoniError::at(target.span, code))
            }
        }
    }

    /// Checks a lambda like a function of its own whose slot 0 holds its value, and collects the outside variables it copies.
    fn check_lambda(
        &mut self,
        lambda: &mut Lambda,
        hint: Option<FunctionHint>,
    ) -> Result<Type, SomoniError> {
        let index = self.lambda_count;
        self.lambda_count += 1;
        lambda.index = Some(index);
        let params = self.lambda_params(&lambda.function.params, hint.as_ref())?;
        let returns = match (
            &lambda.function.return_type,
            hint.and_then(|hint| hint.result),
        ) {
            (Some(written), _) => Returns::Known(self.resolve_type(written, true)?),
            (None, Some(result)) => Returns::Known(result),
            (None, None) => Returns::FromBody,
        };
        let ty = Type::function(params.clone(), returns.returned_type());
        let outer = std::mem::replace(&mut self.scopes, Scopes::new());
        self.lambda_scopes.push(LambdaScope {
            outer,
            captures: Vec::new(),
        });
        let name = lambda.function.name.clone();
        let hidden = Some((CLOSURE, ty.clone()));
        let checked = self.check_body_with(&mut lambda.function, &name, &params, returns, hidden);
        let level = self
            .lambda_scopes
            .pop()
            .ok_or_else(|| SomoniError::internal("lambda scope missing"))?;
        self.scopes = level.outer;
        let return_type = checked?;
        lambda.captures = level
            .captures
            .into_iter()
            .map(|(capture, _)| capture)
            .collect();
        Ok(Type::function(params, return_type))
    }

    /// Checks the expressions of `"Hello {name}"`, which must be values that `string(...)` turns into text, or T105.
    fn check_interpolation(&mut self, parts: &mut [Expr]) -> Result<Type, SomoniError> {
        for part in parts {
            let found = self.check_expr(part)?;
            if !matches!(found, Type::Int | Type::Float | Type::Bool | Type::Str) {
                let code = ErrorCode::InterpolationType { found };
                return Err(SomoniError::at(part.span, code));
            }
        }
        Ok(Type::Str)
    }

    /// Returns the types of the parameters of a lambda: as written, else from `hint`, else T103 (T104 if `hint` has another count).
    fn lambda_params(
        &self,
        params: &[Param],
        hint: Option<&FunctionHint>,
    ) -> Result<Vec<Type>, SomoniError> {
        let missing = params.iter().find(|param| param.type_name.is_none());
        if let (Some(hint), Some(param)) = (hint, missing)
            && hint.params.len() != params.len()
        {
            let code = ErrorCode::LambdaParamCount {
                expected: hint.params.len(),
                found: params.len(),
            };
            return Err(SomoniError::at(param.name_span, code));
        }
        params
            .iter()
            .enumerate()
            .map(|(place, param)| match (&param.type_name, hint) {
                (Some(written), _) => self.resolve_type(written, false),
                (None, Some(hint)) => hint
                    .params
                    .get(place)
                    .cloned()
                    .ok_or_else(|| SomoniError::internal("lambda hint too short")),
                (None, None) => Err(SomoniError::at(
                    param.name_span,
                    ErrorCode::LambdaParamType {
                        name: param.name.clone(),
                    },
                )),
            })
            .collect()
    }

    /// Returns the copy number and type of an outside variable that a name inside a lambda means, copying it if needed.
    fn captured_variable(&mut self, expr: &Expr) -> Option<(String, usize, Type)> {
        let ExprKind::Ident { name, .. } = &expr.kind else {
            return None;
        };
        if self.scopes.lookup(name).is_some() {
            return None;
        }
        let depth = self.lambda_scopes.len().checked_sub(1)?;
        let (index, ty) = self.capture_at(depth, name)?;
        Some((name.clone(), index, ty))
    }

    /// Checks `callee(args)`, or returns T091 if the callee is no function.
    fn check_call_value(&mut self, call: &mut CallValue, span: Span) -> Result<Type, SomoniError> {
        let found = self.check_expr(&mut call.callee)?;
        let Type::Function(function) = &found else {
            let code = ErrorCode::NotCallable { found };
            return Err(SomoniError::at(call.callee.span, code));
        };
        let name = match &call.callee.kind {
            ExprKind::Ident { name, .. } => name.clone(),
            _ => found.to_string(),
        };
        self.check_args(&name, &function.params, &mut call.args, span)?;
        Ok(function.result.clone())
    }

    /// Checks a variable and gives it its slot.
    fn check_variable(
        &self,
        name: &str,
        slot: &mut Option<usize>,
        span: Span,
    ) -> Result<Type, SomoniError> {
        let info = self.scopes.lookup(name).ok_or_else(|| {
            let code = if name == THIS {
                ErrorCode::ThisOutsideClass
            } else if self.names.modules.contains(name) {
                ErrorCode::ModuleNotValue {
                    name: name.to_string(),
                }
            } else {
                ErrorCode::UnknownVariable {
                    name: name.to_string(),
                }
            };
            SomoniError::at(span, code)
        })?;
        *slot = Some(info.slot);
        Ok(info.ty.clone())
    }

    /// Checks `list[index]` or `map[key]`.
    fn check_index(&mut self, target: &mut Expr, index: &mut Expr) -> Result<Type, SomoniError> {
        match self.check_expr(target)? {
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

    /// Checks `target.name` on a struct or object.
    fn check_field(
        &mut self,
        target: &mut Expr,
        name: &str,
        name_span: Span,
        index: &mut Option<usize>,
    ) -> Result<Type, SomoniError> {
        let target_type = self.check_expr(target)?;
        let Some(fields) = self.fields_of(&target_type) else {
            return Err(SomoniError::at(
                target.span,
                ErrorCode::NoFields { found: target_type },
            ));
        };
        let owner = target_type.to_string();
        if let Type::Class(layout) = &target_type
            && self.class_info(layout)?.static_fields.contains_key(name)
        {
            let code = ErrorCode::StaticFieldOnObject {
                class: layout.name.clone(),
                name: name.to_string(),
            };
            return Err(SomoniError::at(name_span, code));
        }
        let (position, ty) = field_of(fields, &owner, name, name_span)?;
        *index = Some(position);
        let ty = ty.clone();
        if let Type::Class(layout) = &target_type {
            self.check_access(layout, name, name_span)?;
        }
        Ok(ty)
    }

    /// Checks a call of a built-in or an own function.
    fn check_call(&mut self, call: &mut Call, span: Span) -> Result<Type, SomoniError> {
        if let Some(builtin) = Builtin::from_name(&call.name) {
            return self.check_builtin_call(builtin, call, span);
        }
        let Some(signature) = self.names.functions.get(&call.name).cloned() else {
            return Err(SomoniError::at(
                call.name_span,
                ErrorCode::UnknownFunction {
                    name: call.name.clone(),
                },
            ));
        };
        call.target = Some(CallTarget::Function(signature.index));
        self.check_args(&call.name, &signature.params, &mut call.args, span)?;
        Ok(signature.return_type)
    }

    /// Checks a call of a built-in function, or returns T049 for `ok` and `err` without an expected type.
    fn check_builtin_call(
        &mut self,
        builtin: Builtin,
        call: &mut Call,
        span: Span,
    ) -> Result<Type, SomoniError> {
        if builtin.builds_result() {
            return Err(SomoniError::at(
                span,
                ErrorCode::ResultTypeUnknown {
                    name: builtin.name(),
                },
            ));
        }
        call.target = Some(CallTarget::Builtin(builtin));
        check_arg_count(&call.name, builtin.arity(), &call.args, span)?;
        let mut arg_types = Vec::new();
        for arg in call.args.iter_mut() {
            arg_types.push(self.check_expr(arg)?);
        }
        builtin
            .result_type(&arg_types)
            .map_err(|bad| wrong_arg_type(&call.name, arg_types[bad].clone(), call.args[bad].span))
    }

    /// Checks `target.name(args)` on an object or an interface.
    fn check_method_call(
        &mut self,
        call: &mut MethodCall,
        span: Span,
    ) -> Result<Type, SomoniError> {
        let target_type = self.check_expr(&mut call.target)?;
        match &target_type {
            Type::Interface(layout) => self.check_interface_call(layout, call, span),
            Type::Class(layout) => self.check_class_method_call(layout, call, span),
            Type::Struct(_) if self.has_field(&target_type, &call.name) => {
                self.check_field_call(&target_type, call, span)
            }
            Type::Str | Type::Float | Type::List(_) | Type::Map(_, _) => {
                self.check_builtin_method(target_type, call, span)
            }
            _ => Err(SomoniError::at(
                call.target.span,
                ErrorCode::NoMethods { found: target_type },
            )),
        }
    }

    /// Checks `s.upper()` and other built-in methods, which get the value before the dot as their first argument.
    fn check_builtin_method(
        &mut self,
        target_type: Type,
        call: &mut MethodCall,
        span: Span,
    ) -> Result<Type, SomoniError> {
        let unknown = || unknown_method(&target_type.to_string(), &call.name, call.name_span);
        let Some(builtin) = Builtin::method_named(&call.name) else {
            return Err(unknown());
        };
        check_arg_count(&call.name, builtin.arity() - 1, &call.args, span)?;
        let mut types = vec![target_type.clone()];
        for arg in call.args.iter_mut() {
            let found = match (&mut arg.kind, builtin.function_hint(&types)) {
                (ExprKind::Lambda(lambda), Some(hint)) => self.check_lambda(lambda, Some(hint))?,
                _ => self.check_expr(arg)?,
            };
            types.push(found);
        }
        let result = match builtin.result_type(&types) {
            Ok(result) => result,
            Err(0) => return Err(unknown()),
            Err(bad) => {
                let found = types[bad].clone();
                return Err(wrong_arg_type(&call.name, found, call.args[bad - 1].span));
            }
        };
        call.call = Some(MethodTarget::Builtin(builtin));
        Ok(result)
    }

    /// Checks a method call on a value of an interface type.
    fn check_interface_call(
        &mut self,
        layout: &Decl<InterfaceLayout>,
        call: &mut MethodCall,
        span: Span,
    ) -> Result<Type, SomoniError> {
        let info = self.interface_info(layout)?;
        let found = info.methods.iter().find(|(m, _)| *m == call.name).cloned();
        let Some((_, signature)) = found else {
            return Err(unknown_method(&layout.name, &call.name, call.name_span));
        };
        call.call = Some(MethodTarget::Interface {
            interface: info.index,
            method: signature.index,
        });
        self.check_args(&call.name, &signature.params, &mut call.args, span)?;
        Ok(signature.return_type)
    }

    /// Checks a method call on an object of a class, or returns T075 for a static method.
    fn check_class_method_call(
        &mut self,
        layout: &Decl<ClassLayout>,
        call: &mut MethodCall,
        span: Span,
    ) -> Result<Type, SomoniError> {
        let info = self.class_info(layout)?;
        let Some(method) = info.methods.get(&call.name).cloned() else {
            let object = Type::Class(layout.clone());
            if self.has_field(&object, &call.name) {
                return self.check_field_call(&object, call, span);
            }
            if !info.statics.contains_key(&call.name) {
                return Err(unknown_method(&layout.name, &call.name, call.name_span));
            }
            let code = ErrorCode::StaticOnObject {
                class: layout.name.clone(),
                name: call.name.clone(),
            };
            return Err(SomoniError::at(call.name_span, code));
        };
        self.check_access(layout, &call.name, call.name_span)?;
        call.call = Some(MethodTarget::Virtual { slot: method.slot });
        self.check_args(&call.name, &method.signature.params, &mut call.args, span)?;
        Ok(method.signature.return_type)
    }

    /// Returns true if the struct or class has a field with this name.
    fn has_field(&self, ty: &Type, name: &str) -> bool {
        self.fields_of(ty)
            .is_some_and(|fields| fields.iter().any(|(field, _)| field == name))
    }

    /// Checks `k.aktion(args)` for a field with a function type, or returns T091 for a field of another type.
    fn check_field_call(
        &mut self,
        object: &Type,
        call: &mut MethodCall,
        span: Span,
    ) -> Result<Type, SomoniError> {
        let fields = self
            .fields_of(object)
            .ok_or_else(|| SomoniError::internal("field call without fields"))?;
        let (position, ty) = field_of(fields, &object.to_string(), &call.name, call.name_span)?;
        let Type::Function(function) = ty.clone() else {
            let code = ErrorCode::NotCallable { found: ty.clone() };
            return Err(SomoniError::at(call.name_span, code));
        };
        if let Type::Class(layout) = object {
            self.check_access(layout, &call.name, call.name_span)?;
        }
        call.call = Some(MethodTarget::Field { position });
        self.check_args(&call.name, &function.params, &mut call.args, span)?;
        Ok(function.result.clone())
    }

    /// Checks `super(args)`.
    fn check_super_init(
        &mut self,
        args: &mut [Expr],
        target: &mut Option<usize>,
        span: Span,
    ) -> Result<Type, SomoniError> {
        let parent = self.current_parent(span)?;
        if !self.super_init_allowed {
            return Err(SomoniError::at(span, ErrorCode::SuperInitMisplaced));
        }
        let constructor = self.class_info(&parent)?.constructor.clone();
        *target = Some(constructor.index);
        self.check_args(&parent.name, &constructor.params, args, span)?;
        Ok(Type::Void)
    }

    /// Checks `super.name(args)`, or returns T078 for an abstract method.
    fn check_super_call(&mut self, call: &mut SuperCall, span: Span) -> Result<Type, SomoniError> {
        let parent = self.current_parent(span)?;
        let info = self.class_info(&parent)?;
        let Some(method) = info.methods.get(&call.name).cloned() else {
            return Err(unknown_method(&parent.name, &call.name, call.name_span));
        };
        if method.is_abstract {
            let code = ErrorCode::AbstractSuper {
                class: parent.name.clone(),
                name: call.name.clone(),
            };
            return Err(SomoniError::at(call.name_span, code));
        }
        call.target = Some(info.method_table[method.slot]);
        self.check_access(&parent, &call.name, call.name_span)?;
        self.check_args(&call.name, &method.signature.params, &mut call.args, span)?;
        Ok(method.signature.return_type)
    }

    /// Checks `Class::name` for a static field, or returns T074 for a field that is not static and T085 for none.
    fn check_static_access(&mut self, access: &mut StaticAccess) -> Result<Type, SomoniError> {
        let layout = self.class_named(&access.class_name, access.class_span)?;
        let info = self.class_info(&layout)?;
        let Some(field) = info.static_fields.get(&access.name).cloned() else {
            let class = layout.name.clone();
            let name = access.name.clone();
            let code = if info.fields.iter().any(|(field, _)| *field == access.name) {
                ErrorCode::NotStatic { class, name }
            } else {
                ErrorCode::UnknownStaticField { class, name }
            };
            return Err(SomoniError::at(access.name_span, code));
        };
        self.check_visible(
            field.visibility,
            &field.owner,
            &access.name,
            access.name_span,
        )?;
        access.global = Some(field.global);
        Ok(field.ty)
    }

    /// Checks `Class::name(args)`, or returns T074 for a method that is not static.
    fn check_static_call(
        &mut self,
        call: &mut StaticCall,
        span: Span,
    ) -> Result<Type, SomoniError> {
        let layout = self.class_named(&call.class_name, call.class_span)?;
        let info = self.class_info(&layout)?;
        let Some(method) = info.statics.get(&call.name).cloned() else {
            if !info.methods.contains_key(&call.name) {
                return Err(unknown_method(&layout.name, &call.name, call.name_span));
            }
            let code = ErrorCode::NotStatic {
                class: layout.name.clone(),
                name: call.name.clone(),
            };
            return Err(SomoniError::at(call.name_span, code));
        };
        self.check_visible(method.visibility, &method.owner, &call.name, call.name_span)?;
        call.target = Some(method.signature.index);
        self.check_args(&call.name, &method.signature.params, &mut call.args, span)?;
        Ok(method.signature.return_type)
    }

    /// Checks `value instanceof Name`, which is a bool, or returns T079 or T080.
    fn check_instanceof(&mut self, test: &mut InstanceOf) -> Result<Type, SomoniError> {
        let found = self.check_expr(&mut test.value)?;
        let object = match &found {
            Type::Optional(inner) => (**inner).clone(),
            other => other.clone(),
        };
        if !matches!(object, Type::Class(_) | Type::Interface(_)) {
            let code = ErrorCode::InstanceofNotObject { found };
            return Err(SomoniError::at(test.value.span, code));
        }
        let (wanted, target) = self.instance_type(&test.type_name, test.type_span)?;
        if let (Type::Class(from), Type::Class(to)) = (&object, &wanted)
            && !self.is_subclass(from, to)
            && !self.is_subclass(to, from)
        {
            let code = ErrorCode::InstanceofNever {
                from: object,
                to: wanted,
            };
            return Err(SomoniError::at(test.type_span, code));
        }
        test.target = Some(target);
        Ok(Type::Bool)
    }

    /// Checks `value?` and returns the type inside the `ok`; the function must return a result with the same error type.
    fn check_try(&mut self, value: &mut Expr, span: Span) -> Result<Type, SomoniError> {
        let (ok_type, err_type) = match self.check_expr(value)? {
            Type::Result(ok_type, err_type) => (*ok_type, *err_type),
            found => {
                return Err(SomoniError::at(
                    value.span,
                    ErrorCode::TryNotResult { found },
                ));
            }
        };
        let Returns::Known(expected @ Type::Result(_, expected_err)) = &self.current_return else {
            return Err(SomoniError::at(span, ErrorCode::TryOutsideResultFunction));
        };
        if **expected_err != err_type {
            let code = ErrorCode::TryErrorType {
                found: err_type,
                expected: expected.clone(),
            };
            return Err(SomoniError::at(span, code));
        }
        Ok(ok_type)
    }

    /// Returns the class or interface type named after `instanceof` and its number, or T081 for another type.
    fn instance_type(&self, name: &str, span: Span) -> Result<(Type, InstanceTarget), SomoniError> {
        if let Some(layout) = self.names.classes.get(name) {
            let class = self.class_info(layout)?.index;
            return Ok((Type::Class(layout.clone()), InstanceTarget::Class(class)));
        }
        if let Some(layout) = self.names.interfaces.get(name) {
            let interface = self.interface_info(layout)?.index;
            return Ok((
                Type::Interface(layout.clone()),
                InstanceTarget::Interface(interface),
            ));
        }
        let code = if self.is_type_name(name) {
            ErrorCode::InstanceofNotType {
                name: name.to_string(),
            }
        } else {
            ErrorCode::UnknownType {
                name: name.to_string(),
            }
        };
        Err(SomoniError::at(span, code))
    }

    /// Checks `new Name(args)`, or returns T076 for an abstract class.
    fn check_new(&mut self, new: &mut New, span: Span) -> Result<Type, SomoniError> {
        let layout = self.class_named(&new.name, new.name_span)?;
        let info = self.class_info(&layout)?;
        if info.is_abstract {
            let code = ErrorCode::AbstractNew {
                name: layout.name.clone(),
            };
            return Err(SomoniError::at(new.name_span, code));
        }
        self.check_visible(
            info.constructor_visibility,
            &layout,
            CONSTRUCTOR,
            new.name_span,
        )?;
        let constructor = info.constructor.clone();
        new.class = Some(info.index);
        new.target = Some(constructor.index);
        self.check_args(&new.name, &constructor.params, &mut new.args, span)?;
        Ok(constructor.return_type)
    }

    /// Checks `!x` or `-x`.
    fn check_unary(
        &mut self,
        op: UnaryOp,
        operand: &mut Expr,
        span: Span,
    ) -> Result<Type, SomoniError> {
        let ty = self.check_expr(operand)?;
        match (op, &ty) {
            (UnaryOp::Not, Type::Bool) => Ok(Type::Bool),
            (UnaryOp::Not, _) => Err(SomoniError::at(span, ErrorCode::NotNeedsBool { found: ty })),
            (UnaryOp::Neg, Type::Int | Type::Float) => Ok(ty),
            (UnaryOp::Neg, _) => Err(SomoniError::at(
                span,
                ErrorCode::NegNeedsNumber { found: ty },
            )),
        }
    }

    /// Returns the type of `l op r`.
    pub(super) fn binary_type(
        &self,
        op: BinaryOp,
        l: Type,
        r: Type,
        op_span: Span,
    ) -> Result<Type, SomoniError> {
        let is_equality = matches!(op, BinaryOp::Eq | BinaryOp::NotEq);
        if l != r && !(is_equality && (self.fits(&l, &r) || self.fits(&r, &l))) {
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
            (Type::Function(_), _) => Err(SomoniError::at(
                op_span,
                ErrorCode::OperatorNotAllowed { found: l },
            )),
            (_, BinaryOp::Eq | BinaryOp::NotEq) => Ok(Type::Bool),
            (Type::Int, BinaryOp::Mod) => Ok(Type::Int),
            (
                Type::Int | Type::Float | Type::Str,
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
}

/// Returns T028 for the first field of struct `name` that a literal at `span` does not give.
fn check_no_missing_field(
    name: &str,
    span: Span,
    declared: &[(String, Type)],
    given: &[bool],
) -> Result<(), SomoniError> {
    let missing = declared.iter().zip(given).find(|(_, given)| !**given);
    if let Some(((field, _), _)) = missing {
        return Err(SomoniError::at(
            span,
            ErrorCode::MissingField {
                struct_name: name.to_string(),
                name: field.clone(),
            },
        ));
    }
    Ok(())
}

/// Returns the error T056 for a method that the class or interface `owner` does not have.
fn unknown_method(owner: &str, name: &str, span: Span) -> SomoniError {
    SomoniError::at(
        span,
        ErrorCode::UnknownMethod {
            class: owner.to_string(),
            name: name.to_string(),
        },
    )
}

/// Returns true if an expression gets its type from where it is used.
fn needs_type_hint(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::List(elements) => elements.is_empty(),
        ExprKind::Map(entries) => entries.is_empty(),
        ExprKind::Tuple(items) => items.iter().any(needs_type_hint),
        ExprKind::Null => true,
        ExprKind::Call(call) => Builtin::result_builder(&call.name).is_some(),
        _ => false,
    }
}
