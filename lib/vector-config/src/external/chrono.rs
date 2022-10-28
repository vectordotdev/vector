use crate::{
    schema::generate_string_schema,
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, GenerateError, Metadata,
};

impl<TZ> Configurable for chrono::DateTime<TZ>
where
    TZ: chrono::TimeZone,
{
    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::default();
        metadata.set_description("ISO 8601 combined date and time with timezone.");
        metadata
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}
