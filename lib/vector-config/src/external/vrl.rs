use std::cell::RefCell;

use serde_json::Value;
use vrl::{compiler::VrlRuntime, datadog_search_syntax::QueryNode};

use crate::{
    schema::{generate_string_schema, SchemaGenerator, SchemaObject},
    Configurable, GenerateError, Metadata, ToValue,
};

impl Configurable for VrlRuntime {
    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("The runtime to use for executing VRL code.");
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl ToValue for VrlRuntime {
    fn to_value(&self) -> Value {
        Value::String(match self {
            VrlRuntime::Ast => "ast".to_owned(),
        })
    }
}

impl Configurable for QueryNode {
    fn generate_schema(_gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError>
    where
        Self: Sized,
    {
        Ok(generate_string_schema())
    }
}
