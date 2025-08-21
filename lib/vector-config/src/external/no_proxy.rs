use std::cell::RefCell;

use serde_json::Value;

use crate::{
    Configurable, GenerateError, Metadata, ToValue,
    schema::{SchemaGenerator, SchemaObject, generate_array_schema},
};

impl Configurable for no_proxy::NoProxy {
    fn metadata() -> Metadata {
        // Any schema that maps to a scalar type needs to be marked as transparent... and since we
        // generate a schema equivalent to a string, we need to mark ourselves as transparent, too.
        Metadata::with_transparent(true)
    }

    fn generate_schema(
        generator: &RefCell<SchemaGenerator>,
    ) -> Result<SchemaObject, GenerateError> {
        // `NoProxy` (de)serializes itself as a vector of strings, without any constraints on the string value itself, so
        // we just... do that.
        generate_array_schema(&String::as_configurable_ref(), generator)
    }
}

impl ToValue for no_proxy::NoProxy {
    fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("Could not convert no-proxy list to JSON")
    }
}
