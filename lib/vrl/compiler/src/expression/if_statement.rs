use std::fmt;

use value::Value;

use crate::{
    expression::{Block, Expr, Noop, Predicate, Resolved},
    state::{ExternalEnv, LocalEnv},
    value::VrlValueConvert,
    vm::OpCode,
    Context, Expression, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub struct IfStatement {
    pub predicate: Predicate,
    pub consequent: Block,
    pub alternative: Option<Block>,
}

impl IfStatement {
    pub(crate) fn noop() -> Self {
        let predicate = Predicate::new_unchecked(vec![]);

        let consequent = Block::new(vec![Expr::Noop(Noop)], LocalEnv::default());

        Self {
            predicate,
            consequent,
            alternative: None,
        }
    }
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
            None => type_def,
            Some(alternative) => type_def.merge_deep(alternative.type_def(state)),
        }
    }

    fn compile_to_vm(
        &self,
        vm: &mut crate::vm::Vm,
        state: (&mut LocalEnv, &mut ExternalEnv),
    ) -> Result<(), String> {
        let (local, external) = state;

        // Write the predicate which will leave the result on the stack.
        self.predicate.compile_to_vm(vm, (local, external))?;

        // If the value is false, we want to jump to the alternative block.
        // We need to store this jump as it will need updating when we know where
        // the alternative block actually starts.
        let else_jump = vm.emit_jump(OpCode::JumpIfFalse);
        vm.write_opcode(OpCode::Pop);

        // Write the consequent block.
        self.consequent.compile_to_vm(vm, (local, external))?;

        // After the consequent block we want to jump over the alternative.
        let continue_jump = vm.emit_jump(OpCode::Jump);

        // Update the initial if jump to jump to the current position.
        vm.patch_jump(else_jump);
        vm.write_opcode(OpCode::Pop);

        if let Some(alternative) = &self.alternative {
            // Write the alternative block.
            alternative.compile_to_vm(vm, (local, external))?;
        } else {
            // No alternative resolves to Null.
            let null = vm.add_constant(Value::Null);
            vm.write_opcode(OpCode::Constant);
            vm.write_primitive(null);
        }

        // Update the continue jump to jump to the current position after the else block.
        vm.patch_jump(continue_jump);

        Ok(())
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&mut LocalEnv, &mut ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
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
        {
            let fn_ident = "vrl_resolved_initialize";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
            ctx.builder()
                .build_call(fn_impl, &[predicate_ref.into()], fn_ident);
        }

        ctx.set_result_ref(predicate_ref);
        self.predicate.emit_llvm((state.0, state.1), ctx)?;
        ctx.set_result_ref(result_ref);

        let is_true = {
            let fn_ident = "vrl_value_boolean_is_true";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
            ctx.builder()
                .build_call(fn_impl, &[predicate_ref.into()], fn_ident)
                .try_as_basic_value()
                .left()
                .ok_or(format!(r#"result of "{}" is not a basic value"#, fn_ident))?
                .try_into()
                .map_err(|_| format!(r#"result of "{}" is not an int value"#, fn_ident))?
        };

        {
            let fn_ident = "vrl_resolved_drop";
            let fn_impl = ctx
                .module()
                .get_function(fn_ident)
                .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
            ctx.builder()
                .build_call(fn_impl, &[predicate_ref.into()], fn_ident);
        }

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
        self.consequent.emit_llvm((state.0, state.1), ctx)?;
        ctx.builder().build_unconditional_branch(end_block);

        ctx.builder().position_at_end(else_branch_block);
        if let Some(alternative) = &self.alternative {
            alternative.emit_llvm((state.0, state.1), ctx)?;
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
