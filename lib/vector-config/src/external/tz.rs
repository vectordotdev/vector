use crate::{
    schema::generate_string_schema,
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, GenerateError, Metadata,
};

impl Configurable for chrono_tz::Tz {
    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::default();
        metadata.set_description("An IANA timezone.");
        metadata
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}
