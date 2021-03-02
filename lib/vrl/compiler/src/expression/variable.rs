use crate::expression::Resolved;
use crate::parser::ast::Ident;
use crate::{Context, Expression, State, TypeDef, Value};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    ident: Ident,
    value: Option<Value>,
}

impl Variable {
    // TODO:
    //
    // - Error if variable has not been assigned yet.
    pub(crate) fn new(ident: Ident, state: &State) -> Self {
        let value = state
            .variable(&ident)
            .and_then(|v| v.value.as_ref().cloned());

        Self { ident, value }
    }

    pub(crate) fn ident(&self) -> &Ident {
        &self.ident
    }

    pub(crate) fn value(&self) -> Option<&Value> {
        self.value.as_ref()
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
        state
            .variable(&self.ident)
            .cloned()
            .map(|d| d.type_def)
            .unwrap_or_else(|| TypeDef::new().null().infallible())
    }
}

impl fmt::Display for Variable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ident.fmt(f)
    }
}
