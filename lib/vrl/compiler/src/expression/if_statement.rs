use std::fmt;

use value::Value;

use crate::{
    expression::{Block, Predicate, Resolved},
    state::{ExternalEnv, LocalEnv},
    value::VrlValueConvert,
    BatchContext, Context, Expression, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub struct IfStatement {
    predicate: Predicate,
    consequent: Block,
    alternative: Option<Block>,
    selection_vector_ok: Vec<usize>,
    selection_vector_if: Vec<usize>,
    selection_vector_else: Vec<usize>,
}

impl IfStatement {
    #[must_use]
    pub fn new(predicate: Predicate, consequent: Block, alternative: Option<Block>) -> Self {
        Self {
            predicate,
            consequent,
            alternative,
            selection_vector_ok: vec![],
            selection_vector_if: vec![],
            selection_vector_else: vec![],
        }
    }
}

impl Expression for IfStatement {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let predicate = self
            .predicate
            .resolve(ctx)?
            .try_boolean()
            .expect("predicate must be boolean");

        match predicate {
            true => self.consequent.resolve(ctx),
            false => self
                .alternative
                .as_ref()
                .map_or(Ok(Value::Null), |block| block.resolve(ctx)),
        }
    }

    fn resolve_batch(&mut self, ctx: &mut BatchContext, selection_vector: &[usize]) {
        self.predicate.resolve_batch(ctx, selection_vector);

        self.selection_vector_ok.truncate(0);

        for index in selection_vector {
            let index = *index;
            if ctx.resolved_values[index].is_ok() {
                self.selection_vector_ok.push(index);
            }
        }

        self.selection_vector_if.truncate(0);
        self.selection_vector_else.truncate(0);

        for index in &self.selection_vector_ok {
            let index = *index;
            let predicate = match ctx.resolved_values.get(index) {
                Some(Ok(Value::Boolean(predicate))) => *predicate,
                _ => unreachable!("predicate has been checked for error and must be boolean"),
            };

            if predicate {
                self.selection_vector_if.push(index);
            } else {
                self.selection_vector_else.push(index);
            }
        }

        self.consequent
            .resolve_batch(ctx, &self.selection_vector_if);
        if let Some(alternative) = &mut self.alternative {
            alternative.resolve_batch(ctx, &self.selection_vector_else);
        } else {
            for index in &self.selection_vector_else {
                ctx.resolved_values[*index] = Ok(Value::Null);
            }
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
        state: (&LocalEnv, &ExternalEnv),
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
            state,
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
            state,
            if_statement_end_block,
            vec![],
        )?;
        ctx.build_unconditional_branch(if_statement_end_block);

        ctx.position_at_end(else_branch_block);
        if let Some(alternative) = &self.alternative {
            ctx.emit_llvm(
                alternative,
                result_ref,
                state,
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
