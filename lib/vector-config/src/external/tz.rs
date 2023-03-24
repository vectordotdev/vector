use std::cell::RefCell;

use serde_json::Value;

use crate::{
    schema::{generate_string_schema, SchemaGenerator, SchemaObject},
    Configurable, GenerateError, Metadata, ToValue,
};

impl Configurable for chrono_tz::Tz {
    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("An IANA timezone.");
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl ToValue for chrono_tz::Tz {
    fn to_value(&self) -> Value {
        Value::String(self.to_string())
    }
}
