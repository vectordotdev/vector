use std::{collections::BTreeMap, io::Write, sync::Arc};

use bytes::{Bytes};
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
    #[snafu(display("Failed to build the request builder: {}", error_type))]
    FailedToBuild { error_type: &'static str },

    #[snafu(display("Encoding of a request payload failed ({})", reason))]
    FailedToEncode {
        reason: &'static str,
        dropped_events: u64,
    },

    #[snafu(display("A split payload was still too big to encode/compress within size limits"))]
    FailedToSplit { dropped_events: u64 },

    #[snafu(display("An unexpected error occurred"))]
    Unexpected {
        error_type: &'static str,
        dropped_events: u64,
    },
}

impl RequestBuilderError {
    pub const fn into_parts(self) -> (&'static str, u64) {
        match self {
            Self::FailedToBuild { error_type } => (error_type, 0),
            Self::FailedToEncode {
                reason,
                dropped_events,
            } => (reason, dropped_events),
            Self::FailedToSplit { dropped_events } => ("split_failed", dropped_events),
            Self::Unexpected {
                error_type,
                dropped_events,
            } => (error_type, dropped_events),
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
    ) -> Result<Self, RequestBuilderError> {
        Ok(Self {
            api_key,
            endpoint_configuration,
            compression,
            trace_encoder: DatadogTracesEncoder::default(),
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
                results.push(Err(RequestBuilderError::FailedToEncode {
                    reason: "APM stats are not yet supported.",
                    dropped_events: n as u64,
                }))
            }
            DatadogTracesEndpoint::Traces => {
                // Only keep traces
                let mut traces_event = events
                    .into_iter()
                    .filter_map(|e| e.try_into_trace())
                    .collect();
                match self.trace_encoder.encode_trace(&key, &traces_event) {
                    Ok(payload) => {
                        let finalizers = traces_event.take_finalizers();
                        let metadata = RequestMetadata {
                            api_key: key.api_key.take().unwrap_or(Arc::clone(&self.api_key)),
                            batch_size: n,
                            endpoint: key.endpoint,
                            finalizers: finalizers,
                            lang: key.lang,
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
                    Err(err) => results.push(Err(RequestBuilderError::Unexpected {
                        error_type: err.as_error_type(),
                        dropped_events: n as u64,
                    })),
                }
            }
        }
        results
    }

    fn build_request(&mut self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let mut headers = BTreeMap::<String,String>::new();
        headers.insert("Content-Type".to_string(), "application/x-protobuf".to_string());
        headers.insert("DD-API-KEY".to_string(), metadata.api_key.to_string());
        headers.insert(
            "X-Datadog-Reported-Languages".to_string(),
            metadata.lang.unwrap_or("".into()),
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

#[derive(Default)]
pub struct DatadogTracesEncoder {
    max_size: usize,
}

#[derive(Debug, Snafu)]
pub enum EncoderError {
    #[snafu(display("Failed to encode sketch metrics to Protocol Buffers: {}", source))]
    ProtoEncodingFailed { source: prost::EncodeError },

    #[snafu(display("Unable to split payload into small enough chunks"))]
    UnableToSplit,
}

impl EncoderError {
    /// Gets the telemetry-friendly string version of this error.
    ///
    /// The value will be a short string with only lowercase letters and underscores.
    pub const fn as_error_type(&self) -> &'static str {
        match self {
            Self::ProtoEncodingFailed { .. } => "proto encoding failed",
            Self::UnableToSplit { .. } => "unable to split into small chunks",
        }
    }
}
impl DatadogTracesEncoder {
    fn encode_trace(
        &self,
        key: &PartitionKey,
        events: &Vec<TraceEvent>,
    ) -> Result<Vec<u8>, EncoderError> {
        let payload = DatadogTracesEncoder::trace_into_payload(key, events);
        warn!("trace={:?}", payload);
        let encoded_payload = payload.encode_to_vec();
        if encoded_payload.len() > self.max_size {
            warn!("max size exceeded");
            // Todo: attempt to split the processed trace into multiple chunks
        }
        Ok(encoded_payload)
    }

    fn trace_into_payload(key: &PartitionKey, events: &Vec<TraceEvent>) -> dd_proto::TracePayload {
        let mut traces: Vec<dd_proto::ApiTrace> = Vec::new();
        let mut transactions: Vec<dd_proto::Span> = Vec::new();
        for e in events.into_iter() {
            if e.contains("spans") {
                warn!("encoding a trace");
                traces.push(DatadogTracesEncoder::vector_trace_into_dd_trace(&e));
            } else {
                warn!("encoding an APM event ");
                transactions.push(DatadogTracesEncoder::convert_span(e.as_map()));
            }
        }
        dd_proto::TracePayload {
            host_name: key.hostname.clone().unwrap_or("".to_string()),
            env: key.env.clone().unwrap_or("".into()),
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
                m.into_iter()
                    .map(|(k, v)| (k.clone(), v.to_string_lossy()))
                    .collect::<BTreeMap<String, String>>()
            })
            .unwrap_or(BTreeMap::new());

        let metrics = span
            .get("metrics")
            .map(|m| m.as_map())
            .flatten()
            .map(|m| {
                m.into_iter()
                    .filter_map(|(k, v)| {
                        if let Value::Float(f) = v {
                            Some((k.clone(), f.into_inner()))
                        } else {
                            None
                        }
                    })
                    .collect::<BTreeMap<String, f64>>()
            })
            .unwrap_or(BTreeMap::new());

        dd_proto::Span {
            service: span
                .get("service")
                .map(|v| v.to_string_lossy())
                .unwrap_or("".into()),
            name: span
                .get("name")
                .map(|v| v.to_string_lossy())
                .unwrap_or("".into()),
            resource: span
                .get("resource")
                .map(|v| v.to_string_lossy())
                .unwrap_or("".into()),
            r#type: span
                .get("type")
                .map(|v| v.to_string_lossy())
                .unwrap_or("".into()),
            trace_id: trace_id as u64,
            span_id: span_id as u64,
            parent_id: parent_id as u64,
            start: start,
            duration: duration,
            error: error as i32,
            meta,
            metrics,
        }
    }
}
