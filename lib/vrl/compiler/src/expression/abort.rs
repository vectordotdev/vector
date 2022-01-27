use std::fmt;

use diagnostic::{DiagnosticError, Label, Note};
use parser::ast::Node;

use crate::{
    expression::{ExpressionError, Resolved},
    value::{self, Kind},
    Context, Expression, Span, State, TypeDef,
};

use super::Expr;

#[derive(Debug, Clone, PartialEq)]
pub struct Abort {
    span: Span,
    message: Option<Box<Expr>>,
}

impl Abort {
    pub fn new(
        span: Span,
        message: Option<Node<Expr>>,
        state: &State,
    ) -> Result<Self, Box<dyn DiagnosticError>> {
        if let Some(node) = message {
            let (expr_span, expr) = node.take();
            let type_def = expr.type_def(state);

            if type_def.is_fallible() {
                Err(Box::new(Error {
                    variant: ErrorVariant::FallibleExpr,
                    expr_span,
                }))
            } else if !type_def.is_bytes() {
                Err(Box::new(value::Error::Expected {
                    got: type_def.kind(),
                    expected: Kind::Bytes,
                }))
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
        let message = self
            .message
            .as_ref()
            .map::<Result<_, ExpressionError>, _>(|expr| {
                Ok(expr.resolve(ctx)?.try_bytes_utf8_lossy()?.to_string())
            })
            .transpose()?;

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
    expr_span: Span,
}

#[derive(thiserror::Error, Debug)]
pub enum ErrorVariant {
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
            FallibleExpr => 630,
        }
    }

    fn labels(&self) -> Vec<Label> {
        match self.variant {
            ErrorVariant::FallibleExpr => vec![
                Label::primary(
                    "abort only accepts an infallible expression argument",
                    self.expr_span,
                ),
                Label::context(
                    "handle errors before using the expression as an abort message",
                    self.expr_span,
                ),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        match self.variant {
            ErrorVariant::FallibleExpr => vec![Note::SeeErrorDocs],
        }
    }
}
