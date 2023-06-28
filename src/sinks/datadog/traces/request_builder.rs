use std::{
    collections::BTreeMap,
    io::Write,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use bytes::Bytes;
use prost::Message;
use snafu::Snafu;
use vector_common::request_metadata::RequestMetadata;
use vector_core::{
    event::{EventFinalizers, Finalizable},
    EstimatedJsonEncodedSizeOf,
};

use super::{
    apm_stats::{compute_apm_stats, Aggregator},
    config::{DatadogTracesEndpoint, DatadogTracesEndpointConfiguration},
    dd_proto,
    service::TraceApiRequest,
    sink::PartitionKey,
};
use crate::{
    event::{Event, TraceEvent, Value},
    sinks::util::{
        metadata::RequestMetadataBuilder, Compression, Compressor, IncrementalRequestBuilder,
    },
};

#[derive(Debug, Snafu)]
pub enum RequestBuilderError {
    #[snafu(display(
        "Building an APM stats request payload failed ({}, {})",
        message,
        reason
    ))]
    FailedToBuild {
        message: &'static str,
        reason: String,
        dropped_events: u64,
    },

    #[snafu(display("Unsupported endpoint ({})", reason))]
    UnsupportedEndpoint { reason: String, dropped_events: u64 },
}

impl RequestBuilderError {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn into_parts(self) -> (&'static str, String, u64) {
        match self {
            Self::FailedToBuild {
                message,
                reason,
                dropped_events,
            } => (message, reason, dropped_events),
            Self::UnsupportedEndpoint {
                reason,
                dropped_events,
            } => ("unsupported endpoint", reason, dropped_events),
        }
    }
}

pub struct DatadogTracesRequestBuilder {
    api_key: Arc<str>,
    endpoint_configuration: DatadogTracesEndpointConfiguration,
    compression: Compression,
    trace_encoder: DatadogTracesEncoder,
    /// Contains the Aggregated stats across a time window.
    stats_aggregator: Arc<Mutex<Aggregator>>,
}

impl DatadogTracesRequestBuilder {
    pub fn new(
        api_key: Arc<str>,
        endpoint_configuration: DatadogTracesEndpointConfiguration,
        compression: Compression,
        max_size: usize,
        stats_aggregator: Arc<Mutex<Aggregator>>,
    ) -> Result<Self, RequestBuilderError> {
        Ok(Self {
            api_key,
            endpoint_configuration,
            compression,
            trace_encoder: DatadogTracesEncoder { max_size },
            stats_aggregator,
        })
    }
}

pub struct DDTracesMetadata {
    pub api_key: Arc<str>,
    pub endpoint: DatadogTracesEndpoint,
    pub finalizers: EventFinalizers,
    pub uncompressed_size: usize,
    pub content_type: String,
}

impl IncrementalRequestBuilder<(PartitionKey, Vec<Event>)> for DatadogTracesRequestBuilder {
    type Metadata = (DDTracesMetadata, RequestMetadata);
    type Payload = Bytes;
    type Request = TraceApiRequest;
    type Error = RequestBuilderError;

