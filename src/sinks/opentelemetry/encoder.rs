#![allow(unused_imports)]
#![allow(warnings)]
use std::{collections::HashMap, io};

use bytes::BytesMut;
use serde_json::{json, to_vec, Map};
use vector_lib::lookup::lookup_v2::ConfigValuePath;
use vrl::event_path;
use vrl::path::PathPrefix;

use base64::prelude::*;

use std::time::SystemTime;

use crate::{
    config::Resource,
    event::{LogEvent, TraceEvent},
    sinks::{prelude::*, util::encoding::Encoder as SinkEncoder},
    template::TemplateRenderingError,
};

use vector_lib::opentelemetry::proto::collector::logs::v1::ExportLogsServiceRequest;
use vector_lib::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;
use vector_lib::opentelemetry::proto::common::v1::KeyValueList;
use vector_lib::opentelemetry::proto::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
use vector_lib::opentelemetry::proto::trace::v1::ResourceSpans;

use prost::Message;

#[derive(Clone, Debug)]
pub(super) struct OpentelemetryEncoder {
    transformer: Transformer,
}

impl OpentelemetryEncoder {
    /// Creates a new `OpentelemetryEncoder`.
    pub(super) const fn new(transformer: Transformer) -> Self {
        Self { transformer }
    }

    fn encode_trace(&self, mut trace: TraceEvent) -> ResourceSpans {
        ResourceSpans::default()
    }

    /// Encode a log event into an OpenTelemetry `ResourceLogs` message.
    ///
    /// Log Events must match the OpenTelemetry log record format:
    ///
    /// body
    /// attributes
    /// resource
    /// trace_id
    /// ...
    fn encode_log(&self, mut log: LogEvent) -> ResourceLogs {
        let mut log_record = LogRecord::default();

        if let Some(msg) = log.get_message() {
            log_record.body = Some(msg.clone().into());
        }

        if let Some(Value::Timestamp(timestamp)) = log.get_timestamp() {
            log_record.time_unix_nano = timestamp.timestamp_nanos_opt().unwrap_or(0) as u64;
        }

        if let Some(Value::Timestamp(timestamp)) = log.remove(event_path!("observed_timestamp")) {
            log_record.observed_time_unix_nano =
                timestamp.timestamp_nanos_opt().unwrap_or(0) as u64;
        }

        if let Some(attrs) = log.remove(event_path!("attributes")) {
            match attrs {
                Value::Object(map) => {
                    log_record.attributes = Into::<KeyValueList>::into(map).into();
                }
                _ => {} /* TODO: how to handle? */
            }
        }

        if let Some(trace_id) = log.remove(event_path!("trace_id")) {
            match trace_id {
                Value::Bytes(id) => {
                    log_record.trace_id = id.into();
                }
                _ => {} /* TODO: how to handle? */
            }
        }

        let mut scope_logs = ScopeLogs::default();
        scope_logs.log_records = vec![log_record];
        let mut resource_logs = ResourceLogs::default();
        resource_logs.scope_logs = vec![scope_logs];

        if let Some(attrs) = log.remove(event_path!("resource")) {
            match attrs {
                Value::Object(map) => {
                    let mut resource =
                        ::vector_lib::opentelemetry::proto::resource::v1::Resource::default();
                    resource.attributes = Into::<KeyValueList>::into(map).into();

                    resource_logs.resource = Some(resource);
                }
                _ => {} /* TODO: how to handle? */
            }
        }

        resource_logs
    }
}

impl SinkEncoder<Vec<Event>> for OpentelemetryEncoder {
    fn encode_input(
        &self,
        mut events: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();
        let mut n_events = events.len();

        for event in events.iter_mut() {
            self.transformer.transform(event);
            byte_size.add_event(event, event.estimated_json_encoded_size_of());
        }

        // TODO: will there always be at least one event?
        let ty = events.get(0).unwrap();

        let payload = match ty {
            Event::Log(_) => {
                let logs = events
                    .into_iter()
                    .map(|e| self.encode_log(e.into_log()))
                    .collect();
                let payload = ExportLogsServiceRequest {
                    resource_logs: logs,
                };
                payload.encode_to_vec()
            }
            Event::Trace(_) => {
                // TODO: can we get all spans for a trace into one event?
                let spans = events
                    .into_iter()
                    .map(|e| self.encode_trace(e.into_trace()))
                    .collect();
                let payload = ExportTraceServiceRequest {
                    resource_spans: spans,
                };
                payload.encode_to_vec()
            }
            Event::Metric(_) => {
                unreachable!()
            }
        };

        write_all(writer, n_events, &payload).map(|()| (payload.len(), byte_size))
    }
}
