use super::Error as E;
use crate::{Expression, Object, Result, State, Value};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("missing path: {0}")]
    Missing(String),

    #[error("unable to resolve path: {0}")]
    Resolve(String),
}

#[derive(Debug)]
pub(crate) struct Path {
    path: String,
}

impl Path {
    pub(crate) fn new(path: String) -> Self {
        Self { path }
    }
}

impl Expression for Path {
    fn execute(&self, _: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        object
            .find(&self.path)
            .map_err(|e| E::from(Error::Resolve(e)))?
            .ok_or_else(|| E::from(Error::Missing(self.path.to_owned())).into())
            .map(Some)
    }
}
