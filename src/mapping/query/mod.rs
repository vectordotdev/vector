use crate::{
    event::{Event, Value},
    mapping::Result,
};

pub mod arithmetic;
pub mod function;
pub mod path;

pub(in crate::mapping) trait Function: Send + core::fmt::Debug {
    /// Run the function to produce a [`Value`].
    fn execute(&self, context: &Event) -> Result<Value>;

    /// Return the static set of parameters this function accepts.
    fn parameters() -> &'static [function::Parameter]
    where
        Self: Sized,
    {
        &[]
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct Literal {
    value: Value,
}

impl From<Value> for Literal {
    fn from(value: Value) -> Self {
        Self { value }
    }
}

#[cfg(test)]
impl From<&str> for Literal {
    fn from(value: &str) -> Self {
        Self {
            value: Value::from(value),
        }
    }
}

impl Function for Literal {
    fn execute(&self, _: &Event) -> Result<Value> {
        Ok(self.value.clone())
    }
}
