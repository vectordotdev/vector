use crate::{
    schema::generate_array_schema,
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, GenerateError, Metadata,
};

impl Configurable for no_proxy::NoProxy {
    fn metadata() -> Metadata<Self> {
        // Any schema that maps to a scalar type needs to be marked as transparent... and since we
        // generate a schema equivalent to a string, we need to mark ourselves as transparent, too.
        Metadata::with_transparent(true)
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // `NoProxy` (de)serializes itself as a vector of strings, without any constraints on the string value itself, so
        // we just... do that.
        generate_array_schema::<String>(gen)
    }
}
