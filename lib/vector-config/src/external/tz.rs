use crate::{
    schema::generate_string_schema,
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, GenerateError,
};

impl Configurable for chrono_tz::Tz {
    fn description() -> Option<&'static str> {
        Some("An IANA timezone.")
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}
