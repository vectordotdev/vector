mod all_fields;
mod contains;
mod get;
mod get_mut;
mod insert;
mod keys;
mod path_iter;
mod remove;

pub(self) use super::Value;
pub(crate) use path_iter::{PathComponent, PathIter};

pub use all_fields::all_fields;
pub use contains::contains;
pub use get::get;
pub use get_mut::get_mut;
pub use insert::{insert, insert_path};
pub use keys::keys;
pub use remove::remove;

#[cfg(test)]
pub(self) mod test {
    use super::Value;
    use serde_json::Value as JsonValue;
    use std::collections::BTreeMap;

    pub fn fields_from_json(json_value: JsonValue) -> BTreeMap<String, Value> {
        match Value::from(json_value) {
            Value::Map(map) => map,
            something => panic!("Expected a map, got {:?}", something),
        }
    }
}
