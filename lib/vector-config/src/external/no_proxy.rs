use crate::{
    schema::generate_array_schema,
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, GenerateError,
};

impl Configurable for no_proxy::NoProxy {
    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // `NoProxy` (de)serializes itself as a vector of strings, without any constraints on the string value itself, so
        // we just... do that.
        generate_array_schema::<String>(gen)
    }
}
