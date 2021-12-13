use std::fmt;

use diagnostic::{DiagnosticError, Label, Note, Urls};
use parser::ast::Node;

use crate::{
    expression::{ExpressionError, Resolved},
    state::{ExternalEnv, LocalEnv},
    value::Kind,
    value::VrlValueConvert,
    vm::OpCode,
    Context, Expression, Span, TypeDef, Value,
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

        Ok(Self { span, message })
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

    fn type_def(&self, _: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        TypeDef::null().infallible()
    }

    fn compile_to_vm(
        &self,
        vm: &mut crate::vm::Vm,
        state: (&mut LocalEnv, &mut ExternalEnv),
    ) -> Result<(), String> {
        match &self.message {
            None => {
                // If there is no message, just write a Null to the stack which
                // the abort instruction will use to know not to attach a message.
                let nullidx = vm.add_constant(Value::Null);
                vm.write_opcode(OpCode::Constant);
                vm.write_primitive(nullidx);
            }
            Some(message) => message.compile_to_vm(vm, state)?,
        }

        vm.write_opcode(OpCode::Abort);

        // The `Abort` `OpCode` needs the span of the expression to return in the abort error.
        vm.write_primitive(self.span.start());
        vm.write_primitive(self.span.end());
        Ok(())
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(&self, ctx: &mut crate::llvm::Context<'ctx>) -> Result<(), String> {
        let function = ctx.function();
        let abort_begin_block = ctx.context().append_basic_block(function, "abort_begin");
        ctx.builder().build_unconditional_branch(abort_begin_block);
        ctx.builder().position_at_end(abort_begin_block);

        let span_name = format!("{:?}", self.span);
        let span_ref = ctx.into_const(self.span, &span_name).as_pointer_value();

        let message_ref = ctx.build_alloca_resolved("message");

        {
            let fn_ident = "vrl_resolved_initialize";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;

            ctx.builder()
                .build_call(fn_impl, &[message_ref.into()], fn_ident);
        }

        {
            let fn_ident = "vrl_expression_abort_impl";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;

            if let Some(message) = &self.message {
                let result_ref = ctx.result_ref();
                ctx.set_result_ref(message_ref);
                message.emit_llvm(ctx)?;
                ctx.set_result_ref(result_ref);
            }

            ctx.builder().build_call(
                fn_impl,
                &[
                    ctx.builder()
                        .build_bitcast(
                            span_ref,
                            fn_impl
                                .get_nth_param(0)
                                .unwrap()
                                .get_type()
                                .into_pointer_type(),
                            "cast",
                        )
                        .into(),
                    message_ref.into(),
                    ctx.result_ref().into(),
                ],
                fn_ident,
            );
        }

        {
            let fn_ident = "vrl_resolved_drop";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;

            ctx.builder()
                .build_call(fn_impl, &[message_ref.into()], fn_ident);
        }

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

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use ErrorVariant::*;

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
