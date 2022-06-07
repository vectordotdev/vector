use std::fmt;

use value::Value;

use crate::{
    expression::Resolved,
    state::{ExternalEnv, LocalEnv},
    Context, Expression, TypeDef,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Noop;

impl Expression for Noop {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Ok(Value::Null)
    }

    fn type_def(&self, _: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        TypeDef::null().infallible()
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        _: (&mut LocalEnv, &mut ExternalEnv),
        _: &mut crate::llvm::Context<'ctx>,
        _: &mut Vec<crate::llvm::BasicBlock<'ctx>>,
    ) -> Result<(), String> {
        Ok(())
    }
}

impl fmt::Display for Noop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("null")
    }
}
