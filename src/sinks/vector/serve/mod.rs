use crate::{proto::vector as proto, sinks::util::SinkBuilderExt};
use futures::{TryFutureExt, stream::BoxStream};
use prost::Message;
use stream_cancel::Trigger;
use vector_lib::{
    ByteSizeOf,
    event::{EventFinalizers, Finalizable},
    shutdown::ShutdownSignal,
    stream::{BatcherSettings, batcher::data::BatchReduce},
};

#[derive(Debug, Clone)]
pub struct ServeService {
    pub emitter: tokio::sync::broadcast::Sender<Vec<crate::event::proto::EventWrapper>>,
    pub protocol: &'static str,
    pub endpoint: &'static str,
}

#[tonic::async_trait]
impl proto::Service for ServeService {
    async fn push_events(
        &self,
        _: tonic::Request<proto::PushEventsRequest>,
    ) -> Result<tonic::Response<proto::PushEventsResponse>, tonic::Status> {
        Err(tonic::Status::new(
            tonic::Code::Unimplemented,
            "Sink vector does not support PushEventsRequest",
        ))
    }

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
        let emitter = self.emitter.subscribe();
        let mut stream = tokio_stream::wrappers::BroadcastStream::new(emitter);
        let protocol = self.protocol;
        let endpoint = self.endpoint;
        Ok(tonic::Response::new(Box::pin(async_stream::stream! {
            while let Some(result) = tokio_stream::StreamExt::next(&mut stream).await {
                let events = match result {
                    Ok(events) => events,
                    Err(err) => {
                        error!(message = "Received an error from stream", %err);
                        break;
                    }
                };
                vector_lib::internal_event::emit(
                    crate::internal_events::EndpointBytesSent::<'static>{
                        byte_size: events.iter().map(|event| event.encoded_len())
                            .sum(),
                        protocol,
                        endpoint
                    }
                );
            yield Ok(proto::PullEventsResponse {
                events
            });
        }
        })))
    }
}

#[derive(Default)]
struct EventBatch {
    pub finalizers: EventFinalizers,
    pub events: Vec<crate::event::proto::EventWrapper>,
}

pub struct ServeSink {
    pub emitter: tokio::sync::broadcast::Sender<Vec<crate::event::proto::EventWrapper>>,
    pub batch_settings: BatcherSettings,
    pub grpc_shutdown_trigger: Trigger,
    pub stream_shutdown_signal: ShutdownSignal,
}

#[async_trait::async_trait]
impl vector_lib::sink::StreamSink<crate::event::Event> for ServeSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, crate::event::Event>) -> Result<(), ()> {
        let mut batched_stream = Box::pin(input.batched(self.batch_settings.as_reducer_config(
            |event: &crate::event::Event| event.size_of(),
            BatchReduce::new(|batch: &mut EventBatch, mut event: crate::event::Event| {
                batch.finalizers.merge(event.take_finalizers());
                batch
                    .events
                    .push(crate::event::proto::EventWrapper::from(event));
            }),
        )));
        loop {
            tokio::select! {
                _ = &mut self.stream_shutdown_signal => {
                    break;
                }
                maybe_batch = futures::StreamExt::next(&mut batched_stream) => {
                    match maybe_batch {
                        None => break,
                        Some(batch) => {
                            if self.emitter.send(batch.events).is_err() {
                                batch
                                    .finalizers
                                    .update_status(vector_lib::event::EventStatus::Rejected)
                            };
                        }
                    }
                }
            }
        }
        self.grpc_shutdown_trigger.cancel();
        Ok(())
    }
}

pub async fn config_to_serve_sink(
    config: &super::config::VectorConfig,
) -> crate::Result<(crate::sinks::VectorSink, crate::sinks::Healthcheck)> {
    let tls = vector_lib::tls::MaybeTlsSettings::from_config(config.tls.as_ref(), false)?;
    let (emitter, _) = tokio::sync::broadcast::channel(10000);
    let (endpoint, protocol) = crate::sinks::util::uri::protocol_endpoint(
        config
            .address
            .as_ref()
            .ok_or_else(|| -> crate::Error { "address must be defined if mode is serve".into() })?
            .parse::<http::Uri>()
            .map_err(|_| -> crate::Error { "failed to parse address into SocketAddr".into() })?,
    );
    let grpc_bind_address = std::net::ToSocketAddrs::to_socket_addrs(&(endpoint.as_str(), 0))
        .map_err(|e| {
            crate::Error::from(format!(
                "failed to resolve socket addrs from endpoint: {}",
                e
            ))
        })?
        .next()
        .unwrap();

    let service = super::serve::ServeService {
        emitter: emitter.clone(),
        protocol: protocol.leak(),
        endpoint: endpoint.leak(),
    };
    let proto_server = proto::Server::new(service).max_decoding_message_size(usize::MAX);

    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();

    health_reporter
        .set_service_status("vector.Vector", tonic_health::ServingStatus::Serving)
        .await;
    let mut builder = tonic::transport::server::RoutesBuilder::default();
    builder.add_service(proto_server);
    builder.add_service(health_service);

    let (grpc_shutdown_trigger, grpc_shutdown_signal, _) =
        vector_lib::shutdown::ShutdownSignal::new_wired();
    let (stream_shutdown_trigger, stream_shutdown_signal, _) =
        vector_lib::shutdown::ShutdownSignal::new_wired();
    let batch_settings = config.batch.into_batcher_settings()?;
    let source = crate::sources::util::grpc::run_grpc_server_with_routes(
        grpc_bind_address,
        tls,
        builder.routes(),
        crate::sources::util::grpc::GrpcKeepaliveConfig::default(),
        grpc_shutdown_signal,
    )
    .map_err(|error| {
        error!(message = "Sink serve future failed.", %error);
        stream_shutdown_trigger.cancel();
    });
    tokio::spawn(source);
    Ok((
        crate::sinks::VectorSink::from_event_streamsink(super::serve::ServeSink {
            emitter,
            batch_settings,
            grpc_shutdown_trigger: grpc_shutdown_trigger,
            stream_shutdown_signal: stream_shutdown_signal,
        }),
        Box::pin(async { Ok(()) }),
    ))
}
