use futures::TryFutureExt;
use prost::Message;
use tonic::{Request, Response, Status, body::BoxBody};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::decoding::{OtlpDeserializer, format::Deserializer},
    config::LogNamespace,
    event::{BatchNotifier, BatchStatusReceiver, Event},
    internal_event::{CountByteSize, InternalEventHandle as _, Registered},
    opentelemetry::proto::collector::{
        logs::v1::{
            ExportLogsServiceRequest, ExportLogsServiceResponse, logs_service_server::LogsService,
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
    tls::TlsPeerMetadata,
};

use crate::{
    SourceSender,
    internal_events::{EventsReceived, StreamClosedError},
    sources::opentelemetry::{
        config::{LOGS, METRICS, OpentelemetryConfig, TRACES},
        request_control::{AcknowledgementFailure, MiddlewareError, PendingAcknowledgement},
    },
};

pub(crate) fn middleware_error_response(error: MiddlewareError) -> http::Response<BoxBody> {
    Status::unavailable(error.message()).to_http()
}

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
        let tls_peer_metadata = request.extensions().get::<TlsPeerMetadata>().cloned();
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
        let receiver = self
            .handle_events(events, TRACES, tls_peer_metadata)
            .await?;

        Ok(response_with_acknowledgement(
            ExportTraceServiceResponse {
                partial_success: None,
            },
            receiver,
        ))
    }
}

#[tonic::async_trait]
impl LogsService for Service {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        let tls_peer_metadata = request.extensions().get::<TlsPeerMetadata>().cloned();
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
        let receiver = self.handle_events(events, LOGS, tls_peer_metadata).await?;

        Ok(response_with_acknowledgement(
            ExportLogsServiceResponse {
                partial_success: None,
            },
            receiver,
        ))
    }
}

#[tonic::async_trait]
impl MetricsService for Service {
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        let tls_peer_metadata = request.extensions().get::<TlsPeerMetadata>().cloned();
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

        let receiver = self
            .handle_events(events, METRICS, tls_peer_metadata)
            .await?;

        Ok(response_with_acknowledgement(
            ExportMetricsServiceResponse {
                partial_success: None,
            },
            receiver,
        ))
    }
}

impl Service {
    async fn handle_events(
        &self,
        mut events: Vec<Event>,
        log_name: &'static str,
        tls_peer_metadata: Option<TlsPeerMetadata>,
    ) -> Result<Option<BatchStatusReceiver>, Status> {
        if let Some(tls_peer_metadata) = &tls_peer_metadata {
            tls_peer_metadata.insert_into(&mut events, OpentelemetryConfig::NAME);
        }
        // When using OTLP decoding, count individual items within the batch
        // to maintain consistency with other Vector sources
        let count = if self.deserializer.is_some() {
            super::count_otlp_items(&events)
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
            .await?;
        Ok(receiver)
    }
}

fn response_with_acknowledgement<T>(
    message: T,
    receiver: Option<BatchStatusReceiver>,
) -> Response<T> {
    let mut response = Response::new(message);
    if let Some(receiver) = receiver {
        response
            .extensions_mut()
            .insert(PendingAcknowledgement::<BoxBody>::new(
                receiver,
                acknowledgement_failure_response,
            ));
    }
    response
}

fn acknowledgement_failure_response(status: AcknowledgementFailure) -> http::Response<BoxBody> {
    match status {
        AcknowledgementFailure::Errored => Status::internal("Delivery error"),
        AcknowledgementFailure::Rejected => Status::data_loss("Delivery failed"),
    }
    .to_http()
}
