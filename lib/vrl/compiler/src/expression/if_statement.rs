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
                .map_or(Ok(Value::Null), |block| block.resolve(ctx)),
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
    ) -> Result<(), String> {
        let if_statement_begin_block = ctx.append_basic_block("if_statement_begin");
        let if_statement_end_block = ctx.append_basic_block("if_statement_end");
        let if_branch_block = ctx.append_basic_block("if_statement_if_branch");
        let else_branch_block = ctx.append_basic_block("if_statement_else_branch");

        ctx.build_unconditional_branch(if_statement_begin_block);
        ctx.position_at_end(if_statement_begin_block);

        let result_ref = ctx.result_ref();

        let predicate_ref = ctx.build_alloca_resolved_initialized("predicate");

        ctx.emit_llvm(
            &self.predicate,
            predicate_ref,
            (state.0, state.1),
            if_statement_end_block,
            vec![(predicate_ref.into(), ctx.fns().vrl_resolved_drop)],
        )?;

        let is_true = ctx
            .fns()
            .vrl_value_boolean_is_true
            .build_call(ctx.builder(), predicate_ref)
            .try_as_basic_value()
            .left()
            .expect("result is not a basic value")
            .try_into()
            .expect("result is not an int value");

        ctx.fns()
            .vrl_resolved_drop
            .build_call(ctx.builder(), predicate_ref);

        ctx.build_conditional_branch(is_true, if_branch_block, else_branch_block);

        ctx.position_at_end(if_branch_block);
        ctx.emit_llvm(
            &self.consequent,
            result_ref,
            (state.0, state.1),
            if_statement_end_block,
            vec![],
        )?;
        ctx.build_unconditional_branch(if_statement_end_block);

        ctx.position_at_end(else_branch_block);
        if let Some(alternative) = &self.alternative {
            ctx.emit_llvm(
                alternative,
                result_ref,
                (state.0, state.1),
                if_statement_end_block,
                vec![],
            )?;
        } else {
            ctx.fns()
                .vrl_resolved_ok_null
                .build_call(ctx.builder(), result_ref);
        }
        ctx.build_unconditional_branch(if_statement_end_block);

        ctx.position_at_end(if_statement_end_block);

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
