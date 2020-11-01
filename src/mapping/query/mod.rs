use crate::{
    event::{Event, Value},
    mapping::Result,
};

pub mod arithmetic;
pub mod function;
pub mod path;
pub mod query_value;
pub mod regex;

use query_value::QueryValue;

pub(in crate::mapping) trait Function: Send + core::fmt::Debug {
    /// Run the function to produce a [`Value`].
    fn execute(&self, context: &Event) -> Result<QueryValue>;

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
    value: QueryValue,
}

impl From<Value> for Literal {
    fn from(value: Value) -> Self {
        Self {
            value: QueryValue::Value(value),
        }
    }
}

impl From<QueryValue> for Literal {
    fn from(value: QueryValue) -> Self {
        Self { value }
    }
}

impl Function for Literal {
    fn execute(&self, _: &Event) -> Result<query_value::QueryValue> {
        Ok(self.value.clone())
    }
}
