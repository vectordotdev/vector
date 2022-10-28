use vector_config_common::validation::{Format, Validation};

use crate::{
    schema::generate_string_schema,
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, GenerateError, Metadata,
};

impl Configurable for url::Url {
    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::default();
        metadata.set_description("A uniform resource location (URL).");
        metadata.add_validation(Validation::KnownFormat(Format::Uri));
        metadata
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        Ok(generate_string_schema())
    }
}
