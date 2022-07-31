use std::fmt;

use crate::{
    expression::{Not, Resolved},
    state::{ExternalEnv, LocalEnv},
    BatchContext, Context, Expression, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Unary {
    variant: Variant,
}

impl Unary {
    #[must_use]
    pub fn new(variant: Variant) -> Self {
        Self { variant }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Variant {
    Not(Not),
}

impl Expression for Unary {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Variant::Not;

        match &self.variant {
            Not(v) => v.resolve(ctx),
        }
    }

    fn resolve_batch(&mut self, ctx: &mut BatchContext, selection_vector: &[usize]) {
        use Variant::Not;

        match &mut self.variant {
            Not(v) => v.resolve_batch(ctx, selection_vector),
        }
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        use Variant::Not;

        match &self.variant {
            Not(v) => v.type_def(state),
        }
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&mut LocalEnv, &mut ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        use Variant::Not;

        match &self.variant {
            Not(v) => v.emit_llvm(state, ctx),
        }
    }
}

impl fmt::Display for Unary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Variant::Not;

        match &self.variant {
            Not(v) => v.fmt(f),
        }
    }
}

impl From<Not> for Variant {
    fn from(not: Not) -> Self {
        Variant::Not(not)
    }
}
