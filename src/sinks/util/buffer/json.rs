use serde_json::value::{to_raw_value, RawValue, Value};

use super::super::batch::{err_event_too_large, Batch, BatchSize, PushResult};

pub type BoxedRawValue = Box<RawValue>;

/// A `batch` implementation for storing an array of json
/// values.
///
/// Note: This has been deprecated, please do not use when creating new Sinks.
#[derive(Debug)]
pub struct JsonArrayBuffer {
    buffer: Vec<BoxedRawValue>,
    total_bytes: usize,
    settings: BatchSize<Self>,
}

impl JsonArrayBuffer {
    pub const fn new(settings: BatchSize<Self>) -> Self {
        Self {
            buffer: Vec::new(),
            total_bytes: 0,
            settings,
        }
    }
}

impl Batch for JsonArrayBuffer {
    type Input = Value;
    type Output = Vec<BoxedRawValue>;

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        let raw_item = to_raw_value(&item).expect("Value should be valid json");
        let new_len = self.total_bytes + raw_item.get().len() + 1;
        if self.is_empty() && new_len >= self.settings.bytes {
            err_event_too_large(raw_item.get().len(), self.settings.bytes)
        } else if self.buffer.len() >= self.settings.events || new_len > self.settings.bytes {
            PushResult::Overflow(item)
        } else {
            self.total_bytes = new_len;
            self.buffer.push(raw_item);
            PushResult::Ok(
                self.buffer.len() >= self.settings.events || new_len >= self.settings.bytes,
            )
        }
    }

    fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    fn fresh(&self) -> Self {
        Self::new(self.settings)
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
    use serde_json::json;

    use super::{super::PushResult, *};
    use crate::sinks::util::BatchSettings;

    #[test]
    fn multi_object_array() {
        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 2;

        let mut buffer = JsonArrayBuffer::new(batch_settings.size);

        assert_eq!(
            buffer.push(json!({
                "key1": "value1"
            })),
            PushResult::Ok(false)
        );

        assert_eq!(
            buffer.push(json!({
                "key2": "value2"
            })),
            PushResult::Ok(true)
        );

        assert!(matches!(buffer.push(json!({})), PushResult::Overflow(_)));

        assert_eq!(buffer.num_items(), 2);
        assert_eq!(buffer.total_bytes, 36);

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
