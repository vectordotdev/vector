//! Special case Loki sink batch buffer
//!
//! This buffer handles stream merging -- when a record is inserted into
//! the buffer, all records having the same stream label set are grouped
//! together for more efficient output.

use super::{
    err_event_too_large, json::BoxedRawValue, Batch, BatchConfig, BatchError, BatchSettings,
    BatchSize, PushResult,
};
use crate::sinks::loki::OutOfOrderAction;
use dashmap::DashMap;
use serde_json::{json, value::to_raw_value};
use std::collections::HashMap;
use std::sync::Arc;

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
    pub partition: PartitionKey,
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

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct PartitionKey {
    pub tenant_id: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct GlobalTimestamps {
    map: Arc<DashMap<PartitionKey, HashMap<Labels, i64>>>,
}

impl GlobalTimestamps {
    pub fn take(&self, partition: &PartitionKey) -> HashMap<Labels, i64> {
        self.map
            .remove(partition)
            .map(|(_k, v)| v)
            .unwrap_or_default()
    }

    pub fn insert(&self, partition: PartitionKey, map: HashMap<Labels, i64>) {
        self.map.insert(partition, map);
    }
}

#[derive(Debug)]
pub struct LokiBuffer {
    num_bytes: usize,
    num_items: usize,
    streams: HashMap<Labels, Vec<LokiEncodedEvent>>,
    settings: BatchSize<Self>,

    partition: Option<PartitionKey>,
    latest_timestamps: Option<HashMap<Labels, i64>>,
    global_timestamps: GlobalTimestamps,
    out_of_order_action: OutOfOrderAction,
}

impl LokiBuffer {
    pub fn new(
        settings: BatchSize<Self>,
        global_timestamps: GlobalTimestamps,
        out_of_order_action: OutOfOrderAction,
    ) -> Self {
        Self {
            num_bytes: WRAPPER_OVERHEAD,
            num_items: 0,
            streams: HashMap::new(),
            settings,
            partition: None,
            latest_timestamps: None,
            global_timestamps,
            out_of_order_action,
        }
    }

    fn is_full(&self) -> bool {
        self.num_bytes >= self.settings.bytes || self.num_items >= self.settings.events
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
        // We must sort the stream labels to ensure they hash to
        // the same stream if the label set matches.
        item.labels.sort_unstable();

        let partition = &item.partition;
        if self.latest_timestamps.is_none() {
            self.partition = Some(item.partition.clone());
            self.latest_timestamps = Some(self.global_timestamps.take(&partition));
        }
        let latest_timestamp = self
            .latest_timestamps
            .as_ref()
            .unwrap()
            .get(&item.labels)
            .cloned()
            .unwrap_or(item.event.timestamp);
        if item.event.timestamp < latest_timestamp {
            match self.out_of_order_action {
                OutOfOrderAction::Drop => {
                    warn!(
                        msg = "Received out-of-order event; dropping event.",
                        internal_log_rate_secs = 30
                    );
                    return PushResult::Ok(self.is_full());
                }
                OutOfOrderAction::RewriteTimestamp => {
                    warn!(
                        msg = "Received out-of-order event, rewriting timestamp.",
                        internal_log_rate_secs = 30
                    );
                    item.event.timestamp = latest_timestamp;
                }
            }
        }

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
            PushResult::Ok(self.is_full())
        }
    }

    fn is_empty(&self) -> bool {
        self.streams.is_empty()
    }

    fn fresh(&self) -> Self {
        Self::new(
            self.settings,
            self.global_timestamps.clone(),
            self.out_of_order_action.clone(),
        )
    }

    fn finish(self) -> Self::Output {
        let mut latest_timestamps = self.latest_timestamps.expect("Batch is empty");
        let streams_json = self
            .streams
            .into_iter()
            .map(|(labels, mut events)| {
                // Sort events by timestamp
                events.sort_by_key(|e| e.timestamp);

                latest_timestamps.insert(
                    labels.clone(),
                    events.last().expect("Batch is empty").timestamp,
                );

                let labels = labels.into_iter().collect::<HashMap<_, _>>();
                let events = events.into_iter().map(|e| e.encoded).collect::<Vec<_>>();

                (
                    to_raw_value(&labels).expect("JSON encoding should never fail"),
                    events,
                )
            })
            .collect::<Vec<_>>();
        self.global_timestamps
            .insert(self.partition.expect("Bacth is empty"), latest_timestamps);

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
        let mut buffer = LokiBuffer::new(
            BatchSettings::default().size,
            Default::default(),
            Default::default(),
        );
        assert!(matches!(
            buffer.push(LokiRecord {
                partition: PartitionKey { tenant_id: None },
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
        let mut buffer = LokiBuffer::new(
            BatchSettings::default().size,
            Default::default(),
            Default::default(),
        );
        for n in 1..4 {
            assert!(matches!(
                buffer.push(LokiRecord {
                    partition: PartitionKey { tenant_id: None },
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
        let mut buffer = LokiBuffer::new(
            BatchSettings::default().size,
            Default::default(),
            Default::default(),
        );
        for n in 1..4 {
            assert!(matches!(
                buffer.push(LokiRecord {
                    partition: PartitionKey { tenant_id: None },
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
