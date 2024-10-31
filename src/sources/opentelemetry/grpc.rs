use futures::TryFutureExt;
use tonic::{Request, Response, Status};
use vector_lib::internal_event::{CountByteSize, InternalEventHandle as _, Registered};
use vector_lib::opentelemetry::proto::collector::{
    logs::v1::{
        logs_service_server::LogsService, ExportLogsServiceRequest, ExportLogsServiceResponse,
    },
    trace::v1::{
        trace_service_server::TraceService, ExportTraceServiceRequest, ExportTraceServiceResponse,
    },
};
use vector_lib::{
    config::LogNamespace,
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
    EstimatedJsonEncodedSizeOf,
};

use crate::{
    internal_events::{EventsReceived, StreamClosedError},
    sources::opentelemetry::{LOGS, TRACES},
    SourceSender,
};

#[derive(Clone)]
pub(super) struct Service {
    pub pipeline: SourceSender,
    pub acknowledgements: bool,
    pub events_received: Registered<EventsReceived>,
    pub log_namespace: LogNamespace,
}

#[tonic::async_trait]
impl TraceService for Service {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let events: Vec<Event> = request
            .into_inner()
            .resource_spans
            .into_iter()
            .flat_map(|v| v.into_event_iter())
            .collect();
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
        let events: Vec<Event> = request
            .into_inner()
            .resource_logs
            .into_iter()
            .flat_map(|v| v.into_event_iter(self.log_namespace))
            .collect();
        self.handle_events(events, LOGS).await?;

        Ok(Response::new(ExportLogsServiceResponse {
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
