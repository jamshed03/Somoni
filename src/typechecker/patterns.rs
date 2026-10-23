//! Enum values and `match`, also over `result`.

use std::rc::Rc;

use crate::ast::{ERR_VARIANT, EnumDecl, Expr, ExprKind, MatchArm, OK_VARIANT, Pattern};
use crate::error::{ErrorCode, SomoniError};
use crate::span::Span;
use crate::types::Type;

use super::TypeChecker;

impl TypeChecker {
    /// Checks one branch of a `match`.
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

    /// Returns the enum and variant number if `expr` is a variant like `Farbe.Rot`.
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

/// The `_` branch of a `match`: its number and its place.
struct RestArm {
    arm: usize,
    span: Span,
}

/// The branch of each variant in a `match`, and the `_` branch if there is one.
struct Branches {
    of_variant: Vec<Option<usize>>,
    rest: Option<RestArm>,
}

/// Returns the branch of each variant of a `match` over `ty`, or T041 if `ty` is no enum or result.
pub(super) fn check_match(
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
    let branches = find_branches(ty, &variants, arms)?;
    let missing: Vec<String> = variants
        .into_iter()
        .zip(&branches.of_variant)
        .filter(|(_, branch)| branch.is_none())
        .map(|(name, _)| name)
        .collect();
    complete_branches(branches, &missing, span)
}

/// Finds the branch of each variant, or returns T044 for a branch after `_` and T043 for a variant twice.
fn find_branches(
    ty: &Type,
    variants: &[String],
    arms: &[MatchArm],
) -> Result<Branches, SomoniError> {
    let mut branches = Branches {
        of_variant: vec![None; variants.len()],
        rest: None,
    };
    for (i, arm) in arms.iter().enumerate() {
        if let Some(rest) = &branches.rest {
            return Err(SomoniError::at(rest.span, ErrorCode::RestNotLast));
        }
        let (variant, pattern_span) = match &arm.pattern {
            Pattern::Rest(span) => {
                branches.rest = Some(RestArm {
                    arm: i,
                    span: *span,
                });
                continue;
            }
            pattern => pattern_variant(ty, pattern)?,
        };
        if branches.of_variant[variant].is_some() {
            let name = variants[variant].clone();
            return Err(SomoniError::at(
                pattern_span,
                ErrorCode::DuplicateBranch { name },
            ));
        }
        branches.of_variant[variant] = Some(i);
    }
    Ok(branches)
}

/// Gives the `missing` variants the `_` branch, or returns T046 without one and T045 for a `_` that nothing reaches.
fn complete_branches(
    branches: Branches,
    missing: &[String],
    span: Span,
) -> Result<Vec<usize>, SomoniError> {
    match branches.rest {
        Some(rest) if missing.is_empty() => {
            Err(SomoniError::at(rest.span, ErrorCode::RestUnreachable))
        }
        Some(rest) => Ok(branches
            .of_variant
            .into_iter()
            .map(|branch| branch.unwrap_or(rest.arm))
            .collect()),
        None if !missing.is_empty() => Err(SomoniError::at(
            span,
            ErrorCode::MissingBranch {
                missing: missing.join(", "),
            },
        )),
        None => Ok(branches.of_variant.into_iter().flatten().collect()),
    }
}

/// Returns the variants a `match` over `ty` must cover.
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

/// Returns the number and place of the variant a branch stands for.
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
            return Ok((if *ok { OK_VARIANT } else { ERR_VARIANT }, *span));
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

/// Returns the number of a variant of an enum.
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
