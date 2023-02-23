use std::cell::RefCell;

use serde_json::Value;
use vector_config_common::validation::{Format, Validation};

use crate::{
    schema::{generate_string_schema, SchemaGenerator, SchemaObject},
    Configurable, GenerateError, Metadata, ToValue,
};

impl Configurable for url::Url {
    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("A uniform resource location (URL).");
        metadata.add_validation(Validation::KnownFormat(Format::Uri));
        metadata
    }

    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}

impl ToValue for url::Url {
    fn to_value(&self) -> Value {
        Value::String(self.to_string())
    }
}
