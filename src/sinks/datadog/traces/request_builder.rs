use std::{collections::BTreeMap, io::Write, sync::Arc};

use bytes::Bytes;
use prost::Message;
use snafu::Snafu;
use vector_core::event::EventFinalizers;

use super::{
    config::{DatadogTracesEndpoint, DatadogTracesEndpointConfiguration},
    service::TraceApiRequest,
};
use crate::{
    event::{Event, TraceEvent, Value},
    sinks::{
        datadog::traces::sink::PartitionKey,
        util::{Compression, Compressor, IncrementalRequestBuilder},
    },
    vector_core::event::Finalizable,
};
mod dd_proto {
    include!(concat!(env!("OUT_DIR"), "/dd_trace.rs"));
}

#[derive(Debug, Snafu)]
pub enum RequestBuilderError {
    #[snafu(display("Encoding of a request payload failed ({})", reason))]
    FailedToEncode {
        reason: &'static str,
        dropped_events: u64,
    },

    #[snafu(display("Unsupported endpoint ({})", reason))]
    UnsupportedEndpoint {
        reason: &'static str,
        dropped_events: u64,
    },
}

impl RequestBuilderError {
    pub const fn into_parts(self) -> (&'static str, u64) {
        match self {
            Self::FailedToEncode {
                reason,
                dropped_events,
            } => (reason, dropped_events),
            Self::UnsupportedEndpoint {
                reason,
                dropped_events,
            } => (reason, dropped_events),
        }
    }
}

pub struct DatadogTracesRequestBuilder {
    api_key: Arc<str>,
    endpoint_configuration: DatadogTracesEndpointConfiguration,
    compression: Compression,
    trace_encoder: DatadogTracesEncoder,
}

impl DatadogTracesRequestBuilder {
    pub fn new(
        api_key: Arc<str>,
        endpoint_configuration: DatadogTracesEndpointConfiguration,
        compression: Compression,
        max_size: usize,
    ) -> Result<Self, RequestBuilderError> {
        Ok(Self {
            api_key,
            endpoint_configuration,
            compression,
            trace_encoder: DatadogTracesEncoder { max_size },
        })
    }
}

pub struct RequestMetadata {
    api_key: Arc<str>,
    batch_size: usize,
    endpoint: DatadogTracesEndpoint,
    finalizers: EventFinalizers,
    lang: Option<String>,
}

impl IncrementalRequestBuilder<(PartitionKey, Vec<Event>)> for DatadogTracesRequestBuilder {
    type Metadata = RequestMetadata;
    type Payload = Bytes;
    type Request = TraceApiRequest;
    type Error = RequestBuilderError;

    fn encode_events_incremental(
        &mut self,
        input: (PartitionKey, Vec<Event>),
    ) -> Vec<Result<(Self::Metadata, Self::Payload), Self::Error>> {
        let (mut key, events) = input;
        let mut results = Vec::new();
        let n = events.len();
        match key.endpoint {
            DatadogTracesEndpoint::APMStats => {
                results.push(Err(RequestBuilderError::UnsupportedEndpoint {
                    reason: "APM stats are not yet supported.",
                    dropped_events: n as u64,
                }))
            }
            DatadogTracesEndpoint::Traces => {
                let traces_event = events
                    .into_iter()
                    .filter_map(|e| e.try_into_trace())
                    .collect();
                self.trace_encoder
                    .encode_trace(&key, traces_event)
                    .into_iter()
                    .for_each(|r| match r {
                        Ok((payload, mut processed)) => {
                            let metadata = RequestMetadata {
                                api_key: key
                                    .api_key
                                    .take()
                                    .unwrap_or_else(|| Arc::clone(&self.api_key)),
                                batch_size: n,
                                lang: key.lang.clone(),
                                endpoint: key.endpoint,
                                finalizers: processed.take_finalizers(),
                            };
                            let mut compressor = Compressor::from(self.compression);
                            match compressor.write_all(&payload) {
                                Ok(()) => {
                                    results.push(Ok((metadata, compressor.into_inner().freeze())))
                                }
                                Err(_) => results.push(Err(RequestBuilderError::FailedToEncode {
                                    reason: "Payload compression failed.",
                                    dropped_events: n as u64,
                                })),
                            }
                        }
                        Err(err) => results.push(Err(RequestBuilderError::FailedToEncode {
                            reason: err.parts().0,
                            dropped_events: err.parts().1,
                        })),
                    })
            }
        }
        results
    }

    fn build_request(&mut self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let mut headers = BTreeMap::<String, String>::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/x-protobuf".to_string(),
        );
        headers.insert("DD-API-KEY".to_string(), metadata.api_key.to_string());
        headers.insert(
            "X-Datadog-Reported-Languages".to_string(),
            metadata.lang.unwrap_or_else(|| "".into()),
        );
        if let Some(ce) = self.compression.content_encoding() {
            headers.insert("Content-Encoding".to_string(), ce.to_string());
        }
        TraceApiRequest {
            batch_size: metadata.batch_size,
            body: payload,
            headers,
            finalizers: metadata.finalizers,
            uri: self
                .endpoint_configuration
                .get_uri_for_endpoint(metadata.endpoint),
        }
    }
}

pub struct DatadogTracesEncoder {
    max_size: usize,
}

#[derive(Debug, Snafu)]
pub enum EncoderError {
    #[snafu(display("Unable to split payload into small enough chunks"))]
    UnableToSplit { dropped_events: u64 },
}

