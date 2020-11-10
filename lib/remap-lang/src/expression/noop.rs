use crate::{Expression, Object, Result, State, Value};

#[derive(Debug, Clone)]
pub struct Noop;

impl Expression for Noop {
    fn execute(&self, _: &mut State, _: &mut dyn Object) -> Result<Option<Value>> {
        Ok(None)
    }
}
