//! The [`IfStatement`] expression.
//!
//! An `if` expression is a conditional branch in program control.
//!
//! The syntax of an `if` expression is a [`Predicate`], followed by
//! a consequent [`Block`], and an optional trailing alternative `Block`. The
//! condition operands must have the boolean type. If a condition operand
//! evaluates to true, the consequent block is executed and the alternative
//! block is skipped. If a condition operand evaluates to false, the consequent
//! block is skipped and the optional alternative block is evaluated. If no
//! alternative is provided, then the condition resolves to `null`.

use std::fmt;

use value::Value;

use crate::{
    expression::{Block, Predicate, Resolved},
    state::{ExternalEnv, LocalEnv},
    value::VrlValueConvert,
    Context, Expression, TypeDef,
};

/// The [`IfStatement`] expression.
///
/// See module-level documentation for more details.
#[derive(Debug, Clone, PartialEq)]
pub struct IfStatement {
    pub(crate) predicate: Predicate,
    pub(crate) consequent: Block,
    pub(crate) alternative: Option<Block>,
}

impl Expression for IfStatement {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let predicate = self.predicate.resolve(ctx)?.try_boolean()?;

        match predicate {
            true => self.consequent.resolve(ctx),
            false => self
                .alternative
                .as_ref()
                .map_or(Ok(Value::Null), |block| block.resolve(ctx)),
        }
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let type_def = self.consequent.type_def(state);

        match &self.alternative {
            None => type_def.add_null(),
            Some(alternative) => type_def.merge_deep(alternative.type_def(state)),
        }
    }
}

impl fmt::Display for IfStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("if ")?;
        self.predicate.fmt(f)?;
        f.write_str(" ")?;
        self.consequent.fmt(f)?;

        if let Some(alt) = &self.alternative {
            f.write_str(" else")?;
            alt.fmt(f)?;
        }

        Ok(())
    }
}
