use http::StatusCode;
use serde_json::Value;
use std::cell::RefCell;

use crate::{
    schema::{generate_number_schema, SchemaGenerator, SchemaObject},
    Configurable, GenerateError, Metadata, ToValue,
};

impl ToValue for StatusCode {
    fn to_value(&self) -> Value {
        serde_json::to_value(self.as_u16()).expect("Could not convert HTTP status code to JSON")
    }
}

impl Configurable for StatusCode {
    fn referenceable_name() -> Option<&'static str> {
        Some("http::StatusCode")
    }

    fn is_optional() -> bool {
        true
    }

    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("HTTP response status code");
        metadata.set_default_value(StatusCode::OK);
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_number_schema::<u16>())
    }
}
