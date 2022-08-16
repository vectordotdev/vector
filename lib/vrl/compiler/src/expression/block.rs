use std::fmt;

use crate::state::{TypeInfo, TypeState};
use crate::{
    expression::{Expr, Resolved},
    Context, Expression, TypeDef,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    inner: Vec<Expr>,

    // false - This is just an inline block of code
    // true - This is a block of code nested in a child scope
    new_scope: bool,
}

impl Block {
    #[must_use]
    fn new(inner: Vec<Expr>, new_scope: bool) -> Self {
        Self { inner, new_scope }
    }

    #[must_use]
    pub fn new_scoped(inner: Vec<Expr>) -> Self {
        Self::new(inner, true)
    }

    #[must_use]
    pub fn new_inline(inner: Vec<Expr>) -> Self {
        Self::new(inner, false)
    }

    #[must_use]
    pub fn into_inner(self) -> Vec<Expr> {
        self.inner
    }

    #[must_use]
    pub fn exprs(&self) -> &Vec<Expr> {
        &self.inner
    }
}

impl Expression for Block {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        // Variables are checked at compile-time to ensure only variables
        // in scope can be accessed here, so it doesn't need to be checked at runtime.
        let (last, other) = self.inner.split_last().expect("at least one expression");

        other
            .iter()
            .try_for_each(|expr| expr.resolve(ctx).map(|_| ()))?;

        last.resolve(ctx)
    }

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        let parent_locals = state.local.clone();

        let mut state = state.clone();
        let mut result = TypeDef::null();
        let mut fallible = false;

        for expr in &self.inner {
            result = expr.apply_type_info(&mut state);

            if result.is_fallible() {
                fallible = true;
            }
            if result.is_never() {
                break;
            }
        }

        if self.new_scope {
            state.local = parent_locals.apply_child_scope(state.local);
        }

        TypeInfo::new(state, result.with_fallibility(fallible))
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
