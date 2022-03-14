mod all_fields;
mod contains;
mod insert;
mod keys;
mod remove;

pub use all_fields::all_fields;
pub use contains::contains;
pub use insert::insert;
pub use keys::keys;
pub use remove::remove;

pub(self) use super::Value;

#[cfg(test)]
pub(self) mod test {
    use std::collections::BTreeMap;

    use serde_json::Value as JsonValue;

    use super::Value;

    pub(crate) fn fields_from_json(json_value: JsonValue) -> BTreeMap<String, Value> {
        match Value::from(json_value) {
            Value::Object(map) => map,
            something => panic!("Expected a map, got {:?}", something),
        }
    }
}
