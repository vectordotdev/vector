use std::collections::BTreeMap;

use serde::Serialize;

use crate::encode_key_value::{to_string as encode_key_value, EncodingError};

pub fn to_string<V: Serialize>(input: BTreeMap<String, V>) -> Result<String, EncodingError> {
    encode_key_value(input, &[], "=", " ", true)
}
