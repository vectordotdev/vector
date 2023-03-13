use std::cell::RefCell;

use chrono::{DateTime, TimeZone};
use serde_json::Value;

use crate::{
    schema::{generate_string_schema, SchemaGenerator, SchemaObject},
    Configurable, GenerateError, Metadata, ToValue,
};

impl<TZ> Configurable for DateTime<TZ>
where
    TZ: TimeZone,
{
    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("ISO 8601 combined date and time with timezone.");
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl<TZ> ToValue for DateTime<TZ>
where
    Self: ToString,
    TZ: TimeZone,
{
    fn to_value(&self) -> Value {
        Value::String(self.to_string())
    }
}
