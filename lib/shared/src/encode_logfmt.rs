use crate::encode_key_value::{to_string as encode_key_value, EncodingError};
use serde::Serialize;
use std::collections::BTreeMap;

pub fn to_string<V: Serialize>(input: BTreeMap<String, V>) -> Result<String, EncodingError> {
    encode_key_value(input, &[], "=", " ", true)
}
