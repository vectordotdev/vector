use crate::expression::{Expr, Resolved};
use crate::{Context, Expression, State, TypeDef};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    inner: Box<Expr>,
}

impl Group {
    pub fn new(inner: Expr) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }
}

impl Expression for Group {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.inner.resolve(ctx)
    }

    fn type_def(&self, state: &State) -> TypeDef {
        self.inner.type_def(state)
    }

    fn dump(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        self.inner.dump(vm)
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(&self, ctx: &mut crate::llvm::Context<'ctx>) -> Result<(), String> {
        self.inner.emit_llvm(ctx)
    }
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, r#"({})"#, self.inner)
    }
}
