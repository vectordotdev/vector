use crate::{
    schema::{finalize_schema, generate_array_schema},
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, Metadata,
};

impl Configurable for no_proxy::NoProxy {
    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<Self>) -> SchemaObject {
        // `NoProxy` (de)serializes itself as a vector of strings, without any constraints on the string value itself, so
        // we just... do that. We do set the element metadata to be transparent, the same as we do for `Vec<T>`, because
        // all of the pertinent information will be on `NoProxy` itself.
        let mut element_metadata = String::metadata();
        element_metadata.set_transparent();

        let mut schema = generate_array_schema(gen, element_metadata);
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}
