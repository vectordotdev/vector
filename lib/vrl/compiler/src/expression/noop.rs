//! The [`Noop`] expression.
//!
//! This expression is equivalent to `Literal::Null`, but is used internally to
//! indicate that the actual return value doesn't matter.

use std::fmt;

use value::Value;

use crate::{
    expression::Resolved,
    state::{ExternalEnv, LocalEnv},
    Context, Expression, TypeDef,
};

/// The [`Noop`] expression.
///
/// See module-level documentation for more details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Noop;

impl Expression for Noop {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Ok(Value::Null)
    }

    fn type_def(&self, _: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        TypeDef::null().infallible()
    }
}

impl fmt::Display for Noop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("null")
    }
}
