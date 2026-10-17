//! Enum values and `match`, also over `result`.

use std::rc::Rc;

use crate::ast::{EnumDecl, Expr, ExprKind, MatchArm, Pattern};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::types::Type;

use super::TypeChecker;

impl TypeChecker {
    /// Checks the branches of a `match` over a value of type `ty`: every variant needs one,
    /// either its own or `_`. Returns, for each variant, the number of its branch.
    pub(super) fn check_match(
        &self,
        ty: &Type,
        subject_span: Span,
        arms: &[MatchArm],
        span: Span,
    ) -> Result<Vec<usize>, SomoniError> {
        let Some(variants) = variant_names(ty) else {
            return Err(SomoniError::at(
                subject_span,
                ErrorCode::MatchNotEnum { found: ty.clone() },
            ));
        };
        let mut branch_of: Vec<Option<usize>> = vec![None; variants.len()];
        let mut rest: Option<(usize, Span)> = None;
        for (i, arm) in arms.iter().enumerate() {
            if let Some((_, rest_span)) = rest {
                return Err(SomoniError::at(rest_span, ErrorCode::RestNotLast));
            }
            let (variant, pattern_span) = match &arm.pattern {
                Pattern::Rest(rest_span) => {
                    rest = Some((i, *rest_span));
                    continue;
                }
                pattern => pattern_variant(ty, pattern)?,
            };
            if branch_of[variant].is_some() {
                return Err(SomoniError::at(
                    pattern_span,
                    ErrorCode::DuplicateBranch {
                        name: variants[variant].clone(),
                    },
                ));
            }
            branch_of[variant] = Some(i);
        }
        let missing: Vec<String> = variants
            .into_iter()
            .zip(&branch_of)
            .filter(|(_, branch)| branch.is_none())
            .map(|(name, _)| name)
            .collect();
        match rest {
            Some((_, rest_span)) if missing.is_empty() => {
                Err(SomoniError::at(rest_span, ErrorCode::RestUnreachable))
            }
            Some((rest_branch, _)) => Ok(branch_of
                .into_iter()
                .map(|branch| branch.unwrap_or(rest_branch))
                .collect()),
            None if !missing.is_empty() => Err(SomoniError::at(
                span,
                ErrorCode::MissingBranch {
                    missing: missing.join(", "),
                },
            )),
            None => Ok(branch_of.into_iter().flatten().collect()),
        }
    }

    /// Checks the body of one branch of a `match` over `subject`.
    /// The variable of `ok(n: int)` exists only inside its branch.
    pub(super) fn check_arm(
        &mut self,
        arm: &mut MatchArm,
        subject: &Type,
    ) -> Result<(), SomoniError> {
        let Pattern::Result { ok, var, slot, .. } = &mut arm.pattern else {
            return self.check_stmt(&mut arm.body);
        };
        let Type::Result(value, error) = subject else {
            return Err(SomoniError::internal(
                "ok or err in a match over a non-result",
            ));
        };
        let given = if *ok { value } else { error };
        self.in_scope(|this| {
            let slots =
                this.declare_bound_vars(std::slice::from_ref(var), vec![(**given).clone()])?;
            *slot = slots.first().copied();
            this.check_stmt(&mut arm.body)
        })
    }

    /// If `expr` is `Farbe.Rot` and `Farbe` is an enum (and no variable), returns the enum
    /// and the number of the variant.
    pub(super) fn enum_variant(
        &self,
        expr: &Expr,
    ) -> Result<Option<(Rc<EnumDecl>, usize)>, SomoniError> {
        let ExprKind::Field {
            target,
            name,
            name_span,
            ..
        } = &expr.kind
        else {
            return Ok(None);
        };
        let ExprKind::Ident {
            name: enum_name, ..
        } = &target.kind
        else {
            return Ok(None);
        };
        if self.scopes.lookup(enum_name).is_some() {
            return Ok(None);
        }
        let Some(decl) = self.names.enums.get(enum_name) else {
            return Ok(None);
        };
        let variant = variant_of(decl, name, *name_span)?;
        Ok(Some((Rc::clone(&decl.0), variant)))
    }
}

/// Returns the variants a `match` over `ty` must cover, as written in a branch:
/// `Farbe.Rot, ...` for an enum, `ok` and `err` for a result. None if `match` cannot take `ty`.
fn variant_names(ty: &Type) -> Option<Vec<String>> {
    match ty {
        Type::Enum(decl) => Some(
            decl.variants
                .iter()
                .map(|(name, _)| format!("{}.{}", decl.name, name))
                .collect(),
        ),
        Type::Result(_, _) => Some(vec!["ok".to_string(), "err".to_string()]),
        _ => None,
    }
}

/// Returns the number of the variant a branch stands for (`ok` is 0, `err` is 1) and the
/// place of the pattern, or T042 if the pattern does not fit the type of the `match`.
fn pattern_variant(ty: &Type, pattern: &Pattern) -> Result<(usize, Span), SomoniError> {
    let (found, span) = match (ty, pattern) {
        (
            Type::Enum(decl),
            Pattern::Variant {
                enum_name,
                name,
                name_span,
                ..
            },
        ) if *enum_name == decl.name => {
            return Ok((variant_of(decl, name, *name_span)?, *name_span));
        }
        (Type::Result(_, _), Pattern::Result { ok, span, .. }) => {
            return Ok((if *ok { 0 } else { 1 }, *span));
        }
        (
            _,
            Pattern::Variant {
                enum_name,
                enum_span,
                ..
            },
        ) => (enum_name.clone(), *enum_span),
        (_, Pattern::Result { span, .. }) => ("result".to_string(), *span),
        (_, Pattern::Rest(_)) => return Err(SomoniError::internal("'_' has no variant")),
    };
    Err(SomoniError::at(
        span,
        ErrorCode::PatternWrongEnum {
            expected: ty.clone(),
            found,
        },
    ))
}

/// Returns the number of the variant `name` of an enum, or T040 if it has none.
fn variant_of(decl: &EnumDecl, name: &str, span: Span) -> Result<usize, SomoniError> {
    decl.variants
        .iter()
        .position(|(variant, _)| variant == name)
        .ok_or_else(|| {
            SomoniError::at(
                span,
                ErrorCode::UnknownVariant {
                    enum_name: decl.name.clone(),
                    name: name.to_string(),
                },
            )
        })
}
