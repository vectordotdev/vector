//! Special case Loki sink batch buffer
//!
//! This buffer handles stream merging -- when a record is inserted into
//! the buffer, all records having the same stream label set are grouped
//! together for more efficient output.

use dashmap::DashMap;
use serde::{ser::SerializeSeq, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use vector_core::ByteSizeOf;

pub type Labels = Vec<(String, String)>;

#[derive(Debug, Default, Serialize)]
pub struct LokiBatch {
    stream: HashMap<String, String>,
    values: Vec<LokiEvent>,
}

impl From<Vec<LokiRecord>> for LokiBatch {
    fn from(events: Vec<LokiRecord>) -> Self {
        events.into_iter().fold(Self::default(), |mut res, item| {
            res.stream.extend(item.labels.into_iter());
            res.values.push(item.event);
            res
        })
    }
}

#[derive(Clone, Debug)]
pub struct LokiEvent {
    pub timestamp: i64,
    pub event: String,
}

impl ByteSizeOf for LokiEvent {
    fn allocated_bytes(&self) -> usize {
        self.timestamp.allocated_bytes() + self.event.allocated_bytes()
    }
}

impl Serialize for LokiEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(2))?;
        seq.serialize_element(&self.timestamp.to_string())?;
        seq.serialize_element(&self.event)?;
        seq.end()
    }
}

#[derive(Clone, Debug)]
pub struct LokiRecord {
    pub partition: PartitionKey,
    pub labels: Labels,
    pub event: LokiEvent,
}

impl ByteSizeOf for LokiRecord {
    fn allocated_bytes(&self) -> usize {
        self.partition.allocated_bytes()
            + self.labels.iter().fold(0, |res, item| {
                res + item.0.allocated_bytes() + item.1.allocated_bytes()
            })
            + self.event.allocated_bytes()
    }
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct PartitionKey {
    pub tenant_id: Option<String>,
    labels: String,
}

impl ByteSizeOf for PartitionKey {
    fn allocated_bytes(&self) -> usize {
        self.tenant_id
            .as_ref()
            .map(|value| value.allocated_bytes())
            .unwrap_or(0)
            + self.labels.allocated_bytes()
    }
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
