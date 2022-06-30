//! The [`Block`] expression.
//!
//! A block is a grouped set of expressions, for which the local scope lives as
//! long as the block itself.
//!
//! As an example:
//!
//! ```coffee
//! { "foo"; "bar" }
//! ```
//!
//! This block (delimited by `{` and `}`) contains two literal string
//! expressions.
//!
//! A block **cannot** be empty (`{}`), as that syntax is used to create empty
//! literal objects.
//!
//! A block can also contain dynamic values, and can manipulate the state of the
//! program at runtime:
//!
//! ```coffee
//! foo = true
//!
//! {
//!     foo
//!     bar = false
//!     foo = false
//! }
//!
//! bar
//! foo
//! ```
//!
//! In this example, the `foo` variable is read from within the block, which
//! works as expected. However, reading `bar` outside the block triggers
//! a compiler error instead of returning `false`, because `bar` was defined
//! within the local scope of the block, and that scope is gone after the block
//! ends.
//!
//! Mutating `foo` is allowed, and persists across the block boundary, as that
//! variable is originally defined outside of the block.

use std::fmt;

use crate::{
    expression::{Expr, Resolved},
    state::{ExternalEnv, LocalEnv},
    Context, Expression, TypeDef,
};

/// The [`Block`] expression.
///
/// See module-level documentation for more details.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    inner: Vec<Expr>,

    /// The local environment of the block.
    ///
    /// This allows any expressions within the block to mutate the local
    /// environment, but once the block ends, the environment is reset to the
    /// state of the parent expression of the block.
    pub(crate) local_env: LocalEnv,
}

impl Block {
    /// Create a new [`Block`] expression.
    ///
    /// The [`LocalEnv`] is used to pass along to child expressions, to ensure
    /// they use the same scope as the block itself.
    #[must_use]
    pub fn new(inner: Vec<Expr>, local_env: LocalEnv) -> Self {
        Self { inner, local_env }
    }

    /// Get the list of expressions stored within the block.
    #[must_use]
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

    /// If an expression has a "never" type, it is considered a "terminating" expression.
    /// Type information of future expressions in this block should not be considered after
    /// a terminating expression.
    ///
    /// Since type definitions due to assignments are calculated outside of the "`type_def`" function,
    /// assignments that can never execute might still have adjusted the type definition.
    /// Therefore, expressions after a terminating expression must not be included in a block.
    /// It is considered an internal compiler error if this situation occurs, which is checked here
    /// and will result in a panic.
    ///
    /// VRL is allowed to have expressions after a terminating expression, but the compiler
    /// MUST not include them in a block expression when compiled.
    fn type_def(&self, (_, external): (&LocalEnv, &ExternalEnv)) -> TypeDef {
        let mut last = TypeDef::null();
        let mut fallible = false;
        let mut has_terminated = false;
        for expr in &self.inner {
            assert!(!has_terminated, "VRL block contains an expression after a terminating expression. This is an internal compiler error. Please submit a bug report.");
            last = expr.type_def((&self.local_env, external));
            if last.is_never() {
                has_terminated = true;
            }
            if last.is_fallible() {
                fallible = true;
            }
        }

        last.with_fallibility(fallible)
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
