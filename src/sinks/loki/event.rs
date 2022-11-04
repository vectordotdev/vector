use std::{collections::HashMap, io};

use bytes::Bytes;
use serde::{ser::SerializeSeq, Serialize};
use vector_buffers::EventCount;
use vector_core::{
    event::{EventFinalizers, Finalizable},
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
};

use crate::sinks::util::encoding::{write_all, Encoder};

use super::sink::LokiRecords;

pub type Labels = Vec<(String, String)>;

#[derive(Clone)]
pub enum LokiBatchEncoding {
    Json,
    Protobuf,
}

#[derive(Clone)]
pub struct LokiBatchEncoder(pub LokiBatchEncoding);

impl Encoder<LokiRecords> for LokiBatchEncoder {
    fn encode_input(&self, input: LokiRecords, writer: &mut dyn io::Write) -> io::Result<usize> {
        let count = input.0.len();
        let batch = LokiBatch::from(input);
        let body = match self.0 {
            LokiBatchEncoding::Json => {
                let body = serde_json::json!({ "streams": [batch] });
                serde_json::to_vec(&body)?
            }
            LokiBatchEncoding::Protobuf => {
                let labels = batch.stream;
                let entries = batch
                    .values
                    .iter()
                    .map(|event| {
                        loki_logproto::util::Entry(
                            event.timestamp,
                            String::from_utf8_lossy(&event.event).into_owned(),
                        )
                    })
                    .collect();
                let batch = loki_logproto::util::Batch(labels, entries);
                batch.encode()
            }
        };
        write_all(writer, count, &body).map(|()| body.len())
    }
}

#[derive(Debug, Default, Serialize)]
pub struct LokiBatch {
    stream: HashMap<String, String>,
    values: Vec<LokiEvent>,
    #[serde(skip)]
    finalizers: EventFinalizers,
}

impl From<LokiRecords> for LokiBatch {
    fn from(events: LokiRecords) -> Self {
        let mut result = events
            .0
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
    pub event: Bytes,
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
        let event = String::from_utf8_lossy(&self.event);
        seq.serialize_element(&event)?;
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

impl EstimatedJsonEncodedSizeOf for LokiRecord {
    fn estimated_json_encoded_size_of(&self) -> usize {
        self.event.estimated_json_encoded_size_of()
    }
}

impl EventCount for LokiRecord {
    fn event_count(&self) -> usize {
        // A Loki record is mapped one-to-one with an event.
        1
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
