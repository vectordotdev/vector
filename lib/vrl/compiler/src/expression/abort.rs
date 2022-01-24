use std::fmt;

use crate::{
    expression::{ExpressionError, Resolved},
    Context, Expression, Span, State, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Abort {
    span: Span,
    message: Option<String>,
}

impl Abort {
    pub fn new(span: Span, message: Option<String>) -> Abort {
        Abort { span, message }
    }
}

impl Expression for Abort {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Err(ExpressionError::Abort {
            span: self.span,
            message: self.message.clone(),
        })
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
