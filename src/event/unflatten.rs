use crate::event::{Atom, Value};
use std::collections::HashMap;

pub fn unflatten_dotted(mut key: Atom, mut value: Value) -> (Atom, Value) {
    let key_string = key.to_string();
    let mut iter = key_string.rsplit('.').peekable();
    while let Some(current) = iter.next() {
        if iter.peek().is_none() {
            key = Atom::from(current);
            break;
        }
        let mut map = HashMap::new();
        map.insert(Atom::from(current), value);
        value = Value::Map(map);
    }
    (key, value)
}
