mod all_fields;
mod keys;

pub use all_fields::{all_fields, all_fields_non_object_root};
pub use keys::keys;

pub(self) use super::Value;

#[cfg(test)]
pub(self) mod test {
    use std::collections::BTreeMap;

    use serde_json::Value as JsonValue;

    use super::Value;

    pub(crate) fn fields_from_json(json_value: JsonValue) -> BTreeMap<String, Value> {
        match Value::from(json_value) {
            Value::Object(map) => map,
            something => panic!("Expected a map, got {something:?}"),
        }
    }
}
