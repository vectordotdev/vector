use crate::event::{Atom, LogEvent, Value};
use serde_json::{Map, Value as JsonValue};

/// Recursevly inserts json values to event
pub fn flatten(event: &mut LogEvent, map: Map<String, JsonValue>) {
    for (name, value) in map {
        insert(event, name, value);
    }
}

/// Recursevly inserts json values to event under given name
pub fn insert<K: Into<Atom> + AsRef<str>>(event: &mut LogEvent, name: K, value: JsonValue) {
    match value {
        JsonValue::String(string) => {
            event.insert(name, string);
        }
        JsonValue::Number(number) => {
            let val: Value = if let Some(val) = number.as_i64() {
                val.into()
            } else if let Some(val) = number.as_f64() {
                val.into()
            } else {
                number.to_string().into()
            };

            event.insert(name, val);
        }
        JsonValue::Bool(b) => {
            event.insert(name, b);
        }
        JsonValue::Null => {
            event.insert(name, Value::Null);
        }
        JsonValue::Array(array) => {
            for (i, element) in array.into_iter().enumerate() {
                let element_name = format!("{}[{}]", name.as_ref(), i);
                insert(event, element_name, element);
            }
        }
        JsonValue::Object(object) => {
            for (key, value) in object.into_iter() {
                let item_name = format!("{}.{}", name.as_ref(), key);
                insert(event, item_name, value);
            }
        }
    }
}
