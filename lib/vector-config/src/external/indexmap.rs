use indexmap::{IndexMap, IndexSet};
use serde_json::Value;

use crate::{
    schema::{assert_string_schema_for_map, generate_map_schema, generate_set_schema},
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    str::ConfigurableString,
    Configurable, GenerateError, Metadata, ToValue,
};

impl<K, V> Configurable for IndexMap<K, V>
where
    K: ConfigurableString + ToValue + std::hash::Hash + Eq,
    V: Configurable + ToValue,
{
    fn is_optional() -> bool {
        // A hashmap with required fields would be... an object.  So if you want that, make a struct
        // instead, not a hashmap.
        true
    }

    fn metadata() -> Metadata {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata) -> Result<(), GenerateError> {
        let converted = metadata.convert();
        V::validate_metadata(&converted)
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        // Make sure our key type is _truly_ a string schema.
        assert_string_schema_for_map::<K, Self>(gen)?;

        generate_map_schema::<V>(gen)
    }
}

impl<K, V> ToValue for IndexMap<K, V>
where
    K: ToString,
    V: ToValue,
{
    fn to_value(&self) -> Value {
        Value::Object(
            self.iter()
                .map(|(k, v)| (k.to_string(), v.to_value()))
                .collect(),
        )
    }
}

impl<V> Configurable for IndexSet<V>
where
    V: Configurable + ToValue + std::hash::Hash + Eq,
{
    fn metadata() -> Metadata {
        Metadata::with_transparent(true)
    }

    fn validate_metadata(metadata: &Metadata) -> Result<(), GenerateError> {
        let converted = metadata.convert();
        V::validate_metadata(&converted)
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        generate_set_schema::<V>(gen)
    }
}

impl<V: ToValue> ToValue for IndexSet<V> {
    fn to_value(&self) -> Value {
        Value::Array(self.iter().map(ToValue::to_value).collect())
    }
}
