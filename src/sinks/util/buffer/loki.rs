//! Special case Loki sink batch buffer
//!
//! This buffer handles stream merging -- when a record is inserted into
//! the buffer, all records having the same stream label set are grouped
//! together for more efficient output.

use super::{
    err_event_too_large, json::BoxedRawValue, Batch, BatchConfig, BatchError, BatchSettings,
    BatchSize, PushResult,
};
use serde_json::{json, value::to_raw_value};
use std::collections::HashMap;

const WRAPPER_OVERHEAD: usize = r#"{"streams":[]}"#.len();
const STREAM_OVERHEAD: usize = r#"{"stream":{},"values":[]}"#.len();
const LABEL_OVERHEAD: usize = r#""":"""#.len();

pub type Labels = Vec<(String, String)>;

#[derive(Clone, Debug)]
pub struct LokiEvent {
    pub timestamp: i64,
    pub event: String,
}

#[derive(Clone, Debug)]
pub struct LokiRecord {
    pub labels: Labels,
    pub event: LokiEvent,
}

#[derive(Debug)]
struct LokiEncodedEvent {
    pub timestamp: i64,
    pub encoded: BoxedRawValue,
}

impl From<&LokiEvent> for LokiEncodedEvent {
    // Pre-encode the record to JSON, but keep the timestamp for sorting at the end.
    // The final output should be: `[ts, line]'
    fn from(event: &LokiEvent) -> Self {
        Self {
            timestamp: event.timestamp,
            encoded: to_raw_value(&json!([format!("{}", event.timestamp), event.event]))
                .expect("JSON encoding should never fail"),
        }
    }
}

#[derive(Debug)]
pub struct LokiBuffer {
    num_bytes: usize,
    num_items: usize,
    streams: HashMap<Labels, Vec<LokiEncodedEvent>>,
    settings: BatchSize<Self>,
}

impl LokiBuffer {
    pub fn new(settings: BatchSize<Self>) -> Self {
        Self {
            num_bytes: WRAPPER_OVERHEAD,
            num_items: 0,
            streams: HashMap::default(),
            settings,
        }
    }
}

impl Batch for LokiBuffer {
    type Input = LokiRecord;
    type Output = serde_json::Value;

