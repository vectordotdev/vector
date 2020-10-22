use crate::{Error as E, Expression, Object, Result, State, Value};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("undefined constant: {0}")]
    Undefined(String),
}

#[derive(Debug)]
pub(crate) struct Constant {
    ident: String,
}

impl Constant {
    pub fn new(ident: String) -> Self {
        Self { ident }
    }
}

impl Expression for Constant {
    fn execute(&self, state: &mut State, _: &mut dyn Object) -> Result<Option<Value>> {
        state
            .constant(&self.ident)
            .cloned()
            .ok_or_else(|| E::from(Error::Undefined(self.ident.to_owned())))
            .map(Some)
    }
}
