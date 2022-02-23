mod all_fields;
mod insert;
mod keys;
mod path_iter;
pub use all_fields::all_fields;
pub use insert::insert_path;
pub use keys::keys;
pub use path_iter::{PathComponent, PathIter};

pub(self) use super::Value;

#[cfg(test)]
pub(self) mod test {
    use std::collections::BTreeMap;

    use serde_json::Value as JsonValue;

    use super::Value;

    pub fn fields_from_json(json_value: JsonValue) -> BTreeMap<String, Value> {
        match Value::from(json_value) {
            Value::Object(map) => map,
            something => panic!("Expected a map, got {:?}", something),
        }
    }
}
