use std::fmt;

use diagnostic::{DiagnosticMessage, Label, Note, Urls};

use crate::value::VrlValueConvert;
use crate::{
    expression::{Expr, Noop, Resolved},
    parser::Node,
    state::{ExternalEnv, LocalEnv},
    value::Kind,
    vm::OpCode,
    Context, Expression, Span, TypeDef,
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

    pub fn noop() -> Self {
        Not {
            inner: Box::new(Noop.into()),
        }
    }
}

impl Expression for Not {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        Ok((!self.inner.resolve(ctx)?.try_boolean()?).into())
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let fallible = self.inner.type_def(state).is_fallible();

        TypeDef::boolean().with_fallibility(fallible)
    }

    fn compile_to_vm(
        &self,
        vm: &mut crate::vm::Vm,
        state: (&mut LocalEnv, &mut ExternalEnv),
    ) -> std::result::Result<(), String> {
        self.inner.compile_to_vm(vm, state)?;
        vm.write_opcode(OpCode::Not);

        Ok(())
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&mut LocalEnv, &mut ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> std::result::Result<(), String> {
        let function = ctx.function();
        let not_begin_block = ctx.context().append_basic_block(function, "not_begin");
        ctx.builder().build_unconditional_branch(not_begin_block);
        ctx.builder().position_at_end(not_begin_block);

        self.inner.emit_llvm((state.0, state.1), ctx)?;

        let not_end_block = ctx.context().append_basic_block(function, "not_end");

        let type_def = self.inner.type_def((state.0, state.1));
        if type_def.is_fallible() || type_def.is_abortable() {
            let is_err = ctx
                .vrl_resolved_is_err()
                .build_call(ctx.builder(), ctx.result_ref())
                .try_as_basic_value()
                .left()
                .expect("result is not a basic value")
                .try_into()
                .expect("result is not an int value");

            let not_is_ok_block = ctx.context().append_basic_block(function, "not_is_ok");

            ctx.builder()
                .build_conditional_branch(is_err, not_end_block, not_is_ok_block);

            ctx.builder().position_at_end(not_is_ok_block);
        }

        ctx.vrl_expression_not()
            .build_call(ctx.builder(), ctx.result_ref());

        ctx.builder().build_unconditional_branch(not_end_block);

        ctx.builder().position_at_end(not_end_block);

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
        use ErrorVariant::*;

        match &self.variant {
            NonBoolean(..) => 660,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use ErrorVariant::*;

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
        use ErrorVariant::*;

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
