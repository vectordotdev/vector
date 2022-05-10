use std::fmt;

use crate::{
    expression::{Expr, Resolved},
    state::{ExternalEnv, LocalEnv},
    vm::OpCode,
    Context, Expression, TypeDef, Value,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    inner: Vec<Expr>,

    /// The local environment of the block.
    ///
    /// This allows any expressions within the block to mutate the local
    /// environment, but once the block ends, the environment is reset to the
    /// state of the parent expression of the block.
    local_env: LocalEnv,
}

impl Block {
    pub fn new(inner: Vec<Expr>, local_env: LocalEnv) -> Self {
        Self { inner, local_env }
    }

    pub fn into_inner(self) -> Vec<Expr> {
        self.inner
    }
}

impl Expression for Block {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        // NOTE:
        //
        // Technically, this invalidates the scoping invariant of variables
        // defined in child scopes to not be accessible in parrent scopes.
        //
        // However, because we guard against this (using the "undefined
        // variable" check) at compile-time, we can omit any (costly) run-time
        // operations to track/restore variables across scopes.
        //
        // This also means we don't need to make any changes to the VM runtime,
        // as it uses the same compiler as this AST runtime.
        let (last, other) = self.inner.split_last().expect("at least one expression");

        other
            .iter()
            .try_for_each(|expr| expr.resolve(ctx).map(|_| ()))?;

        last.resolve(ctx)
    }

    fn type_def(&self, (_, external): (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let mut type_defs = self
            .inner
            .iter()
            .map(|expr| expr.type_def((&self.local_env, external)))
            .collect::<Vec<_>>();

        // If any of the stored expressions is fallible, the entire block is
        // fallible.
        let fallible = type_defs.iter().any(TypeDef::is_fallible);

        // The last expression determines the resulting value of the block.
        let type_def = type_defs.pop().unwrap_or_else(TypeDef::null);

        type_def.with_fallibility(fallible)
    }

    fn compile_to_vm(
        &self,
        vm: &mut crate::vm::Vm,
        state: (&mut LocalEnv, &mut ExternalEnv),
    ) -> Result<(), String> {
        let (local, external) = state;
        let mut jumps = Vec::new();

        // An empty block should resolve to Null.
        if self.inner.is_empty() {
            let null = vm.add_constant(Value::Null);
            vm.write_opcode(OpCode::Constant);
            vm.write_primitive(null);
        }

        let mut expressions = self.inner.iter().peekable();

        while let Some(expr) = expressions.next() {
            // Write each of the inner expressions
            expr.compile_to_vm(vm, (local, external))?;

            if expressions.peek().is_some() {
                // At the end of each statement (apart from the last one) we need to clean up
                // This involves popping the value remaining on the stack, and jumping to the end
                // of the block if we are in error.
                jumps.push(vm.emit_jump(OpCode::EndStatement));
            }
        }

        // Update all the jumps to jump to the end of the block.
        for jump in jumps {
            vm.patch_jump(jump);
        }

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
