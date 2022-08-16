use crate::{
    amqp::AmqpConfig,
    codecs::{EncodingConfig, Transformer},
    config::{DataType, GenerateConfig, Input, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    internal_events::{
        sink::{AmqpAcknowledgementFailed, AmqpDeliveryFailed, AmqpNoAcknowledgement},
        TemplateRenderingError,
    },
    sinks::{util::builder::SinkBuilderExt, VectorSink},
    template::{Template, TemplateParseError},
};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use codecs::TextSerializerConfig;
use futures::{future::BoxFuture, FutureExt, StreamExt};
use futures_util::stream::BoxStream;
use lapin::{options::BasicPublishOptions, BasicProperties};
use snafu::{ResultExt, Snafu};
use std::{
    convert::TryFrom,
    io,
    sync::Arc,
    task::{Context, Poll},
};
use tokio_util::codec::Encoder as _;
use tower::{Service, ServiceBuilder};
use vector_common::{
    finalization::{EventFinalizers, EventStatus, Finalizable},
    internal_event::EventsSent,
};
use vector_config::configurable_component;
use vector_core::{config::AcknowledgementsConfig, sink::StreamSink, stream::DriverResponse};

use super::util::{encoding::Encoder, request_builder::EncodeResult, Compression, RequestBuilder};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("creating amqp producer failed: {}", source))]
    AmqpCreateFailed {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[snafu(display("invalid exchange template: {}", source))]
    ExchangeTemplate { source: TemplateParseError },
    #[snafu(display("invalid routing key template: {}", source))]
    RoutingKeyTemplate { source: TemplateParseError },
}

/// Configuration for the `amqp` sink. Handles AMQP version 0.9.
#[configurable_component(source)]
#[derive(Clone, Debug)]
pub struct AmqpSinkConfig {
    /// The exchange to publish messages to.
    pub(crate) exchange: String,

    /// Template use to generate a routing key which corresponds to a queue binding.
    pub(crate) routing_key: Option<String>,

    /// Connection options for Amqp sink
    pub(crate) connection: AmqpConfig,

    #[configurable(derived)]
    pub(crate) encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub(crate) acknowledgements: AcknowledgementsConfig,
}

impl Default for AmqpSinkConfig {
    fn default() -> Self {
        Self {
            exchange: "vector".to_string(),
            routing_key: None,
            encoding: TextSerializerConfig::new().into(),
            connection: AmqpConfig::default(),
            acknowledgements: AcknowledgementsConfig::default(),
        }
    }
}

inventory::submit! {
    SinkDescription::new::<AmqpSinkConfig>("amqp")
}

impl GenerateConfig for AmqpSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"connection.connection_string = "amqp://localhost:5672/%2f"
            routing_key = "user_id"
            exchange = "test"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "amqp")]
impl SinkConfig for AmqpSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, super::Healthcheck)> {
        let sink = AmqpSink::new(self.clone()).await?;
        let hc = healthcheck(self.clone(), Arc::clone(&sink.channel)).boxed();
        Ok((VectorSink::from_event_streamsink(sink), hc))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Log)
    }

    fn sink_type(&self) -> &'static str {
        "amqp"
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
struct AMQPEncoder {
    encoder: crate::codecs::Encoder<()>,
    transformer: crate::codecs::Transformer,
}

impl Encoder<Event> for AMQPEncoder {
    fn encode_input(&self, mut input: Event, writer: &mut dyn io::Write) -> io::Result<usize> {
        let mut body = BytesMut::new();
        self.transformer.transform(&mut input);
        let mut encoder = self.encoder.clone();
        encoder
            .encode(input, &mut body)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "unable to encode"))?;

        let body = body.freeze().to_vec();
        writer.write_all(&body)?;

        Ok(body.len())
    }
}

struct AMQPRequest {
    body: Bytes,
    exchange: String,
    routing_key: String,
    finalizers: EventFinalizers,
}

impl Finalizable for AMQPRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

struct AMQPMetadata {
    exchange: String,
    routing_key: String,
    finalizers: EventFinalizers,
}

struct AMQPService {
    channel: Arc<lapin::Channel>,
}

struct AMQPResponse {
    byte_size: usize,
}

impl DriverResponse for AMQPResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: 1,
            byte_size: self.byte_size,
            output: None,
        }
    }
}

impl Service<AMQPRequest> for AMQPService {
    type Response = AMQPResponse;

    type Error = ();

    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: AMQPRequest) -> Self::Future {
        let channel = Arc::clone(&self.channel);
        Box::pin(async move {
            let byte_size = req.body.len();
            let f = channel
                .basic_publish(
                    &req.exchange,
                    &req.routing_key,
                    BasicPublishOptions::default(),
                    req.body.as_ref(),
                    BasicProperties::default(),
                )
                .await;

            match f {
                Ok(result) => match result.await {
                    Ok(lapin::publisher_confirm::Confirmation::Nack(_)) => {
                        emit!(AmqpNoAcknowledgement::default());
                    }
                    Err(error) => emit!(AmqpAcknowledgementFailed { error }),
                    Ok(_) => (),
                },
                Err(error) => emit!(AmqpDeliveryFailed { error }),
            }

            Ok(AMQPResponse { byte_size })
        })
    }
}

