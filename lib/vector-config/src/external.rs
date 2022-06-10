use schemars::{gen::SchemaGenerator, schema::SchemaObject};

use crate::{
    schema::{finalize_schema, generate_string_schema},
    Configurable, Metadata,
};

impl<'de> Configurable<'de> for &'static encoding_rs::Encoding {
    // TODO: At some point, we might want to override the metadata to define a validation pattern that only matches
    // valid character set encodings... but that would be a very large array of strings, and technically the Encoding
    // Standard standard is a living standard, so... :thinkies:

    fn referencable_name() -> Option<&'static str> {
        Some("encoding_rs::Encoding")
    }

    fn description() -> Option<&'static str> {
        Some(
            "An encoding as defined in the [Encoding Standard](https://encoding.spec.whatwg.org/).",
        )
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        let mut schema = generate_string_schema();
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}
