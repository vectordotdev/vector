use std::cell::RefCell;

use serde_json::Value;

use crate::{
    schema::{SchemaGenerator, SchemaObject},
    Configurable, GenerateError, ToValue,
};

impl Configurable for toml::Value {
    fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        // `toml::Value` can be anything that it is possible to represent in TOML, and equivalently, is anything it's
        // possible to represent in JSON, so... a default schema indicates that.
        Ok(SchemaObject::default())
    }
}

impl ToValue for toml::Value {
    fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("Could not convert TOML value to JSON")
    }
}
