use crate::{
    schema::generate_string_schema,
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, GenerateError,
};

impl<TZ> Configurable for chrono::DateTime<TZ>
where
    TZ: chrono::TimeZone,
{
    fn description() -> Option<&'static str> {
        Some("ISO 8601 combined date and time with timezone.")
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}
