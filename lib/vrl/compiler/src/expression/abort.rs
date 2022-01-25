use std::fmt;

use diagnostic::DiagnosticError;

use crate::{
    expression::{ExpressionError, Resolved},
    Context, Expression, Span, State, TypeDef,
};

use super::Expr;

#[derive(Debug, Clone, PartialEq)]
pub struct Abort {
    span: Span,
    message: Option<Box<Expr>>,
}

impl Abort {
    pub fn new(span: Span, message: Option<Expr>, state: &State) -> Result<Self, Error> {
        if let Some(expr) = message {
            let type_def = expr.type_def(state);
            if type_def.is_fallible() {
                Err(Error {
                    variant: ErrorVariant::FallibleExpr,
                })
            } else if !type_def.is_bytes() {
                Err(Error {
                    variant: ErrorVariant::NonString,
                })
            } else {
                Ok(Self {
                    span,
                    message: Some(Box::new(expr)),
                })
            }
        } else {
            Ok(Self {
                span,
                message: None,
            })
        }
    }

    pub fn noop(span: Span) -> Self {
        Self {
            span,
            message: None,
        }
    }
}

impl Expression for Abort {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let message = if let Some(expr) = &self.message {
            Some(expr.resolve(ctx)?.try_bytes_utf8_lossy()?.to_string())
        } else {
            None
        };

        Err(ExpressionError::Abort {
            span: self.span,
            message,
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

// -----------------------------------------------------------------------------

#[derive(Debug)]
pub struct Error {
    variant: ErrorVariant,
}

#[derive(thiserror::Error, Debug)]
pub enum ErrorVariant {
    #[error("non-string message")]
    NonString,
    #[error("unhandled fallible expression")]
    FallibleExpr,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#}", self.variant)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.variant)
    }
}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use ErrorVariant::*;

        match self.variant {
            NonString => 300,
            FallibleExpr => 630,
        }
    }
}
