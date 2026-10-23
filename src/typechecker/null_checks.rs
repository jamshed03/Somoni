//! Null checks: which variables cannot be null after a condition like `a != null`.

use crate::ast::{BinaryOp, Expr, ExprKind, UnaryOp};
use crate::types::Type;

use super::{TypeChecker, VarInfo};

impl TypeChecker {
    /// Runs `check` while the variables in `known` are known not to be null.
    pub(super) fn with_not_null<T>(
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

    /// Returns the variables that cannot be null when `condition` is `when`.
    pub(super) fn known_not_null(&self, condition: &Expr, when: bool) -> Vec<(String, Type)> {
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

    /// Returns the variable and its type without `?` for `a != null`.
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
}
