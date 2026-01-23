use futures::TryFutureExt;
use prost::Message;
use tonic::{Request, Response, Status};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::decoding::{OtlpDeserializer, format::Deserializer},
    config::LogNamespace,
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
    internal_event::{CountByteSize, InternalEventHandle as _, Registered},
    opentelemetry::proto::{
        RESOURCE_LOGS_JSON_FIELD, RESOURCE_METRICS_JSON_FIELD, RESOURCE_SPANS_JSON_FIELD,
        collector::{
            logs::v1::{
                ExportLogsServiceRequest, ExportLogsServiceResponse,
                logs_service_server::LogsService,
            },
            metrics::v1::{
                ExportMetricsServiceRequest, ExportMetricsServiceResponse,
                metrics_service_server::MetricsService,
            },
            trace::v1::{
                ExportTraceServiceRequest, ExportTraceServiceResponse,
                trace_service_server::TraceService,
            },
        },
    },
};

use crate::{
    SourceSender,
    internal_events::{EventsReceived, StreamClosedError},
    sources::opentelemetry::config::{LOGS, METRICS, TRACES},
};

#[derive(Clone)]
pub(super) struct Service {
    pub pipeline: SourceSender,
    pub acknowledgements: bool,
    pub events_received: Registered<EventsReceived>,
    pub log_namespace: LogNamespace,
    pub deserializer: Option<OtlpDeserializer>,
}

#[tonic::async_trait]
impl TraceService for Service {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let events = if let Some(deserializer) = self.deserializer.as_ref() {
            let raw_bytes = request.get_ref().encode_to_vec();
            let bytes = bytes::Bytes::from(raw_bytes);
            deserializer
                .parse(bytes, self.log_namespace)
                .map_err(|e| Status::invalid_argument(e.to_string()))
                .map(|buf| buf.into_vec())?
        } else {
            request
                .into_inner()
                .resource_spans
                .into_iter()
                .flat_map(|v| v.into_event_iter())
                .collect()
        };
        self.handle_events(events, TRACES).await?;

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}

#[tonic::async_trait]
impl LogsService for Service {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        let events = if let Some(deserializer) = self.deserializer.as_ref() {
            let raw_bytes = request.get_ref().encode_to_vec();
            let bytes = bytes::Bytes::from(raw_bytes);
            deserializer
                .parse(bytes, self.log_namespace)
                .map_err(|e| Status::invalid_argument(e.to_string()))
                .map(|buf| buf.into_vec())?
        } else {
            request
                .into_inner()
                .resource_logs
                .into_iter()
                .flat_map(|v| v.into_event_iter(self.log_namespace))
                .collect()
        };
        self.handle_events(events, LOGS).await?;

        Ok(Response::new(ExportLogsServiceResponse {
            partial_success: None,
        }))
    }
}

#[tonic::async_trait]
impl MetricsService for Service {
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        let events = if let Some(deserializer) = self.deserializer.as_ref() {
            let raw_bytes = request.get_ref().encode_to_vec();
            // Major caveat here, the output event will be logs.
            let bytes = bytes::Bytes::from(raw_bytes);
            deserializer
                .parse(bytes, self.log_namespace)
                .map_err(|e| Status::invalid_argument(e.to_string()))
                .map(|buf| buf.into_vec())?
        } else {
            request
                .into_inner()
                .resource_metrics
                .into_iter()
                .flat_map(|v| v.into_event_iter())
                .collect()
        };

        self.handle_events(events, METRICS).await?;

        Ok(Response::new(ExportMetricsServiceResponse {
            partial_success: None,
        }))
    }
}

