use std::fmt;

use diagnostic::{DiagnosticMessage, Label, Note, Urls};
use parser::ast::Node;
use value::Value;

use crate::{
    expression::{Expr, ExpressionError, Resolved},
    state::{ExternalEnv, LocalEnv},
    value::{Kind, VrlValueConvert},
    BatchContext, Context, Expression, Span, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Abort {
    span: Span,
    message: Option<Box<Expr>>,
    messages: Vec<Option<String>>,
}

impl Abort {
    /// # Errors
    ///
    /// * The optional message is fallible.
    /// * The optional message does not resolve to a string.
    pub fn new(
        span: Span,
        message: Option<Node<Expr>>,
        state: (&LocalEnv, &ExternalEnv),
    ) -> Result<Self, Error> {
        let message = message
            .map(|node| {
                let (expr_span, expr) = node.take();
                let type_def = expr.type_def(state);

                if type_def.is_fallible() {
                    Err(Error {
                        variant: ErrorVariant::FallibleExpr,
                        expr_span,
                    })
                } else if !type_def.is_bytes() {
                    Err(Error {
                        variant: ErrorVariant::NonString(type_def.into()),
                        expr_span,
                    })
                } else {
                    Ok(Box::new(expr))
                }
            })
            .transpose()?;

        Ok(Self {
            span,
            message,
            messages: vec![],
        })
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

    fn resolve_batch(&mut self, ctx: &mut BatchContext, selection_vector: &[usize]) {
        self.messages.resize(selection_vector.len(), None);

        if let Some(expr) = &mut self.message {
            expr.resolve_batch(ctx, selection_vector);

            for index in selection_vector {
                let index = *index;
                let resolved = &mut ctx.resolved_values[index];
                let resolved = {
                    let mut moved = Ok(Value::Null);
                    std::mem::swap(resolved, &mut moved);
                    moved
                };

                self.messages[index] = (|| -> Result<_, ExpressionError> {
                    Ok(Some(resolved?.try_bytes_utf8_lossy()?.to_string()))
                })()
                .unwrap_or(None);
            }
        }

        for index in selection_vector {
            let index = *index;
            let message = self.messages[index].take();

            ctx.resolved_values[index] = Err(ExpressionError::Abort {
                span: self.span,
                message,
            });
        }
    }

    fn type_def(&self, _: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        TypeDef::never().infallible().abortable()
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&LocalEnv, &ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        let abort_begin_block = ctx.append_basic_block("abort_begin");
        let abort_end_block = ctx.append_basic_block("abort_end");

        ctx.build_unconditional_branch(abort_begin_block);
        ctx.position_at_end(abort_begin_block);

        let span_name = format!("{:?}", self.span);
        let span_ref = ctx.into_const(self.span, &span_name).as_pointer_value();

        let message_ref = ctx.build_alloca_resolved_initialized("message");

        if let Some(message) = &self.message {
            ctx.emit_llvm(
                message.as_ref(),
                message_ref,
                state,
                abort_end_block,
                vec![(message_ref.into(), ctx.fns().vrl_resolved_drop)],
            )?;
        }

        ctx.fns().vrl_expression_abort.build_call(
            ctx.builder(),
            ctx.cast_span_ref_type(span_ref),
            message_ref,
            ctx.result_ref(),
        );

        ctx.build_unconditional_branch(abort_end_block);
        ctx.position_at_end(abort_end_block);

        Ok(())
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
pub(crate) enum ErrorVariant {
    #[error("unhandled fallible expression")]
    FallibleExpr,
    #[error("non-string abort message")]
    NonString(Kind),
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

impl DiagnosticMessage for Error {
    fn code(&self) -> usize {
        use ErrorVariant::{FallibleExpr, NonString};

        match self.variant {
            FallibleExpr => 631,
            NonString(_) => 300,
        }
    }

    fn labels(&self) -> Vec<Label> {
        match &self.variant {
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
            ErrorVariant::NonString(kind) => vec![
                Label::primary(
                    "abort only accepts an expression argument resolving to a string",
                    self.expr_span,
                ),
                Label::context(
                    format!("this expression resolves to {}", kind),
                    self.expr_span,
                ),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        match self.variant {
            ErrorVariant::FallibleExpr => vec![Note::SeeErrorDocs],
            ErrorVariant::NonString(_) => vec![
                Note::CoerceValue,
                Note::SeeDocs(
                    "type coercion".to_owned(),
                    Urls::func_docs("#coerce-functions"),
                ),
            ],
        }
    }
}
