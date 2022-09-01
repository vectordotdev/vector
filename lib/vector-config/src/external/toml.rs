use crate::{
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, GenerateError,
};

impl Configurable for toml::Value {
    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // `toml::Value` can be anything that it is possible to represent in TOML, and equivalently, is anything it's
        // possible to represent in JSON, so... a default schema indicates that.
        Ok(SchemaObject::default())
    }
}
