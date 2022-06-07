use std::fmt;

use value::Value;

use crate::{
    expression::{Block, Predicate, Resolved},
    state::{ExternalEnv, LocalEnv},
    value::VrlValueConvert,
    Context, Expression, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub struct IfStatement {
    pub predicate: Predicate,
    pub consequent: Block,
    pub alternative: Option<Block>,
}

impl Expression for IfStatement {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let predicate = self.predicate.resolve(ctx)?.try_boolean()?;

        match predicate {
            true => self.consequent.resolve(ctx),
            false => self
                .alternative
                .as_ref()
                .map(|block| block.resolve(ctx))
                .unwrap_or(Ok(Value::Null)),
        }
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let type_def = self.consequent.type_def(state);

        match &self.alternative {
            None => type_def.add_null(),
            Some(alternative) => type_def.merge_deep(alternative.type_def(state)),
        }
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&mut LocalEnv, &mut ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
        function_call_abort_stack: &mut Vec<crate::llvm::BasicBlock<'ctx>>,
    ) -> Result<(), String> {
        let function = ctx.function();
        let if_statement_begin_block = ctx
            .context()
            .append_basic_block(function, "if_statement_begin");
        ctx.builder()
            .build_unconditional_branch(if_statement_begin_block);
        ctx.builder().position_at_end(if_statement_begin_block);

        let result_ref = ctx.result_ref();

        let predicate_ref = ctx.build_alloca_resolved("predicate");
        ctx.vrl_resolved_initialize()
            .build_call(ctx.builder(), predicate_ref);

        ctx.set_result_ref(predicate_ref);
        let mut abort_stack = Vec::new();
        self.predicate
            .emit_llvm((state.0, state.1), ctx, &mut abort_stack)?;
        function_call_abort_stack.extend(abort_stack);
        ctx.set_result_ref(result_ref);

        let is_true = ctx
            .vrl_value_boolean_is_true()
            .build_call(ctx.builder(), predicate_ref)
            .try_as_basic_value()
            .left()
            .expect("result is not a basic value")
            .try_into()
            .expect("result is not an int value");

        ctx.vrl_resolved_drop()
            .build_call(ctx.builder(), predicate_ref);

        let end_block = ctx
            .context()
            .append_basic_block(function, "if_statement_end");

        let if_branch_block = ctx
            .context()
            .append_basic_block(function, "if_statement_if_branch");
        let else_branch_block = ctx
            .context()
            .append_basic_block(function, "if_statement_else_branch");

        ctx.builder()
            .build_conditional_branch(is_true, if_branch_block, else_branch_block);

        ctx.builder().position_at_end(if_branch_block);
        let mut abort_stack = Vec::new();
        self.consequent
            .emit_llvm((state.0, state.1), ctx, &mut abort_stack)?;
        function_call_abort_stack.extend(abort_stack);
        ctx.builder().build_unconditional_branch(end_block);

        ctx.builder().position_at_end(else_branch_block);
        if let Some(alternative) = &self.alternative {
            let mut abort_stack = Vec::new();
            alternative.emit_llvm((state.0, state.1), ctx, &mut abort_stack)?;
            function_call_abort_stack.extend(abort_stack);
        } else {
            ctx.vrl_resolved_set_null()
                .build_call(ctx.builder(), result_ref);
        }
        ctx.builder().build_unconditional_branch(end_block);

        ctx.builder().position_at_end(end_block);

        Ok(())
    }
}

impl fmt::Display for IfStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("if ")?;
        self.predicate.fmt(f)?;
        f.write_str(" ")?;
        self.consequent.fmt(f)?;

        if let Some(alt) = &self.alternative {
            f.write_str(" else")?;
            alt.fmt(f)?;
        }

        Ok(())
    }
}
