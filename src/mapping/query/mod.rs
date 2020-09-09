use crate::{
    event::{Event, Value},
    mapping::Result,
};

pub mod arithmetic;
pub mod functions;
pub mod path;

pub(in crate::mapping) trait Function: Send + core::fmt::Debug {
    fn execute(&self, context: &Event) -> Result<Value>;
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

impl Function for Literal {
    fn execute(&self, _: &Event) -> Result<Value> {
        Ok(self.value.clone())
    }
}
