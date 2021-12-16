mod all_fields;
mod contains;
mod get;
mod get_mut;
mod insert;
mod keys;
mod path_iter;
mod remove;

pub use all_fields::all_fields;
pub use contains::contains;
pub use get::{get, get_value};
pub use get_mut::get_mut;
pub use insert::{insert, insert_path};
pub use keys::keys;
pub use path_iter::{PathComponent, PathIter};
pub use remove::remove;

pub(self) use super::Value;

#[cfg(test)]
pub(self) mod test {
    use std::collections::BTreeMap;

    use serde_json::Value as JsonValue;

    use super::Value;

    pub fn fields_from_json(json_value: JsonValue) -> BTreeMap<String, Value> {
        match Value::from(json_value) {
            Value::Map(map) => map,
            something => panic!("Expected a map, got {:?}", something),
        }
    }
}