struct AMQPRequestBuilder {
    encoder: AMQPEncoder,
}

impl RequestBuilder<AMQPEvent> for AMQPRequestBuilder {
    type Metadata = AMQPMetadata;
    type Events = Event;
    type Encoder = AMQPEncoder;
    type Payload = Bytes;
    type Request = AMQPRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, mut input: AMQPEvent) -> (Self::Metadata, Self::Events) {
        let metadata = AMQPMetadata {
            exchange: input.exchange,
            routing_key: input.routing_key,
            finalizers: input.event.take_finalizers(),
        };

        (metadata, input.event)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let body = payload.into_payload();
        AMQPRequest {
            body,
            finalizers: metadata.finalizers,
            exchange: metadata.exchange,
            routing_key: metadata.routing_key,
        }
    }
}

/// Stores the event together with the rendered exchange and routing_key values.
/// This is passed into the `RequestBuilder` which then splits it out into the event
/// and metadata containing the exchange and routing_key.
/// This event needs to be created prior to building the request so we can filter out
/// any events that error whilst redndering the templates.
struct AMQPEvent {
    event: Event,
    exchange: String,
    routing_key: String,
}

pub struct AmqpSink {
    channel: Arc<lapin::Channel>,
    exchange: Template,
    routing_key: Option<Template>,
    transformer: Transformer,
    encoder: crate::codecs::Encoder<()>,
}

impl AmqpSink {
    async fn new(config: AmqpSinkConfig) -> crate::Result<Self> {
        let (_, channel) = config
            .connection
            .connect()
            .await
            .map_err(|e| BuildError::AmqpCreateFailed { source: e })?;

        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = crate::codecs::Encoder::<()>::new(serializer);

        Ok(AmqpSink {
            channel: Arc::new(channel),
            exchange: Template::try_from(config.exchange).context(ExchangeTemplateSnafu)?,
            routing_key: config
                .routing_key
                .map(|k| Template::try_from(k).context(RoutingKeyTemplateSnafu))
                .transpose()?,
            transformer,
            encoder,
        })
    }

    /// Transforms an event into an AMQP event by rendering the required template fields.
    /// Returns None if there is an error whilst rendering.
    fn make_amqp_event(&self, event: Event) -> Option<AMQPEvent> {
        let exchange = self
            .exchange
            .render_string(&event)
            .map_err(|missing_keys| {
                emit!(TemplateRenderingError {
                    error: missing_keys,
                    field: Some("exchange"),
                    drop_event: true,
                })
            })
            .ok()?;

        let routing_key = match &self.routing_key {
            None => String::new(),
            Some(key) => key
                .render_string(&event)
                .map_err(|missing_keys| {
                    emit!(TemplateRenderingError {
                        error: missing_keys,
                        field: Some("routing_key"),
                        drop_event: true,
                    })
                })
                .ok()?,
        };

        Some(AMQPEvent {
            event,
            exchange,
            routing_key,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder = AMQPRequestBuilder {
            encoder: AMQPEncoder {
                encoder: self.encoder.clone(),
                transformer: self.transformer.clone(),
            },
        };
        let service = ServiceBuilder::new().service(AMQPService {
            channel: Arc::clone(&self.channel),
        });

        let sink = input
            .filter_map(|event| std::future::ready(self.make_amqp_event(event)))
            .request_builder(None, request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build AMQP request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(service);

        sink.run().await
    }
}

#[async_trait]
impl StreamSink<Event> for AmqpSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

async fn healthcheck(_config: AmqpSinkConfig, channel: Arc<lapin::Channel>) -> crate::Result<()> {
    trace!("Healthcheck started.");

    if !channel.status().connected() {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "Not Connected",
        )));
    }

    trace!("Healthcheck completed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn generate_config() {
        crate::test_util::test_generate_config::<AmqpSinkConfig>();
    }
}

