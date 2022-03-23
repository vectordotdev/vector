use std::{collections::HashMap, io};

use serde::{ser::SerializeSeq, Serialize};
use vector_core::{
    event::{EventFinalizers, Finalizable},
    ByteSizeOf,
};

use crate::sinks::util::encoding::Encoder;

pub type Labels = Vec<(String, String)>;

#[derive(Clone, Default)]
pub struct LokiBatchEncoder;

impl Encoder<Vec<LokiRecord>> for LokiBatchEncoder {
    fn encode_input(
        &self,
        input: Vec<LokiRecord>,
        writer: &mut dyn io::Write,
    ) -> io::Result<usize> {
        let batch = LokiBatch::from(input);
        let body = serde_json::json!({ "streams": [batch] });
        let body = serde_json::to_vec(&body)?;
        writer.write(&body)
    }
}

#[derive(Debug, Default, Serialize)]
pub struct LokiBatch {
    stream: HashMap<String, String>,
    values: Vec<LokiEvent>,
    #[serde(skip)]
    finalizers: EventFinalizers,
}

impl From<Vec<LokiRecord>> for LokiBatch {
    fn from(events: Vec<LokiRecord>) -> Self {
        let mut result = events
            .into_iter()
            .fold(Self::default(), |mut res, mut item| {
                res.finalizers.merge(item.take_finalizers());
                res.stream.extend(item.labels.into_iter());
                res.values.push(item.event);
                res
            });
        result.values.sort_by_key(|e| e.timestamp);
        result
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
    pub finalizers: EventFinalizers,
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

impl Finalizable for LokiRecord {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
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
