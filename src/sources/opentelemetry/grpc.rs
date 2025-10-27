use futures::TryFutureExt;
use prost::Message;
use tonic::{Request, Response, Status};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::decoding::{OtlpDeserializer, format::Deserializer},
    config::LogNamespace,
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
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
        let count = events.len();
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
