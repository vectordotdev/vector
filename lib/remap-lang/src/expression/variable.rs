use super::Error as E;
use crate::{Expression, Object, Result, State, Value};

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
}
