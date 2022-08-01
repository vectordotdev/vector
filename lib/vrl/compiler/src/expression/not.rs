use std::{fmt, ptr::addr_of_mut};

use diagnostic::{DiagnosticMessage, Label, Note, Urls};

use crate::{
    expression::{Expr, Resolved},
    parser::Node,
    state::{ExternalEnv, LocalEnv},
    value::{Kind, VrlValueConvert},
    BatchContext, Context, Expression, Span, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Not {
    inner: Box<Expr>,
}

impl Not {
    pub fn new(
        node: Node<Expr>,
        not_span: Span,
        state: (&LocalEnv, &ExternalEnv),
    ) -> Result<Not, Error> {
        let (expr_span, expr) = node.take();
        let type_def = expr.type_def(state);

        if !type_def.is_boolean() {
            return Err(Error {
                variant: ErrorVariant::NonBoolean(type_def.into()),
                not_span,
                expr_span,
            });
        }

        Ok(Self {
            inner: Box::new(expr),
        })
    }
}

impl Expression for Not {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        Ok((!self.inner.resolve(ctx)?.try_boolean()?).into())
    }

    fn resolve_batch(&mut self, ctx: &mut BatchContext, selection_vector: &[usize]) {
        self.inner.resolve_batch(ctx, selection_vector);

        for index in selection_vector {
            let resolved = addr_of_mut!(ctx.resolved_values[*index]);
            let result = (|| Ok((!unsafe { resolved.read() }?.try_boolean()?).into()))();
            unsafe { resolved.write(result) };
        }
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let type_def = self.inner.type_def(state);
        let fallible = type_def.is_fallible();
        let abortable = type_def.is_abortable();

        TypeDef::boolean()
            .with_fallibility(fallible)
            .with_abortability(abortable)
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&LocalEnv, &ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> std::result::Result<(), String> {
        let not_begin_block = ctx.append_basic_block("not_begin");
        let not_end_block = ctx.append_basic_block("not_end");

        ctx.build_unconditional_branch(not_begin_block);
        ctx.position_at_end(not_begin_block);

        ctx.emit_llvm(
            self.inner.as_ref(),
            ctx.result_ref(),
            state,
            not_end_block,
            vec![],
        )?;

        let type_def = self.inner.type_def(state);
        if type_def.is_fallible() {
            let not_is_ok_block = ctx.append_basic_block("not_is_ok");

            let is_err = ctx
                .fns()
                .vrl_resolved_is_err
                .build_call(ctx.builder(), ctx.result_ref())
                .try_as_basic_value()
                .left()
                .expect("result is not a basic value")
                .try_into()
                .expect("result is not an int value");

            ctx.build_conditional_branch(is_err, not_end_block, not_is_ok_block);

            ctx.position_at_end(not_is_ok_block);
        }

        ctx.fns()
            .vrl_expression_not
            .build_call(ctx.builder(), ctx.result_ref());

        ctx.build_unconditional_branch(not_end_block);

        ctx.position_at_end(not_end_block);

        Ok(())
    }
}

impl fmt::Display for Not {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, r#"!{}"#, self.inner)
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug)]
pub struct Error {
    pub(crate) variant: ErrorVariant,

    not_span: Span,
    expr_span: Span,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ErrorVariant {
    #[error("non-boolean negation")]
    NonBoolean(Kind),
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
        use ErrorVariant::NonBoolean;

        match &self.variant {
            NonBoolean(..) => 660,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use ErrorVariant::NonBoolean;

        match &self.variant {
            NonBoolean(kind) => vec![
                Label::primary("negation only works on boolean values", self.not_span),
                Label::context(
                    format!("this expression resolves to {}", kind),
                    self.expr_span,
                ),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use ErrorVariant::NonBoolean;

        match &self.variant {
            NonBoolean(..) => {
                vec![
                    Note::CoerceValue,
                    Note::SeeDocs(
                        "type coercion".to_owned(),
                        Urls::func_docs("#coerce-functions"),
                    ),
                ]
            }
        }
    }
}
