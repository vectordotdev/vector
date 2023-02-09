use serde::Serialize;

use crate::{
    schema::{assert_string_schema_for_map, generate_map_schema, generate_set_schema},
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    str::ConfigurableString,
    Configurable, GenerateError, Metadata,
};

impl<K, V> Configurable for indexmap::IndexMap<K, V>
where
    K: ConfigurableString + Serialize + std::hash::Hash + Eq,
    V: Configurable + Serialize,
{
    fn is_optional() -> bool {
        // A hashmap with required fields would be... an object.  So if you want that, make a struct
        // instead, not a hashmap.
        true
    }

    fn metadata() -> Metadata<Self> {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata<Self>) -> Result<(), GenerateError> {
        let converted = metadata.convert::<V>();
        V::validate_metadata(&converted)
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // Make sure our key type is _truly_ a string schema.
        assert_string_schema_for_map::<K, Self>(gen)?;

        generate_map_schema::<V>(gen)
    }
}

impl<V> Configurable for indexmap::IndexSet<V>
where
    V: Configurable + Serialize + std::hash::Hash + Eq,
{
    fn metadata() -> Metadata<Self> {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata<Self>) -> Result<(), GenerateError> {
        let converted = metadata.convert::<V>();
        V::validate_metadata(&converted)
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        generate_set_schema::<V>(gen)
    }
}
