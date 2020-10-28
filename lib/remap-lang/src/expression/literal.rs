use crate::{Expression, Object, Result, State, Value};

#[derive(Debug)]
pub struct Literal(Value);

impl<T: Into<Value>> From<T> for Literal {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl Expression for Literal {
    fn execute(&self, _: &mut State, _: &mut dyn Object) -> Result<Option<Value>> {
        Ok(Some(self.0.clone()))
    }
}
