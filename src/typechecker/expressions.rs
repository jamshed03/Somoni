//! Checking expressions and the types of operators.

use std::rc::Rc;

use crate::ast::{BinaryOp, CallTarget, Expr, ExprKind, FieldInit, MethodTarget, THIS, UnaryOp};
use crate::builtins::Builtin;
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::types::{Decl, Type};

use super::declarations::check_key_type;
use super::{TypeChecker, check_arg_count, field_of, wrong_arg_type};

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
            (ExprKind::Null, _) if matches!(expected, Type::Optional(_)) => Ok(expected.clone()),
            (ExprKind::Null, _) => Err(SomoniError::at(
                expr.span,
                ErrorCode::NullNotAllowed {
                    expected: expected.clone(),
                },
            )),
            (
                ExprKind::Call {
                    name, args, target, ..
                },
                Type::Result(value, error),
            ) if Builtin::result_builder(name).is_some() => {
                let builtin = Builtin::result_builder(name)
                    .ok_or_else(|| SomoniError::internal("ok or err is no built-in"))?;
                *target = Some(CallTarget::Builtin(builtin));
                check_arg_count(name, builtin.arity(), args, expr.span)?;
                let inner = if builtin == Builtin::Ok { value } else { error };
                let found = self.check_expr_expecting(&mut args[0], inner)?;
                if !self.fits(&found, inner) {
                    return Err(wrong_arg_type(name, found, args[0].span));
                }
                Ok(wanted.clone())
            }
            _ => self.check_expr(expr),
        }
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

    /// Checks a struct literal.
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
        let info = &self.struct_info(&decl)?.clone();
        *index = Some(info.index);
        let mut given = vec![false; info.fields.len()];
        for init in fields {
            let (position, ty) = field_of(&info.fields, name, &init.name, init.name_span)?;
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
        Ok(Type::Struct(decl))
    }

    /// Returns the type of an expression.
    pub(super) fn check_expr(&mut self, expr: &mut Expr) -> Result<Type, SomoniError> {
        if let Some((decl, variant)) = self.enum_variant(expr)? {
            let ty = Type::Enum(Decl(Rc::clone(&decl)));
            expr.kind = ExprKind::EnumValue { decl, variant };
            return Ok(ty);
        }
        let span = expr.span;
        match &mut expr.kind {
            ExprKind::Int(_) => Ok(Type::Int),
            ExprKind::EnumValue { decl, .. } => Ok(Type::Enum(Decl(Rc::clone(decl)))),
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
            ExprKind::Map(entries) => self.check_map(entries, span, None),
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
            ExprKind::Call {
                name,
                name_span,
                args,
                target,
            } => self.check_call(name, *name_span, args, target, span),
            ExprKind::MethodCall {
                target,
                name,
                name_span,
                args,
                call,
            } => self.check_method_call(target, name, *name_span, args, call, span),
            ExprKind::SuperInit { args, target } => self.check_super_init(args, target, span),
            ExprKind::SuperCall {
                name,
                name_span,
                args,
                target,
            } => self.check_super_call(name, *name_span, args, target, span),
            ExprKind::New {
                name,
                name_span,
                args,
                class,
                target,
            } => self.check_new(name, *name_span, args, class, target, span),
            ExprKind::Unary { op, operand } => self.check_unary(*op, operand, span),
        }
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
        let (position, ty) = field_of(fields, &owner, name, name_span)?;
        *index = Some(position);
        let ty = ty.clone();
        if let Type::Class(layout) = &target_type {
            self.check_access(layout, name, name_span)?;
        }
        Ok(ty)
    }

    /// Checks a call of a built-in or an own function.
    fn check_call(
        &mut self,
        name: &str,
        name_span: Span,
        args: &mut [Expr],
        target: &mut Option<CallTarget>,
        span: Span,
    ) -> Result<Type, SomoniError> {
        if let Some(builtin) = Builtin::from_name(name) {
            if builtin.builds_result() {
                return Err(SomoniError::at(
                    span,
                    ErrorCode::ResultTypeUnknown {
                        name: builtin.name(),
                    },
                ));
            }
            *target = Some(CallTarget::Builtin(builtin));
            check_arg_count(name, builtin.arity(), args, span)?;
            let mut arg_types = Vec::new();
            for arg in args.iter_mut() {
                arg_types.push(self.check_expr(arg)?);
            }
            return builtin
                .result_type(&arg_types)
                .map_err(|bad| wrong_arg_type(name, arg_types[bad].clone(), args[bad].span));
        }
        let Some(signature) = self.names.functions.get(name).cloned() else {
            return Err(SomoniError::at(
                name_span,
                ErrorCode::UnknownFunction {
                    name: name.to_string(),
                },
            ));
        };
        *target = Some(CallTarget::Function(signature.index));
        self.check_args(name, &signature.params, args, span)?;
        Ok(signature.return_type)
    }

    /// Checks `target.name(args)` on an object or an interface.
    fn check_method_call(
        &mut self,
        target: &mut Expr,
        name: &str,
        name_span: Span,
        args: &mut [Expr],
        call: &mut Option<MethodTarget>,
        span: Span,
    ) -> Result<Type, SomoniError> {
        let unknown = |owner: &str| {
            SomoniError::at(
                name_span,
                ErrorCode::UnknownMethod {
                    class: owner.to_string(),
                    name: name.to_string(),
                },
            )
        };
        let target_type = self.check_expr(target)?;
        if let Type::Interface(layout) = &target_type {
            let info = self.interface_info(layout)?;
            let Some((_, signature)) = info.methods.iter().find(|(m, _)| m == name).cloned() else {
                return Err(unknown(&layout.name));
            };
            *call = Some(MethodTarget::Interface {
                interface: info.index,
                method: signature.index,
            });
            self.check_args(name, &signature.params, args, span)?;
            return Ok(signature.return_type);
        }
        let Type::Class(layout) = &target_type else {
            return Err(SomoniError::at(
                target.span,
                ErrorCode::NoMethods { found: target_type },
            ));
        };
        let method = self
            .class_info(layout)?
            .methods
            .get(name)
            .cloned()
            .ok_or_else(|| unknown(&layout.name))?;
        self.check_access(layout, name, name_span)?;
        *call = Some(MethodTarget::Virtual { slot: method.slot });
        self.check_args(name, &method.signature.params, args, span)?;
        Ok(method.signature.return_type)
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

    /// Checks `super.name(args)`.
    fn check_super_call(
        &mut self,
        name: &str,
        name_span: Span,
        args: &mut [Expr],
        target: &mut Option<usize>,
        span: Span,
    ) -> Result<Type, SomoniError> {
        let parent = self.current_parent(span)?;
        let info = self.class_info(&parent)?;
        let Some(method) = info.methods.get(name).cloned() else {
            return Err(SomoniError::at(
                name_span,
                ErrorCode::UnknownMethod {
                    class: parent.name.clone(),
                    name: name.to_string(),
                },
            ));
        };
        *target = Some(info.method_table[method.slot]);
        self.check_access(&parent, name, name_span)?;
        self.check_args(name, &method.signature.params, args, span)?;
        Ok(method.signature.return_type)
    }

    /// Checks `new Name(args)`.
    fn check_new(
        &mut self,
        name: &str,
        name_span: Span,
        args: &mut [Expr],
        class: &mut Option<usize>,
        target: &mut Option<usize>,
        span: Span,
    ) -> Result<Type, SomoniError> {
        let layout = self.class_named(name, name_span)?;
        let info = self.class_info(&layout)?;
        let constructor = info.constructor.clone();
        *class = Some(info.index);
        *target = Some(constructor.index);
        self.check_args(name, &constructor.params, args, span)?;
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
}

/// Returns true if an expression gets its type from where it is used.
fn needs_type_hint(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::List(elements) => elements.is_empty(),
        ExprKind::Map(entries) => entries.is_empty(),
        ExprKind::Null => true,
        ExprKind::Call { name, .. } => Builtin::result_builder(name).is_some(),
        _ => false,
    }
}