    fn encode_events_incremental(
        &mut self,
        input: (PartitionKey, Vec<Event>),
    ) -> Vec<Result<(Self::Metadata, Self::Payload), Self::Error>> {
        let (key, events) = input;
        let mut results = Vec::new();
        let n = events.len();
        let trace_events = events
            .into_iter()
            .filter_map(|e| e.try_into_trace())
            .collect::<Vec<TraceEvent>>();

        // Compute APM stats from the incoming events. The stats payloads are sent out
        // separately from the sink framework, by the thread `flush_apm_stats_thread()`
        compute_apm_stats(&key, Arc::clone(&self.stats_aggregator), &trace_events);

        self.trace_encoder
            .encode_trace(&key, trace_events)
            .into_iter()
            .for_each(|r| match r {
                Ok((payload, mut processed)) => {
                    let uncompressed_size = payload.len();
                    let json_size = processed.estimated_json_encoded_size_of();
                    let metadata = DDTracesMetadata {
                        api_key: key
                            .api_key
                            .clone()
                            .unwrap_or_else(|| Arc::clone(&self.api_key)),
                        endpoint: DatadogTracesEndpoint::Traces,
                        finalizers: processed.take_finalizers(),
                        uncompressed_size,
                        content_type: "application/x-protobuf".to_string(),
                    };

                    let mut compressor = Compressor::from(self.compression);
                    match compressor.write_all(&payload) {
                        Ok(()) => {
                            let bytes = compressor.into_inner().freeze();

                            // build RequestMetadata
                            let builder =
                                RequestMetadataBuilder::new(n, uncompressed_size, json_size);
                            let bytes_len = NonZeroUsize::new(bytes.len())
                                .expect("payload should never be zero length");
                            let request_metadata = builder.with_request_size(bytes_len);

                            results.push(Ok(((metadata, request_metadata), bytes)))
                        }
                        Err(e) => results.push(Err(RequestBuilderError::FailedToBuild {
                            message: "Payload compression failed.",
                            reason: e.to_string(),
                            dropped_events: n as u64,
                        })),
                    }
                }
                Err(err) => results.push(Err(RequestBuilderError::FailedToBuild {
                    message: err.parts().0,
                    reason: err.parts().1.into(),
                    dropped_events: err.parts().2,
                })),
            });
        results
    }

    fn build_request(&mut self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        build_request(
            metadata,
            payload,
            self.compression,
            &self.endpoint_configuration,
        )
    }
}

/// Builds the `TraceApiRequest` from inputs.
///
/// # Arguments
///
/// * `metadata`                 - Tuple of Datadog traces specific metadata and the generic `RequestMetadata`.
/// * `payload`                  - Compressed and encoded bytes to send.
/// * `compression`              - `Compression` used to reference the Content-Encoding header.
/// * `endpoint_configuration`   - Endpoint configuration to use when creating the HTTP requests.
pub fn build_request(
    metadata: (DDTracesMetadata, RequestMetadata),
    payload: Bytes,
    compression: Compression,
    endpoint_configuration: &DatadogTracesEndpointConfiguration,
) -> TraceApiRequest {
    let (ddtraces_metadata, request_metadata) = metadata;
    let mut headers = BTreeMap::<String, String>::new();
    headers.insert("Content-Type".to_string(), ddtraces_metadata.content_type);
    headers.insert(
        "DD-API-KEY".to_string(),
        ddtraces_metadata.api_key.to_string(),
    );
    if let Some(ce) = compression.content_encoding() {
        headers.insert("Content-Encoding".to_string(), ce.to_string());
    }
    TraceApiRequest {
        body: payload,
        headers,
        finalizers: ddtraces_metadata.finalizers,
        uri: endpoint_configuration.get_uri_for_endpoint(ddtraces_metadata.endpoint),
        uncompressed_size: ddtraces_metadata.uncompressed_size,
        metadata: request_metadata,
    }
}

pub struct DatadogTracesEncoder {
    max_size: usize,
}

#[derive(Debug, Snafu)]
pub enum EncoderError {
    #[snafu(display("Unable to split payload into small enough chunks"))]
    UnableToSplit {
        dropped_events: u64,
        error_code: &'static str,
    },
}