impl EncoderError {
    pub const fn parts(&self) -> (&'static str, u64) {
        match self {
            Self::UnableToSplit { dropped_events: n } => ("unable to split into small chunks", *n),
        }
    }
}
impl DatadogTracesEncoder {
    fn encode_trace(
        &self,
        key: &PartitionKey,
        events: Vec<TraceEvent>,
    ) -> Vec<Result<(Vec<u8>, Vec<TraceEvent>), EncoderError>> {
        let mut encoded_payloads = Vec::new();
        let payload = DatadogTracesEncoder::trace_into_payload(key, &events);
        let encoded_payload = payload.encode_to_vec();
        // This may happen exceptionnally
        if encoded_payload.len() > self.max_size {
            warn!("Maximum payload size exceeded.");
            let n_chunks: usize = (encoded_payload.len() / self.max_size) + 1;
            let chunk_size = (events.len() / n_chunks) + 1;
            events.chunks(chunk_size).for_each(|events| {
                let chunked_payload = DatadogTracesEncoder::trace_into_payload(key, events);
                let encoded_chunk = chunked_payload.encode_to_vec();
                if encoded_chunk.len() > self.max_size {
                    encoded_payloads.push(Err(EncoderError::UnableToSplit {
                        dropped_events: events.len() as u64,
                    }));
                } else {
                    encoded_payloads.push(Ok((encoded_chunk, events.to_vec())));
                }
            })
        } else {
            encoded_payloads.push(Ok((encoded_payload, events)));
        }
        encoded_payloads
    }

    fn trace_into_payload(key: &PartitionKey, events: &[TraceEvent]) -> dd_proto::TracePayload {
        let mut traces: Vec<dd_proto::ApiTrace> = Vec::new();
        let mut transactions: Vec<dd_proto::Span> = Vec::new();
        for e in events.iter() {
            if e.contains("spans") {
                trace!("Encoding a trace.");
                traces.push(DatadogTracesEncoder::vector_trace_into_dd_trace(e));
            } else {
                trace!("Encoding an APM event.");
                transactions.push(DatadogTracesEncoder::convert_span(e.as_map()));
            }
        }
        dd_proto::TracePayload {
            host_name: key.hostname.clone().unwrap_or_else(|| "".to_string()),
            env: key.env.clone().unwrap_or_else(|| "".into()),
            traces,
            transactions,
        }
    }

    fn vector_trace_into_dd_trace(trace: &TraceEvent) -> dd_proto::ApiTrace {
        let trace_id = match trace.get("trace_id") {
            Some(Value::Integer(val)) => *val,
            _ => 0,
        };
        let start_time = match trace.get("start_time") {
            Some(Value::Timestamp(val)) => val.timestamp_nanos(),
            _ => 0,
        };
        let end_time = match trace.get("end_time") {
            Some(Value::Timestamp(val)) => val.timestamp_nanos(),
            _ => 0,
        };

        let spans = match trace.get("spans") {
            Some(Value::Array(v)) => v
                .iter()
                .filter_map(|s| s.as_map().map(DatadogTracesEncoder::convert_span))
                .collect(),
            _ => vec![],
        };

        dd_proto::ApiTrace {
            trace_id: trace_id as u64,
            spans,
            start_time,
            end_time,
        }
    }

    fn convert_span(span: &BTreeMap<String, Value>) -> dd_proto::Span {
        let trace_id = match span.get("trace_id") {
            Some(Value::Integer(val)) => *val,
            _ => 0,
        };
        let span_id = match span.get("span_id") {
            Some(Value::Integer(val)) => *val,
            _ => 0,
        };
        let parent_id = match span.get("parent_id") {
            Some(Value::Integer(val)) => *val,
            _ => 0,
        };
        let duration = match span.get("duration") {
            Some(Value::Integer(val)) => *val,
            _ => 0,
        };
        let error = match span.get("error") {
            Some(Value::Integer(val)) => *val,
            _ => 0,
        };
        let start = match span.get("start") {
            Some(Value::Timestamp(val)) => val.timestamp_nanos(),
            _ => 0,
        };

        let meta = span
            .get("meta")
            .map(|m| m.as_map())
            .flatten()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.to_string_lossy()))
                    .collect::<BTreeMap<String, String>>()
            })
            .unwrap_or_else(BTreeMap::new);

        let metrics = span
            .get("metrics")
            .map(|m| m.as_map())
            .flatten()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| {
                        if let Value::Float(f) = v {
                            Some((k.clone(), f.into_inner()))
                        } else {
                            None
                        }
                    })
                    .collect::<BTreeMap<String, f64>>()
            })
            .unwrap_or_else(BTreeMap::new);

        dd_proto::Span {
            service: span
                .get("service")
                .map(|v| v.to_string_lossy())
                .unwrap_or_else(|| "".into()),
            name: span
                .get("name")
                .map(|v| v.to_string_lossy())
                .unwrap_or_else(|| "".into()),
            resource: span
                .get("resource")
                .map(|v| v.to_string_lossy())
                .unwrap_or_else(|| "".into()),
            r#type: span
                .get("type")
                .map(|v| v.to_string_lossy())
                .unwrap_or_else(|| "".into()),
            trace_id: trace_id as u64,
            span_id: span_id as u64,
            parent_id: parent_id as u64,
            error: error as i32,
            start,
            duration,
            meta,
            metrics,
        }
    }
}
