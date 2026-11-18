//! Bounded aggregation of already serialized log events.

use std::time::{Duration, Instant};
use vector_lib::configurable::configurable_component;

/// Format used to combine original events into a Kinesis record.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// Serialized text or compact JSON separated by newline characters.
    #[cfg_attr(not(feature = "sinks-aws_kinesis_streams"), default)]
    Newline,
    /// Standard KPL protobuf envelope, preserving individual payload boundaries.
    #[cfg(feature = "sinks-aws_kinesis_streams")]
    #[default]
    Kpl,
}

impl Format {
    pub(crate) fn event_len(self, payload: &[u8]) -> usize {
        match self {
            Self::Newline => payload.len() + usize::from(payload.last() != Some(&b'\n')),
            #[cfg(feature = "sinks-aws_kinesis_streams")]
            Self::Kpl => super::kpl::record_len(payload.len()),
        }
    }

    #[cfg_attr(not(feature = "sinks-aws_kinesis_streams"), allow(unused_variables))]
    pub(crate) const fn overhead(self, key_len: usize) -> usize {
        match self {
            Self::Newline => 0,
            #[cfg(feature = "sinks-aws_kinesis_streams")]
            Self::Kpl => super::kpl::overhead(key_len),
        }
    }

    fn append(self, payload: &[u8], output: &mut Vec<u8>) {
        match self {
            Self::Newline => {
                output.extend_from_slice(payload);
                if payload.last() != Some(&b'\n') {
                    output.push(b'\n');
                }
            }
            #[cfg(feature = "sinks-aws_kinesis_streams")]
            Self::Kpl => super::kpl::append_record(payload, output),
        }
    }
}

/// Limits for one aggregate, independent of the `PutRecords` request batch.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub format: Format,
    pub max_events: usize,
    pub max_bytes: usize,
    pub timeout: Duration,
}

/// An aggregate retains one metadata item and byte boundary per original log.
pub struct Aggregate<M> {
    pub payload: Vec<u8>,
    pub partition_key: Option<String>,
    pub items: Vec<M>,
    ends: Vec<usize>,
}

impl<M> Aggregate<M> {
    /// Split only at an original event boundary, retaining ordering and ownership.
    ///
    /// # Panics
    /// Panics if the aggregate contains fewer than two original events.
    #[must_use]
    pub fn split(mut self) -> (Self, Self) {
        assert!(self.items.len() > 1);
        let mid = self.items.len() / 2;
        let offset = self.ends[mid - 1];
        let right = Self {
            payload: self.payload.split_off(offset),
            partition_key: self.partition_key.clone(),
            items: self.items.split_off(mid),
            ends: self
                .ends
                .split_off(mid)
                .into_iter()
                .map(|end| end - offset)
                .collect(),
        };
        (self, right)
    }
}

/// One active aggregate bounds memory even with high-cardinality partition keys.
pub struct Aggregator<M> {
    settings: Settings,
    current: Option<Aggregate<M>>,
    deadline: Option<Instant>,
}

impl<M> Aggregator<M> {
    #[must_use]
    pub const fn new(settings: Settings) -> Self {
        Self {
            settings,
            current: None,
            deadline: None,
        }
    }

    #[must_use]
    pub const fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub const fn flush(&mut self) -> Option<Aggregate<M>> {
        self.deadline = None;
        self.current.take()
    }