    fn get_settings_defaults(
        config: BatchConfig,
        defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError> {
        Ok(config
            .use_size_as_events()?
            .get_settings_or_default(defaults))
    }

    fn push(&mut self, mut item: Self::Input) -> PushResult<Self::Input> {
        let labels_len = item
            .labels
            .iter()
            .map(|label| label.0.len() + label.1.len() + LABEL_OVERHEAD)
            .sum::<usize>()
            + STREAM_OVERHEAD;
        let event: LokiEncodedEvent = (&item.event).into();
        let event_len = event.encoded.get().len();

        if self.is_empty() && WRAPPER_OVERHEAD + labels_len + event_len > self.settings.bytes {
            err_event_too_large(WRAPPER_OVERHEAD + labels_len + event_len)
        } else if self.num_items >= self.settings.events
            || self.num_bytes + event_len + 1 > self.settings.bytes
        {
            PushResult::Overflow(item)
        } else {
            // We must sort the stream labels here to ensure they hash to
            // the same stream if the label set matches.
            item.labels.sort();
            let new_bytes = match self.streams.get_mut(&item.labels) {
                // Label exists, and we checked the size, just add it
                Some(stream) => {
                    stream.push(event);
                    event_len + 1
                }
                None => {
                    // Have to verify label size doesn't cause overflow
                    let new_bytes =
                        labels_len + event_len + if self.streams.is_empty() { 0 } else { 1 };
                    if self.num_bytes + new_bytes > self.settings.bytes {
                        return PushResult::Overflow(item);
                    } else {
                        self.streams.insert(item.labels, vec![event]);
                        new_bytes
                    }
                }
            };
            self.num_bytes += new_bytes;
            self.num_items += 1;
            PushResult::Ok(
                self.num_bytes >= self.settings.bytes || self.num_items >= self.settings.events,
            )
        }
    }

    fn is_empty(&self) -> bool {
        self.streams.is_empty()
    }

    fn fresh(&self) -> Self {
        Self::new(self.settings)
    }

    fn finish(self) -> Self::Output {
        let streams_json = self
            .streams
            .into_iter()
            .map(|(stream, mut events)| {
                // Sort events by timestamp
                events.sort_by_key(|e| e.timestamp);

                let stream = stream.into_iter().collect::<HashMap<_, _>>();
                let events = events.into_iter().map(|e| e.encoded).collect::<Vec<_>>();

                (
                    to_raw_value(&stream).expect("JSON encoding should never fail"),
                    events,
                )
            })
            .collect::<Vec<_>>();

        // This is just to guarantee stable key ordering for tests
        #[cfg(test)]
        let streams_json = {
            let mut streams_json = streams_json;
            streams_json.sort_unstable_by_key(|s| s.0.to_string());
            streams_json
        };

        let streams_json = streams_json
            .into_iter()
            .map(|(stream, events)| {
                json!({
                    "stream": stream,
                    "values": events,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "streams": streams_json,
        })
    }

    fn num_items(&self) -> usize {
        self.num_items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_finish(buffer: LokiBuffer, expected_json: &str) {
        let buffer_bytes = buffer.num_bytes;
        let json = serde_json::to_string(&buffer.finish()).unwrap();
        // Does it track the number of bytes exactly before encoding?
        assert!(
            buffer_bytes == expected_json.len(),
            "Buffer num_bytes {} was not expected {}",
            buffer_bytes,
            expected_json.len()
        );
        // Does the output JSON match what we expect?
        assert_eq!(json, expected_json);
    }

    #[test]
    fn insert_single() {
        let mut buffer = LokiBuffer::new(BatchSettings::default().size);
        assert!(matches!(
            buffer.push(LokiRecord {
                labels: vec![("label1".into(), "value1".into())],
                event: LokiEvent {
                    timestamp: 123456789,
                    event: "this is an event".into(),
                },
            }),
            PushResult::Ok(false)
        ));

        assert_eq!(buffer.num_items, 1);
        assert_eq!(buffer.streams.len(), 1);
        test_finish(
            buffer,
            r#"{"streams":[{"stream":{"label1":"value1"},"values":[["123456789","this is an event"]]}]}"#,
        );
    }

    #[test]
    fn insert_multiple_streams() {
        let mut buffer = LokiBuffer::new(BatchSettings::default().size);
        for n in 1..4 {
            assert!(matches!(
                buffer.push(LokiRecord {
                    labels: vec![("asdf".into(), format!("value{}", n))],
                    event: LokiEvent {
                        timestamp: 123456780 + n,
                        event: format!("event #{}", n),
                    },
                }),
                PushResult::Ok(false)
            ));
        }

        assert_eq!(buffer.num_items, 3);
        assert_eq!(buffer.streams.len(), 3);
        test_finish(
            buffer,
            r#"{"streams":[{"stream":{"asdf":"value1"},"values":[["123456781","event #1"]]},{"stream":{"asdf":"value2"},"values":[["123456782","event #2"]]},{"stream":{"asdf":"value3"},"values":[["123456783","event #3"]]}]}"#,
        );
    }

    #[test]
    fn insert_multiple_one_stream() {
        let mut buffer = LokiBuffer::new(BatchSettings::default().size);
        for n in 1..4 {
            assert!(matches!(
                buffer.push(LokiRecord {
                    labels: vec![("asdf".into(), "value1".into())],
                    event: LokiEvent {
                        timestamp: 123456780 + n,
                        event: format!("event #{}", n),
                    },
                }),
                PushResult::Ok(false)
            ));
        }

        assert_eq!(buffer.num_items, 3);
        assert_eq!(buffer.streams.len(), 1);
        test_finish(
            buffer,
            r#"{"streams":[{"stream":{"asdf":"value1"},"values":[["123456781","event #1"],["123456782","event #2"],["123456783","event #3"]]}]}"#,
        );
    }
}
