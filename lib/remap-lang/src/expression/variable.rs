use super::Error as E;
use crate::{CompilerState, Expression, Object, Result, State, TypeCheck, Value, ValueConstraint};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("undefined variable: {0}")]
    Undefined(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Variable {
    ident: String,
}

impl Variable {
    pub fn new(ident: String) -> Self {
        Self { ident }
    }
}

impl Expression for Variable {
    fn execute(&self, state: &mut State, _: &mut dyn Object) -> Result<Option<Value>> {
        state
            .variable(&self.ident)
            .cloned()
            .ok_or_else(|| E::from(Error::Undefined(self.ident.to_owned())).into())
            .map(Some)
    }

    fn type_check(&self, state: &CompilerState) -> TypeCheck {
        state
            .variable_type(&self.ident)
            .cloned()
            .unwrap_or(TypeCheck {
                fallible: true,
                optional: false,
                constraint: ValueConstraint::Any,
            })
    }
}
