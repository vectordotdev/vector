use crate::encode_key_value;
use vrl_compiler::Value;

pub fn encode<'a>(input: impl IntoIterator<Item = (String, Value)> + 'a) -> String {
    encode_key_value::encode(input, &[], "=", " ", true)
}