#[cfg(feature = "amqp-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        serde::{default_decoding, default_framing_message_based},
        shutdown::ShutdownSignal,
        test_util::{random_lines_with_stream, random_string},
        SourceSender,
    };
    use futures::StreamExt;
    use std::time::Duration;

    pub fn make_config() -> AmqpSinkConfig {
        let mut config = AmqpSinkConfig {
            exchange: "it".to_string(),
            ..Default::default()
        };
        let user = std::env::var("AMQP_USER").unwrap_or_else(|_| "guest".to_string());
        let pass = std::env::var("AMQP_PASSWORD").unwrap_or_else(|_| "guest".to_string());
        let vhost = std::env::var("AMQP_VHOST").unwrap_or_else(|_| "%2f".to_string());
        config.connection.connection_string =
            format!("amqp://{}:{}@rabbitmq:5672/{}", user, pass, vhost);
        config
    }

    #[tokio::test]
    async fn healthcheck() {
        crate::test_util::trace_init();
        let exchange = format!("test-{}-exchange", random_string(10));

        let mut config = make_config();
        config.exchange = exchange;
        let (_conn, channel) = config.connection.connect().await.unwrap();
        super::healthcheck(config, Arc::new(channel)).await.unwrap();
    }

    #[tokio::test]
    async fn amqp_happy_path_plaintext() {
        crate::test_util::trace_init();

        amqp_happy_path().await;
    }

    #[tokio::test]
    async fn amqp_round_trip_plaintext() {
        crate::test_util::trace_init();

        amqp_round_trip().await;
    }

    async fn amqp_happy_path() {
        let mut config = make_config();
        config.exchange = format!("test-{}-exchange", random_string(10));
        let queue = format!("test-{}-queue", random_string(10));

        let (_conn, channel) = config.connection.connect().await.unwrap();
        let exchange_opts = lapin::options::ExchangeDeclareOptions {
            auto_delete: true,
            ..Default::default()
        };
        channel
            .exchange_declare(
                &config.exchange,
                lapin::ExchangeKind::Fanout,
                exchange_opts,
                lapin::types::FieldTable::default(),
            )
            .await
            .unwrap();

        let sink = VectorSink::from_event_streamsink(AmqpSink::new(config.clone()).await.unwrap());

        // prepare consumer
        let queue_opts = lapin::options::QueueDeclareOptions {
            auto_delete: true,
            ..Default::default()
        };
        channel
            .queue_declare(&queue, queue_opts, lapin::types::FieldTable::default())
            .await
            .unwrap();

        channel
            .queue_bind(
                &queue,
                &config.exchange,
                "",
                lapin::options::QueueBindOptions::default(),
                lapin::types::FieldTable::default(),
            )
            .await
            .unwrap();

        let consumer = format!("test-{}-consumer", random_string(10));
        let mut consumer = channel
            .basic_consume(
                &queue,
                &consumer,
                lapin::options::BasicConsumeOptions::default(),
                lapin::types::FieldTable::default(),
            )
            .await
            .unwrap();

        let num_events = 1000;
        let (input, events) = random_lines_with_stream(100, num_events, None);
        sink.run(events).await.unwrap();

        // loop instead of iter so we can set a timeout
        let mut failures = 0;
        let mut out = Vec::new();
        while failures < 10 && out.len() < input.len() {
            if let Ok(Some(try_msg)) =
                tokio::time::timeout(Duration::from_secs(10), consumer.next()).await
            {
                let (_, msg) = try_msg.unwrap();
                let s = String::from_utf8_lossy(msg.data.as_slice()).into_owned();
                out.push(s);
            } else {
                failures += 1;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        assert_eq!(out.len(), input.len());
        assert_eq!(out, input);
    }

    async fn amqp_round_trip() {
        let mut config = make_config();
        config.exchange = format!("test-{}-exchange", random_string(10));
        let queue = format!("test-{}-queue", random_string(10));

        let (_conn, channel) = config.connection.connect().await.unwrap();
        let exchange_opts = lapin::options::ExchangeDeclareOptions {
            auto_delete: true,
            ..Default::default()
        };
        channel
            .exchange_declare(
                &config.exchange,
                lapin::ExchangeKind::Fanout,
                exchange_opts,
                lapin::types::FieldTable::default(),
            )
            .await
            .unwrap();

        let amqp_sink = AmqpSink::new(config.clone()).await.unwrap();
        let amqp_sink = VectorSink::from_event_streamsink(amqp_sink);

        let source_cfg = crate::sources::amqp::AmqpSourceConfig {
            connection: config.connection.clone(),
            queue: queue.clone(),
            consumer: format!("test-{}-amqp-source", random_string(10)),
            routing_key: None,
            exchange_key: None,
            offset_key: None,
            framing: default_framing_message_based(),
            decoding: default_decoding(),
        };
        let (tx, rx) = SourceSender::new_test();
        let amqp_source =
            crate::sources::amqp::amqp_source(&source_cfg, ShutdownSignal::noop(), tx)
                .await
                .unwrap();

        // prepare server
        let queue_opts = lapin::options::QueueDeclareOptions {
            auto_delete: true,
            ..Default::default()
        };
        channel
            .queue_declare(&queue, queue_opts, lapin::types::FieldTable::default())
            .await
            .unwrap();

        channel
            .queue_bind(
                &queue,
                &config.exchange,
                "",
                lapin::options::QueueBindOptions::default(),
                lapin::types::FieldTable::default(),
            )
            .await
            .unwrap();

        let _source_fut = tokio::spawn(amqp_source);

        //Have sink publish events
        let events_fut = async move {
            let num_events = 1000;
            let (_, events) = random_lines_with_stream(100, num_events, None);
            amqp_sink.run(events).await.unwrap();
            num_events
        };
        let nb_events_published = tokio::spawn(events_fut).await.unwrap();
        let output = crate::test_util::collect_n(rx, 1000).await;

        assert_eq!(output.len(), nb_events_published);
    }
}
