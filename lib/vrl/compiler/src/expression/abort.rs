use std::fmt;

use crate::{
    expression::{ExpressionError, Resolved},
    Context, Expression, Span, State, TypeDef,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Abort {
    span: Span,
}

impl Abort {
    pub fn new(span: Span) -> Abort {
        Abort { span }
    }
}

impl Expression for Abort {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Err(ExpressionError::Abort { span: self.span })
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
