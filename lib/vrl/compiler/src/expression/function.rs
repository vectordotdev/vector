use crate::state::{TypeInfo, TypeState};
use crate::{Context, Expression, TypeDef};
use core::Resolved;
use dyn_clone::DynClone;
use std::fmt;
use std::fmt::Debug;
use value::Value;

/// A trait similar to `Expression`, but simplified specifically for functions.
/// The main difference is this trait prevents mutation of variables both at runtime
/// and compile time.
pub trait FunctionExpression: Send + Sync + fmt::Debug + DynClone + Clone + 'static {
    /// Resolve an expression to a concrete [`Value`].
    /// This method is executed at runtime.
    /// An expression is allowed to fail, which aborts the running program.
    // This should be a read-only reference to `Context`, but function args
    // are resolved in the function themselves, which can theoretically mutate
    // see: https://github.com/vectordotdev/vector/issues/13752
    fn resolve(&self, ctx: &mut Context) -> Resolved;

    /// The resulting type that the function resolves to.
    fn type_def(&self, state: &TypeState) -> TypeDef;

    /// Resolves values at compile-time for constant functions.
    ///
    /// This returns `Some` for constant expressions, or `None` otherwise.
    fn as_value(&self) -> Option<Value> {
        None
    }

    /// Converts this function to a normal `Expression`.
    fn as_expr(&self) -> Box<dyn Expression> {
        Box::new(FunctionExpressionAdapter {
            inner: self.clone(),
        })
    }
}

#[derive(Debug, Clone)]
struct FunctionExpressionAdapter<T> {
    inner: T,
}

impl<T: FunctionExpression + Debug + Clone> Expression for FunctionExpressionAdapter<T> {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.inner.resolve(ctx)
    }

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        let result = self.inner.type_def(state);
        TypeInfo::new(state, result)
    }
}
