use crate::{
    schema::finalize_schema,
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, Metadata,
};

impl Configurable for toml::Value {
    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<Self>) -> SchemaObject {
        // `toml::Value` can be anything that it is possible to represent in TOML, and equivalently, is anything it's
        // possible to represent in JSON, so.... a default schema indicates that.
        let mut schema = SchemaObject::default();
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}
