use crate::expression::{ExpressionError, Resolved};
use crate::{Context, Expression, Span, State, TypeDef};
use std::fmt;

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

    fn dump(&self, _vm: &mut crate::vm::Vm) -> Result<(), String> {
        todo!()
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(&self, ctx: &mut crate::llvm::Context<'ctx>) -> Result<(), String> {
        let function = ctx.function();
        let abort_begin_block = ctx.context().append_basic_block(function, "abort_begin");
        ctx.builder().build_unconditional_branch(abort_begin_block);
        ctx.builder().position_at_end(abort_begin_block);

        let fn_ident = "vrl_expression_abort_impl";
        let fn_impl = ctx
            .module()
            .get_function(fn_ident)
            .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
        let span_name = format!("{:?}", self.span);
        let span_ref = ctx.into_const(self.span, &span_name).as_pointer_value();
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
                ctx.result_ref().into(),
            ],
            fn_ident,
        );
        Ok(())
    }
}

impl fmt::Display for Abort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "abort")
    }
}
