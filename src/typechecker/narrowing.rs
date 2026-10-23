//! Narrowing: which variables have a narrower type after a condition, like `a != null` or `a instanceof Hund`.

use crate::ast::{BinaryOp, Expr, ExprKind, InstanceOf, UnaryOp};
use crate::types::Type;

use super::{TypeChecker, VarInfo};

impl TypeChecker {
    /// Runs `check` while the variables in `known` have their narrower types.
    pub(super) fn with_narrowed<T>(
        &mut self,
        known: Vec<(String, Type)>,
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.scopes.push();
        for (name, ty) in known {
            if let Some(info) = self.scopes.lookup(&name) {
                let narrowed = VarInfo {
                    ty,
                    declared: info.declared.clone(),
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

    /// Gives a variable after `name = value` the type of the value for the rest of this block.
    /// Checks in the blocks around end too, because the assignment may run on only some paths.
    pub(super) fn narrow_after_assign(&mut self, name: &str, found: Type) {
        let Some(info) = self.scopes.lookup(name) else {
            return;
        };
        let (slot, declared) = (info.slot, info.declared.clone());
        self.scopes.update_all(name, |info| {
            if info.slot == slot {
                info.ty = info.declared.clone();
            }
        });
        if found != declared {
            let mutable = self.scopes.lookup(name).is_some_and(|info| info.mutable);
            let narrowed = VarInfo {
                ty: found,
                declared,
                mutable,
                slot,
            };
            self.scopes.set_innermost(name, narrowed);
        }
    }

    /// Returns the variables with a narrower type when `condition` is `when`, e.g. not null.
    pub(super) fn known_narrowed(&self, condition: &Expr, when: bool) -> Vec<(String, Type)> {
        match &condition.kind {
            ExprKind::Binary {
                left, op, right, ..
            } => match (op, when) {
                (BinaryOp::NotEq, true) | (BinaryOp::Eq, false) => {
                    self.null_checked(left, right).into_iter().collect()
                }
                (BinaryOp::And, true) | (BinaryOp::Or, false) => {
                    let mut known = self.known_narrowed(left, when);
                    known.extend(self.known_narrowed(right, when));
                    known
                }
                _ => Vec::new(),
            },
            ExprKind::Unary {
                op: UnaryOp::Not,
                operand,
            } => self.known_narrowed(operand, !when),
            ExprKind::InstanceOf(test) if when => self.instance_checked(test).into_iter().collect(),
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

    /// Returns the variable and its type for `a instanceof Name`, which is `Name` unless the variable is already narrower.
    fn instance_checked(&self, test: &InstanceOf) -> Option<(String, Type)> {
        let ExprKind::Ident { name, .. } = &test.value.kind else {
            return None;
        };
        let current = match &self.scopes.lookup(name)?.ty {
            Type::Optional(inner) => (**inner).clone(),
            other => other.clone(),
        };
        let wanted = self.names.type_named(&test.type_name)?;
        let narrowed = if self.fits(&current, &wanted) {
            current
        } else {
            wanted
        };
        Some((name.clone(), narrowed))
    }
}
