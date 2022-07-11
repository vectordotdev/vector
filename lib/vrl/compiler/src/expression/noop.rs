use std::fmt;

use value::Value;

use crate::{
    expression::Resolved,
    state::{ExternalEnv, LocalEnv},
    BatchContext, Context, Expression, TypeDef,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Noop;

impl Expression for Noop {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Ok(Value::Null)
    }

    fn resolve_batch(&mut self, ctx: &mut BatchContext, selection_vector: &[usize]) {
        for index in selection_vector {
            ctx.resolved_values[*index] = Ok(Value::Null);
        }
    }

    fn type_def(&self, _: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        TypeDef::null().infallible()
    }
}

impl fmt::Display for Noop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("null")
    }
}
