use std::fmt;

use crate::{
    expression::{Expr, Resolved},
    state::{ExternalEnv, LocalEnv},
    Context, Expression, TypeDef,
};

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

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        self.inner.type_def(state)
    }
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, r#"({})"#, self.inner)
    }
}
