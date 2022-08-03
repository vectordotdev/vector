use serde::Serialize;

use crate::{
    schema::{assert_string_schema_for_map, finalize_schema, generate_map_schema},
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    str::ConfigurableString,
    Configurable, Metadata,
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

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<Self>) -> SchemaObject {
        // Make sure our key type is _truly_ a string schema.
        assert_string_schema_for_map::<K, Self>(gen);

        // We explicitly do not pass anything from the override metadata, because there's nothing to
        // reasonably pass: if `V` is referenceable, using the description for `IndexMap<String, V>`
        // likely makes no sense, nor would a default make sense, and so on.
        //
        // We do, however, set `V` to be "transparent", which means that during schema finalization,
        // we will relax the rules we enforce, such as needing a description, knowing that they'll
        // be enforced on the field using `IndexMap<String, V>` itself, where carrying that
        // description forward to `V` might literally make no sense, such as when `V` is a primitive
        // type like an integer or string.
        let mut value_metadata = V::metadata();
        value_metadata.set_transparent();

        let mut schema = generate_map_schema(gen, value_metadata);
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}