impl Service {
    async fn handle_events(
        &self,
        mut events: Vec<Event>,
        log_name: &'static str,
    ) -> Result<(), Status> {
        // When using OTLP decoding, count individual items within the batch
        // to maintain consistency with other Vector sources
        let count = if self.deserializer.is_some() {
            count_otlp_items(&events)
        } else {
            events.len()
        };
        let byte_size = events.estimated_json_encoded_size_of();
        self.events_received.emit(CountByteSize(count, byte_size));

        let receiver = BatchNotifier::maybe_apply_to(self.acknowledgements, &mut events);

        self.pipeline
            .clone()
            .send_batch_named(log_name, events)
            .map_err(|error| {
                let message = error.to_string();
                emit!(StreamClosedError { count });
                Status::unavailable(message)
            })
            .and_then(|_| handle_batch_status(receiver))
            .await?;
        Ok(())
    }
}

/// Counts individual log records, metrics, or spans within OTLP batch events.
/// When use_otlp_decoding is enabled, events contain entire OTLP batches, but
/// we want to count the individual items for metric consistency with other sources.
fn count_otlp_items(events: &[Event]) -> usize {
    events
        .iter()
        .map(|event| {
            match event {
                Event::Log(log) => {
                    // Count log records in resourceLogs
                    if let Some(resource_logs) = log.get(RESOURCE_LOGS_JSON_FIELD) {
                        if let Some(resource_logs_array) = resource_logs.as_array() {
                            return resource_logs_array
                                .iter()
                                .map(|rl| {
                                    if let Some(scope_logs) = rl.get("scopeLogs")
                                        && let Some(scope_logs_array) = scope_logs.as_array()
                                    {
                                        return scope_logs_array
                                            .iter()
                                            .map(|sl| {
                                                sl.get("logRecords")
                                                    .and_then(|lr| lr.as_array())
                                                    .map(|arr| arr.len())
                                                    .unwrap_or(0)
                                            })
                                            .sum();
                                    }
                                    0
                                })
                                .sum();
                        }
                    }
                    // Count metrics in resourceMetrics
                    else if let Some(resource_metrics) = log.get(RESOURCE_METRICS_JSON_FIELD)
                        && let Some(resource_metrics_array) = resource_metrics.as_array()
                    {
                        return resource_metrics_array
                            .iter()
                            .map(|rm| {
                                if let Some(scope_metrics) = rm.get("scopeMetrics")
                                    && let Some(scope_metrics_array) = scope_metrics.as_array()
                                {
                                    return scope_metrics_array
                                        .iter()
                                        .map(|sm| {
                                            sm.get("metrics")
                                                .and_then(|m| m.as_array())
                                                .map(|arr| arr.len())
                                                .unwrap_or(0)
                                        })
                                        .sum();
                                }
                                0
                            })
                            .sum();
                    }
                    0
                }
                Event::Trace(trace) => {
                    // Count spans in resourceSpans
                    if let Some(resource_spans) = trace.get(RESOURCE_SPANS_JSON_FIELD)
                        && let Some(resource_spans_array) = resource_spans.as_array()
                    {
                        return resource_spans_array
                            .iter()
                            .map(|rs| {
                                if let Some(scope_spans) = rs.get("scopeSpans")
                                    && let Some(scope_spans_array) = scope_spans.as_array()
                                {
                                    return scope_spans_array
                                        .iter()
                                        .map(|ss| {
                                            ss.get("spans")
                                                .and_then(|s| s.as_array())
                                                .map(|arr| arr.len())
                                                .unwrap_or(0)
                                        })
                                        .sum();
                                }
                                0
                            })
                            .sum();
                    }
                    0
                }
                _ => 0,
            }
        })
        .sum()
}

async fn handle_batch_status(receiver: Option<BatchStatusReceiver>) -> Result<(), Status> {
    let status = match receiver {
        Some(receiver) => receiver.await,
        None => BatchStatus::Delivered,
    };

    match status {
        BatchStatus::Errored => Err(Status::internal("Delivery error")),
        BatchStatus::Rejected => Err(Status::data_loss("Delivery failed")),
        BatchStatus::Delivered => Ok(()),
    }
}
