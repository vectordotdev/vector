use crate::encode_key_value::{encode as encode_key_value, EncodingError};
use serde::Serialize;

pub fn encode<'a, V: Serialize>(
    input: BTreeMap<String, V>,
    fields_order: &[String],
) -> Result<String, EncodingError> {
    encode_key_value(input, fields_order, "=", " ", true)
}
