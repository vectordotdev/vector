use crate::{Error, Expr, Expression, Object, Result, State, Value};

#[derive(Debug)]
pub(crate) struct Abort {
    reason: Option<Box<Expr>>,
}

impl Abort {
    pub fn new(reason: Option<Box<Expr>>) -> Self {
        Self { reason }
    }
}

impl Expression for Abort {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        match self
            .reason
            .as_ref()
            .map(|r| r.execute(state, object).transpose())
            .flatten()
            .transpose()?
        {
            Some(v) => Err(Error::Abort(Some(v.to_string_lossy()))),
            None => Err(Error::Abort(None)),
        }
    }
}
