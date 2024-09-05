use std::{collections::HashMap, io};

use crate::sinks::{prelude::*, util::encoding::Encoder};
use bytes::Bytes;
use serde::{ser::SerializeSeq, Serialize};
use vector_lib::config::telemetry;

pub type Labels = Vec<(String, String)>;
pub type StructuredMetadata = Vec<(String, String)>;

#[derive(Clone)]
pub enum LokiBatchEncoding {
    Json,
    Protobuf,
}

#[derive(Clone)]
pub struct LokiBatchEncoder(pub LokiBatchEncoding);

impl Encoder<Vec<LokiRecord>> for LokiBatchEncoder {
    fn encode_input(
        &self,
        input: Vec<LokiRecord>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let count = input.len();
        let mut byte_size = telemetry().create_request_count_byte_size();
        for event in &input {
            byte_size.add_event(event, event.estimated_json_encoded_size_of());
        }

        let batch = LokiBatch::from(input);
        let body = match self.0 {
            LokiBatchEncoding::Json => {
                let streams: Vec<LokiStream> = batch.stream_by_labels.into_values().collect();
                let body = serde_json::json!({ "streams": streams });
                serde_json::to_vec(&body)?
            }
            LokiBatchEncoding::Protobuf => {
                let streams = batch.stream_by_labels.into_values();
                let batch = loki_logproto::util::Batch(
                    streams
                        .map(|stream| {
                            let labels = stream.stream;
                            let entries = stream
                                .values
                                .iter()
                                .map(|event| {
                                    loki_logproto::util::Entry(
                                        event.timestamp,
                                        String::from_utf8_lossy(&event.event).into_owned(),
                                        event.structured_metadata.clone(),
                                    )
                                })
                                .collect();
                            loki_logproto::util::Stream(labels, entries)
                        })
                        .collect(),
                );
                batch.encode()
            }
        };
        write_all(writer, count, &body).map(|()| (body.len(), byte_size))
    }
}

#[derive(Debug, Default, Serialize)]
pub struct LokiBatch {
    stream_by_labels: HashMap<String, LokiStream>,
    #[serde(skip)]
    finalizers: EventFinalizers,
}

#[derive(Debug, Default, Serialize)]
pub struct LokiStream {
    stream: HashMap<String, String>,
    values: Vec<LokiEvent>,
}

impl From<Vec<LokiRecord>> for LokiBatch {
    fn from(events: Vec<LokiRecord>) -> Self {
        let mut result = events
            .into_iter()
            .fold(Self::default(), |mut res, mut item| {
                res.finalizers.merge(item.take_finalizers());
                item.labels.sort();
                // Convert a HashMap of keys and values into a string in the
                // format "k1,v1,k2,v2,". If any of the keys or values contain
                // a comma, it escapes the comma by adding a backslash before
                // it (e.g. "val,ue" becomes "val\,ue").
                let labels: String = item
                    .labels
                    .iter()
                    .flat_map(|(a, b)| [a, b])
                    .map(|s| {
                        let mut escaped: String = s
                            .chars()
                            .map(|c| match c {
                                '\\' => "\\\\".to_string(),
                                ',' => "\\,".to_string(),
                                c => c.to_string(),
                            })
                            .collect();
                        escaped.push(',');
                        escaped
                    })
                    .collect();
                if !res.stream_by_labels.contains_key(&labels) {
                    res.stream_by_labels.insert(
                        labels.clone(),
                        LokiStream {
                            stream: item.labels.into_iter().collect(),
                            values: Vec::new(),
                        },
                    );
                }
                let stream = res
                    .stream_by_labels
                    .get_mut(&labels)
                    .expect("stream must exist");
                stream.values.push(item.event);
                res
            });
        for (_k, stream) in result.stream_by_labels.iter_mut() {
            stream.values.sort_by_key(|e| e.timestamp);
        }
        result
    }
}

#[derive(Clone, Debug)]
pub struct LokiEvent {
    pub timestamp: i64,
    pub event: Bytes,
    pub structured_metadata: StructuredMetadata,
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
    pub json_byte_size: JsonSize,
    pub finalizers: EventFinalizers,
    pub event_count_tags: TaggedEventsSent,
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
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        self.json_byte_size
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

impl GetEventCountTags for LokiRecord {
    fn get_tags(&self) -> TaggedEventsSent {
        self.event_count_tags.clone()
    }
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct PartitionKey {
    pub tenant_id: Option<String>,
}

impl ByteSizeOf for PartitionKey {
    fn allocated_bytes(&self) -> usize {
        self.tenant_id
            .as_ref()
            .map(|value| value.allocated_bytes())
            .unwrap_or(0)
    }
}