impl EncoderError {
    pub const fn parts(&self) -> (&'static str, &'static str, u64) {
        match self {
            Self::UnableToSplit {
                dropped_events: n,
                error_code,
            } => ("unable to split into small chunks", error_code, *n),
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
        // This may happen exceptionally
        if encoded_payload.len() > self.max_size {
            debug!("A payload exceeded the maximum size, splitting into multiple.");
            let n_chunks: usize = (encoded_payload.len() / self.max_size) + 1;
            let chunk_size = (events.len() / n_chunks) + 1;
            events.chunks(chunk_size).for_each(|events| {
                let chunked_payload = DatadogTracesEncoder::trace_into_payload(key, events);
                let encoded_chunk = chunked_payload.encode_to_vec();
                if encoded_chunk.len() > self.max_size {
                    encoded_payloads.push(Err(EncoderError::UnableToSplit {
                        dropped_events: events.len() as u64,
                        error_code: "message_too_big",
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
        dd_proto::TracePayload {
            host_name: key.hostname.clone().unwrap_or_default(),
            env: key.env.clone().unwrap_or_default(),
            traces: vec![],       // Field reserved for the older trace payloads
            transactions: vec![], // Field reserved for the older trace payloads
            tracer_payloads: events
                .iter()
                .map(DatadogTracesEncoder::vector_trace_into_dd_tracer_payload)
                .collect(),
            // We only send tags at the Trace level
            tags: BTreeMap::new(),
            agent_version: key.agent_version.clone().unwrap_or_default(),
            target_tps: key.target_tps.map(|tps| tps as f64).unwrap_or_default(),
            error_tps: key.error_tps.map(|tps| tps as f64).unwrap_or_default(),
        }
    }

    fn vector_trace_into_dd_tracer_payload(trace: &TraceEvent) -> dd_proto::TracerPayload {
        let tags = trace
            .get("tags")
            .and_then(|m| m.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.to_string_lossy().into_owned()))
                    .collect::<BTreeMap<String, String>>()
            })
            .unwrap_or_default();

        let spans = match trace.get("spans") {
            Some(Value::Array(v)) => v
                .iter()
                .filter_map(|s| s.as_object().map(DatadogTracesEncoder::convert_span))
                .collect(),
            _ => vec![],
        };

        let chunk = dd_proto::TraceChunk {
            priority: trace
                .get("priority")
                .and_then(|v| v.as_integer().map(|v| v as i32))
                // This should not happen for Datadog originated traces, but in case this field is not populated
                // we default to 1 (https://github.com/DataDog/datadog-agent/blob/eac2327/pkg/trace/sampler/sampler.go#L54-L55),
                // which is what the Datadog trace-agent is doing for OTLP originated traces, as per
                // https://github.com/DataDog/datadog-agent/blob/3ea2eb4/pkg/trace/api/otlp.go#L309.
                .unwrap_or(1i32),
            origin: trace
                .get("origin")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            dropped_trace: trace
                .get("dropped")
                .and_then(|v| v.as_boolean())
                .unwrap_or(false),
            spans,
            tags: tags.clone(),
        };

        dd_proto::TracerPayload {
            container_id: trace
                .get("container_id")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            language_name: trace
                .get("language_name")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            language_version: trace
                .get("language_version")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            tracer_version: trace
                .get("tracer_version")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            runtime_id: trace
                .get("runtime_id")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            chunks: vec![chunk],
            tags,
            env: trace
                .get("env")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            hostname: trace
                .get("hostname")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            app_version: trace
                .get("app_version")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
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
            .and_then(|m| m.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.to_string_lossy().into_owned()))
                    .collect::<BTreeMap<String, String>>()
            })
            .unwrap_or_default();

        let meta_struct = span
            .get("meta_struct")
            .and_then(|m| m.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.coerce_to_bytes().into_iter().collect()))
                    .collect::<BTreeMap<String, Vec<u8>>>()
            })
            .unwrap_or_default();

        let metrics = span
            .get("metrics")
            .and_then(|m| m.as_object())
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
            .unwrap_or_default();

        dd_proto::Span {
            service: span
                .get("service")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            name: span
                .get("name")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            resource: span
                .get("resource")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            r#type: span
                .get("type")
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default(),
            trace_id: trace_id as u64,
            span_id: span_id as u64,
            parent_id: parent_id as u64,
            error: error as i32,
            start,
            duration,
            meta,
            metrics,
            meta_struct,
        }
    }
}
