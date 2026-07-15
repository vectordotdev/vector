use chrono::Utc;
use futures::TryFutureExt;
use tonic::transport::server::RoutesBuilder;
use tonic_health::server::health_reporter;
use vector_lib::EstimatedJsonEncodedSizeOf;
use vector_lib::config::LogNamespace;
use vector_lib::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event};
use vector_lib::internal_event::{CountByteSize, EventsReceived, InternalEventHandle};
use vector_lib::source::Source;
use vector_lib::source_sender::SourceSender;
use vector_lib::tls::MaybeTlsSettings;

use crate::config::SourceContext;
use crate::internal_events::StreamClosedError;
use crate::proto::vector as proto;
use crate::sources::util::grpc::run_grpc_server_with_routes;
use crate::sources::util::decompression::max_decompressed_size_bytes;

#[derive(Debug, Clone)]
struct Service {
    pipeline: SourceSender,
    acknowledgements: bool,
    log_namespace: LogNamespace,
}

#[tonic::async_trait]
impl proto::Service for Service {
    async fn push_events(
        &self,
        request: tonic::Request<proto::PushEventsRequest>,
    ) -> Result<tonic::Response<proto::PushEventsResponse>, tonic::Status> {
        let mut events: Vec<Event> = request
            .into_inner()
            .events
            .into_iter()
            .map(Event::from)
            .collect();

        let now = Utc::now();
        for event in &mut events {
            if let Event::Log(log) = event {
                self.log_namespace.insert_standard_vector_source_metadata(
                    log,
                    super::VectorConfig::NAME,
                    now,
                );
            }
        }

        let count = events.len();
        let byte_size = events.estimated_json_encoded_size_of();
        let events_received = register!(EventsReceived);
        events_received.emit(CountByteSize(count, byte_size));

        let receiver = BatchNotifier::maybe_apply_to(self.acknowledgements, &mut events);

        self.pipeline
            .clone()
            .send_batch(events)
            .map_err(|error| {
                let message = error.to_string();
                emit!(StreamClosedError { count });
                tonic::Status::unavailable(message)
            })
            .and_then(|_| handle_batch_status(receiver))
            .await?;

        Ok(tonic::Response::new(proto::PushEventsResponse {}))
    }

    // TODO: figure out a way to determine if the current Vector instance is "healthy".
    async fn health_check(
        &self,
        _: tonic::Request<proto::HealthCheckRequest>,
    ) -> Result<tonic::Response<proto::HealthCheckResponse>, tonic::Status> {
        let message = proto::HealthCheckResponse {
            status: proto::ServingStatus::Serving.into(),
        };

        Ok(tonic::Response::new(message))
    }

    type PullEventsStream = std::pin::Pin<
        Box<
            dyn tonic::codegen::tokio_stream::Stream<
                    Item = std::result::Result<proto::PullEventsResponse, tonic::Status>,
                > + Send
                + 'static,
        >,
    >;

    async fn pull_events(
        &self,
        _: tonic::Request<proto::PullEventsRequest>,
    ) -> Result<tonic::Response<Self::PullEventsStream>, tonic::Status> {
        Err(tonic::Status::new(
            tonic::Code::Unimplemented,
            "Source vector does not support PullEventsRequest",
        ))
    }
}

async fn handle_batch_status(receiver: Option<BatchStatusReceiver>) -> Result<(), tonic::Status> {
    let status = match receiver {
        Some(receiver) => receiver.await,
        None => BatchStatus::Delivered,
    };

    match status {
        BatchStatus::Errored => Err(tonic::Status::internal("Delivery error")),
        BatchStatus::Rejected => Err(tonic::Status::data_loss("Delivery failed")),
        BatchStatus::Delivered => Ok(()),
    }
}

pub async fn config_to_receive_source(
    config: &super::VectorConfig,
    tls_settings: MaybeTlsSettings,
    cx: SourceContext,
) -> crate::Result<Source> {
    let acknowledgements = cx.do_acknowledgements(config.acknowledgements);
    let log_namespace = cx.log_namespace(config.log_namespace);
    // Create the custom Vector service (existing).
    //
    // Compression negotiation (gzip, zstd) is handled centrally by
    // `DecompressionAndMetricsLayer` in `sources::util::grpc`, so we
    // deliberately do not call `.accept_compressed(..)` here.
    let vector_service = proto::Server::new(Service {
        pipeline: cx.out,
        acknowledgements,
        log_namespace,
    })
    // Tonic added a default of 4MB in 0.9. This replaces the old behavior.
    .max_decoding_message_size(max_decompressed_size_bytes());

    // Create the standard gRPC health service
    let (mut health_reporter, health_service) = health_reporter();

    // Register the Vector service as serving in the health reporter
    health_reporter
        .set_service_status("vector.Vector", tonic_health::ServingStatus::Serving)
        .await;

    // Combine both services using RoutesBuilder
    let mut builder = RoutesBuilder::default();
    builder
        .add_service(health_service)
        .add_service(vector_service);

    let source = run_grpc_server_with_routes(
        config.address,
        tls_settings,
        builder.routes(),
        config.keepalive.clone(),
        cx.shutdown,
    )
    .map_err(|error| {
        error!(message = "Source future failed.", %error);
    });

    Ok(Box::pin(source))
}
