use crate::expression::{assignment, Resolved};
use crate::parser::ast::Ident;
use crate::{Context, Expression, State, TypeDef, Value};
use std::fmt;

#[derive(Debug, PartialEq)]
pub struct Variable {
    ident: Ident,
}

impl Variable {
    // TODO:
    //
    // - Error if variable has not been assigned yet.
    pub(crate) fn new(ident: Ident) -> Self {
        Self { ident }
    }
}

impl Expression for Variable {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        Ok(ctx
            .state()
            .variable(&self.ident)
            .cloned()
            .unwrap_or(Value::Null))
    }

    fn type_def(&self, state: &State) -> TypeDef {
        let target = assignment::Target::Internal(self.ident.clone(), None);

        state
            .assignment(&target)
            .cloned()
            .unwrap_or_else(|| TypeDef::new().null().infallible())
    }
}

impl fmt::Display for Variable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ident.fmt(f)
    }
}
