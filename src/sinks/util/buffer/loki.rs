//! Special case Loki sink batch buffer
//!
//! This buffer handles stream merging -- when a record is inserted into
//! the buffer, all records having the same stream label set are grouped
//! together for more efficient output.

use super::{
    err_event_too_large, json::BoxedRawValue, Batch, BatchConfig, BatchError, BatchSettings,
    BatchSize, PushResult,
};
use crate::{
    internal_events::{LokiOutOfOrderEventDropped, LokiOutOfOrderEventRewritten, LokiUniqueStream},
    sinks::loki::OutOfOrderAction,
};
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
    labels: String,
}

impl PartitionKey {
    pub fn new(tenant_id: Option<String>, labels: &mut Labels) -> Self {
        // Let's join all of the labels to single string so that
        // cloning requires only single allocation.
        // That requires sorting to ensure uniqueness, but
        // also choosing a separator that isn't likely to be
        // used in either name or value.
        labels.sort();
        PartitionKey {
            tenant_id,
            labels: labels.iter().flat_map(|(a, b)| [a, "→", b, "∇"]).collect(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct GlobalTimestamps {
    map: Arc<DashMap<PartitionKey, i64>>,
}

impl GlobalTimestamps {
    pub fn take(&self, partition: &PartitionKey) -> Option<i64> {
        self.map.remove(partition).map(|(_k, v)| v)
    }

    pub fn insert(&self, partition: PartitionKey, timestamp: i64) {
        self.map.insert(partition, timestamp);
    }
}

#[derive(Debug)]
pub struct LokiBuffer {
    num_bytes: usize,
    num_items: usize,
    stream: Vec<LokiEncodedEvent>,
    settings: BatchSize<Self>,

    partition: Option<(PartitionKey, Labels)>,
    latest_timestamp: Option<Option<i64>>,
    global_timestamps: GlobalTimestamps,
    out_of_order_action: OutOfOrderAction,
}

impl LokiBuffer {
    pub const fn new(
        settings: BatchSize<Self>,
        global_timestamps: GlobalTimestamps,
        out_of_order_action: OutOfOrderAction,
    ) -> Self {
        Self {
            num_bytes: WRAPPER_OVERHEAD,
            num_items: 0,
            stream: Vec::new(),
            settings,
            partition: None,
            latest_timestamp: None,
            global_timestamps,
            out_of_order_action,
        }
    }

    const fn is_full(&self) -> bool {
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
        Ok(config.get_settings_or_default(defaults))
    }

    fn push(&mut self, mut item: Self::Input) -> PushResult<Self::Input> {
        // We must sort the stream labels to ensure they hash to
        // the same stream if the label set matches.
        item.labels.sort_unstable();

        let partition = &item.partition;
        if self.latest_timestamp.is_none() {
            self.partition = Some((item.partition.clone(), item.labels.clone()));
            self.latest_timestamp = Some(self.global_timestamps.take(partition));
        }
        // TODO: gauge/count of labels.
        let latest_timestamp = self
            .latest_timestamp
            .unwrap()
            .unwrap_or(item.event.timestamp);
        if item.event.timestamp < latest_timestamp {
            match self.out_of_order_action {
                OutOfOrderAction::Drop => {
                    emit!(&LokiOutOfOrderEventDropped);
                    return PushResult::Ok(self.is_full());
                }
                OutOfOrderAction::RewriteTimestamp => {
                    emit!(&LokiOutOfOrderEventRewritten);
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
            err_event_too_large(
                WRAPPER_OVERHEAD + labels_len + event_len,
                self.settings.bytes,
            )
        } else if self.num_items >= self.settings.events
            || self.num_bytes + event_len + 1 > self.settings.bytes
        {
            PushResult::Overflow(item)
        } else {
            let new_bytes = match self.stream.is_empty() {
                // Event was already added, and we checked the size, just add it
                false => {
                    self.stream.push(event);
                    event_len + 1
                }
                true => {
                    // Have to verify label size doesn't cause overflow
                    let new_bytes = labels_len + event_len;
                    if self.num_bytes + new_bytes > self.settings.bytes {
                        return PushResult::Overflow(item);
                    } else {
                        self.stream.push(event);
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
        self.stream.is_empty()
    }

    fn fresh(&self) -> Self {
        Self::new(
            self.settings,
            self.global_timestamps.clone(),
            self.out_of_order_action.clone(),
        )
    }

    fn finish(self) -> Self::Output {
        let (partition, labels) = self.partition.expect("Batch is empty");

        let mut events = self.stream;
        // Sort events by timestamp
        events.sort_by_key(|e| e.timestamp);

        let latest_timestamp = events.last().expect("Batch is empty").timestamp;
        let events = events.into_iter().map(|e| e.encoded).collect::<Vec<_>>();

        let labels = labels
            .iter()
            .map(|&(ref key, ref value)| (key, value))
            .collect::<HashMap<_, _>>();
        let stream = to_raw_value(&labels).expect("JSON encoding should never fail");

        self.global_timestamps.insert(partition, latest_timestamp);
        if self.latest_timestamp == Some(None) {
            emit!(&LokiUniqueStream);
        }

        json!({
            "streams": vec![json!({
                "stream": stream,
                "values": events,
            })]
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
                partition: PartitionKey::new(None, &mut vec![("label1".into(), "value1".into())]),
                labels: vec![("label1".into(), "value1".into())],
                event: LokiEvent {
                    timestamp: 123456789,
                    event: "this is an event".into(),
                },
            }),
            PushResult::Ok(false)
        ));

        assert_eq!(buffer.num_items, 1);
        assert!(!buffer.stream.is_empty());
        test_finish(
            buffer,
            r#"{"streams":[{"stream":{"label1":"value1"},"values":[["123456789","this is an event"]]}]}"#,
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
                    partition: PartitionKey::new(None, &mut vec![("asdf".into(), "value1".into())]),
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
        assert!(!buffer.stream.is_empty());
        test_finish(
            buffer,
            r#"{"streams":[{"stream":{"asdf":"value1"},"values":[["123456781","event #1"],["123456782","event #2"],["123456783","event #3"]]}]}"#,
        );
    }
}
