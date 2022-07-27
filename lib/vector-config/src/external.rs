use schemars::{gen::SchemaGenerator, schema::SchemaObject};
use serde::Serialize;

use crate::{
    schema::{finalize_schema, generate_array_schema, generate_map_schema, generate_string_schema},
    Configurable, Metadata,
};

impl Configurable for &'static encoding_rs::Encoding {
    // TODO: At some point, we might want to override the metadata to define a validation pattern that only matches
    // valid character set encodings... but that would be a very large array of strings, and technically the Encoding
    // Standard standard is a living standard, so... :thinkies:

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

impl<V> Configurable for indexmap::IndexMap<String, V>
where
    V: Configurable + Serialize,
{
    fn is_optional() -> bool {
        // A hashmap with required fields would be... an object.  So if you want that, make a struct
        // instead, not a hashmap.
        true
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<Self>) -> SchemaObject {
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

impl Configurable for toml::Value {
    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<Self>) -> SchemaObject {
        // `toml::Value` can be anything that it is possible to represent in TOML, and equivalently, is anything it's
        // possible to represent in JSON, so.... a default schema indicates that.
        let mut schema = SchemaObject::default();
        finalize_schema(gen, &mut schema, overrides);
        schema
    }
}

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
