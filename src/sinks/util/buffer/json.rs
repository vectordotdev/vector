use crate::sinks::util::Batch;
use serde_json::value::RawValue;

type BoxedRawValue = Box<RawValue>;

/// A `batch` implementation for storing an array of serialized
/// json values.
///
/// To ensure that `push` does not panic pair it with the use of
/// `JsonArray::encode` to produce the correct `Vec<u8>`.
#[derive(Default, Debug)]
pub struct JsonArrayBuffer {
    buffer: Vec<BoxedRawValue>,
    size: usize,
}

impl JsonArrayBuffer {
    /// Encoding via this will ensure that pushing into this batch will
    // not panic.
    pub fn encode(value: impl serde::Serialize) -> Result<Vec<u8>, crate::Error> {
        serde_json::to_vec(&value).map_err(Into::into)
    }
}

impl Batch for JsonArrayBuffer {
    type Input = Vec<u8>;
    type Output = Vec<BoxedRawValue>;

    fn len(&self) -> usize {
        self.size
    }

    fn push(&mut self, item: Self::Input) {
        self.size += item.len();
        let item = RawValue::from_string(String::from_utf8(item).unwrap()).unwrap();
        self.buffer.push(item);
    }

    fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    fn fresh(&self) -> Self {
        JsonArrayBuffer::default()
    }

    fn finish(self) -> Self::Output {
        self.buffer
    }

    fn num_items(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn multi_object_array() {
        let mut buffer = JsonArrayBuffer::default();

        buffer.push(
            JsonArrayBuffer::encode(json!({
                "key1": "value1"
            }))
            .unwrap(),
        );

        buffer.push(
            JsonArrayBuffer::encode(json!({
                "key2": "value2"
            }))
            .unwrap(),
        );

        assert_eq!(buffer.num_items(), 2);
        assert_eq!(buffer.len(), 34);

        let json = buffer.finish();

        let wrapped = serde_json::to_string(&json!({
            "arr": json,
        }))
        .unwrap();

        let expected = serde_json::to_string(&json!({
            "arr": [
                {
                    "key1": "value1"
                },
                {
                    "key2": "value2"
                },
            ]
        }))
        .unwrap();

        assert_eq!(wrapped, expected);
    }
}
