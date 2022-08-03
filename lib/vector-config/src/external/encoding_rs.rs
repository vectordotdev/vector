use crate::{
    schema::{finalize_schema, generate_string_schema},
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, Metadata,
};

impl Configurable for &'static encoding_rs::Encoding {
    // TODO: At some point, we might want to override the metadata to define a validation pattern that only matches
    // valid character set encodings... but that would be a very large array of strings, and technically the Encoding
    // Standard is a living standard, so... :thinkies:

    fn referenceable_name() -> Option<&'static str> {
        Some("encoding_rs::Encoding")
    }

    fn description() -> Option<&'static str> {
        Some(
            "An encoding as defined in the [Encoding Standard](https://encoding.spec.whatwg.org/).",
        )
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<Self>) -> SchemaObject {
        let mut schema = generate_string_schema();
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}
