use std::fmt;

use crate::{
    expression::{Expr, Resolved},
    vm::OpCode,
    Context, Expression, State, TypeDef, Value,
};

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

    fn compile_to_vm(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        let mut jumps = Vec::new();

        for expr in &self.inner {
            // Write each of the inner expressions
            expr.compile_to_vm(vm)?;

            // If there is an error, we need to jump to the end of the block.
            jumps.push(vm.emit_jump(OpCode::JumpIfErr));
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
