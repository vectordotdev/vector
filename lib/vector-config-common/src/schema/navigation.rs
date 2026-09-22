use super::{DEFINITIONS_PREFIX, RootSchema, Schema, SchemaObject, Set};

impl RootSchema {
    /// Finds the value schema of a named map at the configuration root.
    ///
    /// Follows local references and `allOf` (flattened fields), but does not
    /// descend into nested properties or choose between union alternatives.
    pub fn root_map_value_schema(&self, property: &str) -> Option<&Schema> {
        for schema in self.object_schemas(&self.schema) {
            let Some(map) = schema
                .object
                .as_ref()
                .and_then(|object| object.properties.get(property))
                .and_then(Schema::as_object)
            else {
                continue;
            };
            if let Some(value) = self
                .object_schemas(map)
                .find_map(|schema| schema.object.as_ref()?.additional_properties.as_deref())
            {
                return Some(value);
            }
        }
        None
    }

    fn object_schemas<'a>(
        &'a self,
        schema: &'a SchemaObject,
    ) -> impl Iterator<Item = &'a SchemaObject> {
        let mut pending = vec![schema];
        let mut seen = Set::new();
        std::iter::from_fn(move || {
            let schema = pending.pop()?;
            if let Some(all_of) = schema.subschemas.as_ref().and_then(|s| s.all_of.as_ref()) {
                pending.extend(all_of.iter().rev().filter_map(Schema::as_object));
            }
            if let Some(name) = schema
                .reference
                .as_deref()
                .and_then(|r| r.strip_prefix(DEFINITIONS_PREFIX))
                && seen.insert(name)
                && let Some(target) = self.definitions.get(name).and_then(Schema::as_object)
            {
                pending.push(target);
            }
            Some(schema)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn root_map_lookup_follows_flattening_and_references() {
        let root: RootSchema = serde_json::from_value(json!({
            "allOf": [{"$ref": "#/definitions/config"}],
            "definitions": {
                "config": {"properties": {"sources": {"$ref": "#/definitions/map"}}},
                "map": {"type": "object", "additionalProperties": {"$ref": "#/definitions/outer"}},
                "outer": {"type": "object"}
            }
        }))
        .unwrap();
        assert_eq!(
            root.root_map_value_schema("sources")
                .unwrap()
                .as_object()
                .unwrap()
                .reference
                .as_deref(),
            Some("#/definitions/outer")
        );
        assert!(root.root_map_value_schema("sinks").is_none());
    }

    #[test]
    fn root_map_lookup_does_not_search_nested_properties_or_loop_on_refs() {
        let root: RootSchema = serde_json::from_value(json!({
            "allOf": [{"$ref": "#/definitions/loop"}],
            "properties": {"nested": {"properties": {
                "sources": {"additionalProperties": {"type": "object"}}
            }}},
            "definitions": {"loop": {"$ref": "#/definitions/loop"}}
        }))
        .unwrap();
        assert!(root.root_map_value_schema("sources").is_none());
    }
}
