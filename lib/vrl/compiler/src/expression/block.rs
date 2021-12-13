use crate::expression::{Expr, Resolved};
use crate::{Context, Expression, State, TypeDef, Value};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    inner: Vec<Expr>,
}

impl Block {
    pub fn new(inner: Vec<Expr>) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> Vec<Expr> {
        self.inner
    }
}

impl Expression for Block {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.inner
            .iter()
            .map(|expr| expr.resolve(ctx))
            .collect::<Result<Vec<_>, _>>()
            .map(|mut v| v.pop().unwrap_or(Value::Null))
    }

    fn type_def(&self, state: &State) -> TypeDef {
        let mut type_defs = self
            .inner
            .iter()
            .map(|expr| expr.type_def(state))
            .collect::<Vec<_>>();

        // If any of the stored expressions is fallible, the entire block is
        // fallible.
        let fallible = type_defs.iter().any(TypeDef::is_fallible);

        // The last expression determines the resulting value of the block.
        let type_def = type_defs.pop().unwrap_or_else(|| TypeDef::new().null());

        type_def.with_fallibility(fallible)
    }

    fn dump(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        for expr in &self.inner {
            expr.dump(vm)?;
        }

        Ok(())
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(&self, ctx: &mut crate::llvm::Context<'ctx>) -> Result<(), String> {
        let function = ctx.function();
        let block_begin_block = ctx.context().append_basic_block(function, "block_begin");
        ctx.builder().build_unconditional_branch(block_begin_block);
        ctx.builder().position_at_end(block_begin_block);

        let block_end_block = ctx.context().append_basic_block(function, "block_end");
        let block_error_block = ctx.context().append_basic_block(function, "block_error");

        for expr in &self.inner {
            expr.emit_llvm(ctx)?;
            let is_err = {
                let fn_ident = "vrl_resolved_is_err";
                let fn_impl = ctx
                    .module()
                    .get_function(fn_ident)
                    .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                ctx.builder()
                    .build_call(fn_impl, &[ctx.result_ref().into()], fn_ident)
                    .try_as_basic_value()
                    .left()
                    .ok_or(format!(r#"result of "{}" is not a basic value"#, fn_ident))?
                    .try_into()
                    .map_err(|_| format!(r#"result of "{}" is not an int value"#, fn_ident))?
            };

            let block_next_block = ctx.context().append_basic_block(function, "block_next");
            ctx.builder()
                .build_conditional_branch(is_err, block_error_block, block_next_block);
            ctx.builder().position_at_end(block_next_block);
        }

        let block_next_block = ctx.builder().get_insert_block().unwrap();

        ctx.builder().position_at_end(block_error_block);
        ctx.builder().build_unconditional_branch(block_end_block);

        ctx.builder().position_at_end(block_next_block);
        ctx.builder().build_unconditional_branch(block_end_block);

        ctx.builder().position_at_end(block_end_block);

        Ok(())
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{\n")?;

        let mut iter = self.inner.iter().peekable();
        while let Some(expr) = iter.next() {
            f.write_str("\t")?;
            expr.fmt(f)?;
            if iter.peek().is_some() {
                f.write_str("\n")?;
            }
        }

        f.write_str("\n}")
    }
}
