use crate::expression::{ExpressionError, Resolved};
use crate::{Context, Expression, State, TypeDef};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Abort;

impl Expression for Abort {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Err(ExpressionError::Abort)
    }

    fn type_def(&self, _: &State) -> TypeDef {
        TypeDef::new().infallible().null()
    }
}

impl fmt::Display for Abort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "abort")
    }
}