    pub fn flush_due(&mut self, now: Instant) -> Option<Aggregate<M>> {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.flush()
        } else {
            None
        }
    }

    /// Buffers an event and returns aggregates completed by it.
    ///
    /// # Panics
    ///
    /// Panics if the configured timeout cannot be added to `now`.
    pub fn push(
        &mut self,
        payload: &[u8],
        key: Option<String>,
        metadata: M,
        now: Instant,
    ) -> Vec<Aggregate<M>> {
        let mut ready = Vec::new();
        let size = self.settings.format.event_len(payload);
        // Random keys contain 16 Unicode scalars, at most 64 UTF-8 bytes.
        let overhead = self
            .settings
            .format
            .overhead(key.as_ref().map_or(64, String::len));
        let must_flush = self.deadline.is_some_and(|deadline| now >= deadline)
            || self.current.as_ref().is_some_and(|batch| {
                batch.partition_key != key
                    || batch
                        .payload
                        .len()
                        .saturating_add(size)
                        .saturating_add(overhead)
                        > self.settings.max_bytes
            });
        if must_flush && let Some(batch) = self.flush() {
            ready.push(batch);
        }
        let batch = self.current.get_or_insert_with(|| {
            self.deadline = Some(now + self.settings.timeout);
            Aggregate {
                payload: Vec::with_capacity(self.settings.max_bytes.min(16_384)),
                partition_key: key,
                items: Vec::new(),
                ends: Vec::new(),
            }
        });
        self.settings.format.append(payload, &mut batch.payload);
        batch.items.push(metadata);
        batch.ends.push(batch.payload.len());
        if batch.items.len() >= self.settings.max_events
            || batch.payload.len().saturating_add(overhead) >= self.settings.max_bytes
        {
            ready.push(self.flush().expect("a batch was just populated"));
        }
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn settings(events: usize, bytes: usize) -> Settings {
        Settings {
            format: Format::Newline,
            max_events: events,
            max_bytes: bytes,
            timeout: Duration::from_secs(1),
        }
    }

    #[test]
    fn joins_exact_lines_and_retains_each_metadata_item() {
        let mut a = Aggregator::new(settings(500, 1000));
        let now = Instant::now();
        assert!(a.push(b"one", None, 11, now).is_empty());
        assert!(a.push(b"two\n", None, 22, now).is_empty());
        let batch = a.flush().unwrap();
        assert_eq!(batch.payload, b"one\ntwo\n");
        assert_eq!(batch.items, vec![11, 22]);
        assert!(a.flush().is_none());
    }

    #[cfg(feature = "sinks-aws_kinesis_streams")]
    #[test]
    fn kpl_byte_target_includes_envelope_and_key_table() {
        let mut a = Aggregator::new(Settings {
            format: Format::Kpl,
            ..settings(500, 40)
        });
        let now = Instant::now();
        assert!(a.push(b"a", Some("key".into()), 1, now).is_empty());
        assert!(a.push(b"b", Some("key".into()), 2, now).is_empty());
        let out = a.push(b"c", Some("key".into()), 3, now);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].items, [1, 2]);
        assert_eq!(
            super::super::kpl::envelope("key", &out[0].payload).len(),
            39
        );
        assert_eq!(a.flush().unwrap().items, [3]);
    }

    #[test]
    fn flushes_on_count_and_eof() {
        let mut a = Aggregator::new(settings(2, 1000));
        let now = Instant::now();
        assert!(a.push(b"a", None, (), now).is_empty());
        assert_eq!(a.push(b"b", None, (), now)[0].payload, b"a\nb\n");
        assert!(a.push(b"c", None, (), now).is_empty());
        assert_eq!(a.flush().unwrap().payload, b"c\n");
    }

    #[test]
    fn byte_target_includes_lf_and_accepts_large_singleton() {
        let now = Instant::now();
        let mut a = Aggregator::new(settings(500, 5));
        a.push(b"aa", None, (), now);
        assert_eq!(a.push(b"bb", None, (), now)[0].payload, b"aa\n");
        assert_eq!(
            a.push(b"123456", None, (), now)
                .iter()
                .map(|x| x.payload.clone())
                .collect::<Vec<_>>(),
            vec![b"bb\n".to_vec(), b"123456\n".to_vec()]
        );
    }

    #[test]
    fn explicit_keys_never_mix_and_retain_order() {
        let now = Instant::now();
        let mut a = Aggregator::new(settings(500, 1000));
        a.push(b"a1", Some("a".into()), (), now);
        let out = a.push(b"b1", Some("b".into()), (), now);
        assert_eq!(out[0].partition_key.as_deref(), Some("a"));
        assert_eq!(out[0].payload, b"a1\n");
        a.push(b"b2", Some("b".into()), (), now);
        assert_eq!(a.flush().unwrap().payload, b"b1\nb2\n");
    }

    #[test]
    fn continuously_ready_input_cannot_extend_first_event_deadline() {
        let now = Instant::now();
        let mut a = Aggregator::new(settings(500, 1000));
        a.push(b"a", None, (), now);
        a.push(b"b", None, (), now + Duration::from_millis(900));
        let out = a.push(b"c", None, (), now + Duration::from_secs(1));
        assert_eq!(out[0].payload, b"a\nb\n");
        assert_eq!(a.flush().unwrap().payload, b"c\n");
    }

    #[test]
    fn idle_deadline_and_empty_state() {
        let now = Instant::now();
        let mut a = Aggregator::new(settings(500, 1000));
        assert!(a.deadline().is_none());
        assert!(a.flush_due(now).is_none());
        a.push(b"a", None, (), now);
        assert!(a.flush_due(now + Duration::from_millis(999)).is_none());
        assert_eq!(
            a.flush_due(now + Duration::from_secs(1)).unwrap().payload,
            b"a\n"
        );
        assert!(a.deadline().is_none());
    }

    #[test]
    fn splitting_keeps_payload_metadata_and_boundaries_together() {
        let now = Instant::now();
        let mut a = Aggregator::new(settings(500, 1000));
        for (i, text) in [b"a".as_slice(), b"bb", b"ccc"].into_iter().enumerate() {
            a.push(text, Some("key".into()), i, now);
        }
        let (left, right) = a.flush().unwrap().split();
        assert_eq!(left.payload, b"a\n");
        assert_eq!(left.items, vec![0]);
        assert_eq!(right.payload, b"bb\nccc\n");
        assert_eq!(right.items, vec![1, 2]);
        let (one, two) = right.split();
        assert_eq!(one.payload, b"bb\n");
        assert_eq!(two.payload, b"ccc\n");
    }
}
