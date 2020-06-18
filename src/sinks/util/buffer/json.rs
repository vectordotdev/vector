use crate::sinks::util::Batch;
use serde_json::value::{to_raw_value, RawValue, Value};

pub type BoxedRawValue = Box<RawValue>;

/// A `batch` implementation for storing an array of json
/// values.
#[derive(Default, Debug)]
pub struct JsonArrayBuffer {
    buffer: Vec<BoxedRawValue>,
    total_bytes: usize,
}

impl Batch for JsonArrayBuffer {
    type Input = Value;
    type Output = Vec<BoxedRawValue>;

    fn len(&self) -> usize {
        self.total_bytes
    }

    fn push(&mut self, item: Self::Input) {
        let item = to_raw_value(&item).expect("Value should be valid json");
        self.total_bytes += item.get().len();
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

        buffer.push(json!({
            "key1": "value1"
        }));

        buffer.push(json!({
            "key2": "value2"
        }));

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
