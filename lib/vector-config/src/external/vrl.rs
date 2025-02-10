use std::cell::RefCell;

use serde_json::Value;
use vrl::{compiler::VrlRuntime, datadog_search_syntax::QueryNode, value::Value as VrlValue};

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

impl Configurable for VrlValue {
    fn is_optional() -> bool {
        true
    }

    fn metadata() -> Metadata {
        Metadata::with_transparent(true)
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        // We don't have any constraints on the inputs
        Ok(SchemaObject::default())
    }
}

impl ToValue for VrlValue {
    fn to_value(&self) -> Value {
        // As far as I can tell, this should always succeed, although
        // it will lose type information for some value types
        serde_json::to_value(self).expect("Unable to serialize VRL value")
    }
}
