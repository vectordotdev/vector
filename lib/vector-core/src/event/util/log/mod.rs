mod all_fields;
mod keys;

pub use all_fields::{all_fields, all_fields_non_object_root, all_metadata_fields};
pub use keys::keys;

#[cfg(test)]
mod test {
    use serde_json::Value as JsonValue;

    use crate::event::{ObjectMap, Value};

    pub(crate) fn fields_from_json(json_value: JsonValue) -> ObjectMap {
        match Value::from(json_value) {
            Value::Object(map) => map,
            something => panic!("Expected a map, got {something:?}"),
        }
    }
}
