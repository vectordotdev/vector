use crate::expression::{ExpressionError, Resolved};
use crate::{Context, Expression, State, TypeDef};
use std::fmt;

#[derive(Debug, Clone)]
pub struct InternalExpression {
    pub expr: Box<dyn Expression>,
}

impl Expression for InternalExpression {
    fn resolve(&self, context: &mut Context) -> Resolved {
        self.expr.resolve(context)
    }

    fn type_def(&self, state: &State) -> TypeDef {
        self.expr.type_def(state)
    }
}

impl fmt::Display for InternalExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "internal expression")
    }
}

impl PartialEq for InternalExpression {
    fn eq(&self, other: &Self) -> bool {
        todo!()
    }
}
