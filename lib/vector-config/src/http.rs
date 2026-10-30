use std::cell::RefCell;

use serde_json::Value;
use vector_config_common::{attributes::CustomAttribute, constants};

use crate::{
    Configurable, GenerateError, Metadata, ToValue,
    num::NumberClass,
    schema::{SchemaGenerator, SchemaObject, generate_number_schema},
};

/// Implements `ToValue` and `Configurable` for an HTTP status code type from either `http` crate
/// version, so both share the same referenceable name and schema.
macro_rules! impl_status_code {
    ($status_code:ty) => {
        impl ToValue for $status_code {
            fn to_value(&self) -> Value {
                serde_json::to_value(self.as_u16())
                    .expect("Could not convert HTTP status code to JSON")
            }
        }

        impl Configurable for $status_code {
            fn referenceable_name() -> Option<&'static str> {
                Some("http::StatusCode")
            }

            fn is_optional() -> bool {
                true
            }

            fn metadata() -> Metadata {
                let mut metadata = Metadata::default();
                metadata.set_description("HTTP response status code");
                metadata.add_custom_attribute(CustomAttribute::kv(
                    constants::DOCS_META_NUMERIC_TYPE,
                    NumberClass::Unsigned,
                ));
                metadata
            }

            fn generate_schema(
                _: &RefCell<SchemaGenerator>,
            ) -> Result<SchemaObject, GenerateError> {
                Ok(generate_number_schema::<u16>())
            }
        }
    };
}

impl_status_code!(http_0::StatusCode);
impl_status_code!(http::StatusCode);
