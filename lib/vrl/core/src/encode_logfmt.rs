use std::collections::BTreeMap;

use serde::Serialize;
use value::Value;

use crate::encode_key_value::{to_string as encode_key_value, EncodingError};

/// Serialize the input value map into a logfmt string.
///
/// # Errors
///
/// Returns an `EncodingError` if any of the keys are not strings.
pub fn encode_map<V: Serialize>(input: &BTreeMap<String, V>) -> Result<String, EncodingError> {
    encode_key_value(input, &[], "=", " ", true)
}

/// Serialize the input value into a logfmt string. If the value is not an object,
/// it is treated as the value of an object where the key is "message".
///
/// # Errors
///
/// Returns an `EncodingError` if any of the keys are not strings.
pub fn encode_value(input: &Value) -> Result<String, EncodingError> {
    if let Some(map) = input.as_object() {
        encode_map(map)
    } else {
        let mut map = BTreeMap::new();
        map.insert("message".to_owned(), &input);
        encode_map(&map)
    }
}
