use std::{
    num::NonZeroUsize,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::BytesMut;
use codecs::{encoding::SerializerConfig, TextSerializerConfig};
use futures::{future::BoxFuture, ready, stream::FuturesUnordered, FutureExt, Sink, Stream};
use pulsar::authentication::oauth2::{OAuth2Authentication, OAuth2Params};
use pulsar::error::AuthenticationError;
use pulsar::{
    message::proto, producer::SendFuture, proto::CommandSendReceipt, Authentication,
    Error as PulsarError, Producer, Pulsar, TokioExecutor,
};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::Encoder as _;
use vector_common::internal_event::{
    ByteSize, BytesSent, EventsSent, InternalEventHandle as _, Protocol, Registered,
};
use vector_config::configurable_component;
use vector_core::config::log_schema;

use crate::{
    codecs::{Encoder, EncodingConfig, Transformer},
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, SinkDescription,
    },
    event::{Event, EventFinalizers, EventStatus, Finalizable},
    internal_events::PulsarSendingError,
    sinks::util::metadata::RequestMetadata,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("creating pulsar producer failed: {}", source))]
    CreatePulsarSink { source: PulsarError },
}

/// Configuration for the `pulsar` sink.
#[configurable_component(sink)]
#[derive(Clone, Debug)]
pub struct PulsarSinkConfig {
    /// The endpoint to which the Pulsar client should connect to.
    #[serde(alias = "address")]
    endpoint: String,

    /// The Pulsar topic name to write events to.
    topic: String,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    auth: Option<AuthConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

/// Authentication configuration.
#[configurable_component]
#[derive(Clone, Debug)]
struct AuthConfig {
    /// Basic authentication name/username.
    ///
    /// This can be used either for basic authentication (username/password) or JWT authentication.
    /// When used for JWT, the value should be `token`.
    name: Option<String>,

    /// Basic authentication password/token.
    ///
    /// This can be used either for basic authentication (username/password) or JWT authentication.
    /// When used for JWT, the value should be the signed JWT, in the compact representation.
    token: Option<String>,

    #[configurable(derived)]
    oauth2: Option<OAuth2Config>,
}

/// OAuth2-specific authenticatgion configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct OAuth2Config {
    /// The issuer URL.
    issuer_url: String,

    /// The credentials URL.
    ///
    /// A data URL is also supported.
    credentials_url: String,

    /// The OAuth2 audience.
    audience: Option<String>,

    /// The OAuth2 scope.
    scope: Option<String>,
}

type PulsarProducer = Producer<TokioExecutor>;
type BoxedPulsarProducer = Box<PulsarProducer>;

enum PulsarSinkState {
    None,
    Ready(BoxedPulsarProducer),
    Sending(
        BoxFuture<
            'static,
            (
                BoxedPulsarProducer,
                Result<SendFuture, PulsarError>,
                RequestMetadata,
                EventFinalizers,
            ),
        >,
    ),
}

struct PulsarSink {
    transformer: Transformer,
    encoder: Encoder<()>,
    state: PulsarSinkState,
    in_flight: FuturesUnordered<
        BoxFuture<
            'static,
            (
                Result<CommandSendReceipt, PulsarError>,
                RequestMetadata,
                EventFinalizers,
            ),
        >,
    >,
    bytes_sent: Registered<BytesSent>,
}

inventory::submit! {
    SinkDescription::new::<PulsarSinkConfig>("pulsar")
}

impl GenerateConfig for PulsarSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            endpoint: "pulsar://127.0.0.1:6650".to_string(),
            topic: "topic-1234".to_string(),
            encoding: TextSerializerConfig::new().into(),
            auth: None,
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "pulsar")]
impl SinkConfig for PulsarSinkConfig {
    async fn build(
        &self,
        _cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let producer = self
            .create_pulsar_producer()
            .await
            .context(CreatePulsarSinkSnafu)?;

        let transformer = self.encoding.transformer();
        let serializer = self.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);

        let sink = PulsarSink::new(producer, transformer, encoder)?;

        let producer = self
            .create_pulsar_producer()
            .await
            .context(CreatePulsarSinkSnafu)?;
        let healthcheck = healthcheck(producer).boxed();

        Ok((super::VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "pulsar"
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl PulsarSinkConfig {
    async fn create_pulsar_producer(&self) -> Result<PulsarProducer, PulsarError> {
        let mut builder = Pulsar::builder(&self.endpoint, TokioExecutor);
        if let Some(auth) = &self.auth {
            builder = match (
                auth.name.as_ref(),
                auth.token.as_ref(),
                auth.oauth2.as_ref(),
            ) {
                (Some(name), Some(token), None) => builder.with_auth(Authentication {
                    name: name.clone(),
                    data: token.as_bytes().to_vec(),
                }),
                (None, None, Some(oauth2)) => builder.with_auth_provider(
                    OAuth2Authentication::client_credentials(OAuth2Params {
                        issuer_url: oauth2.issuer_url.clone(),
                        credentials_url: oauth2.credentials_url.clone(),
                        audience: oauth2.audience.clone(),
                        scope: oauth2.scope.clone(),
                    }),
                ),
                _ => return Err(PulsarError::Authentication(AuthenticationError::Custom(
                    "Invalid auth config: can only specify name and token or oauth2 configuration"
                        .to_string(),
                ))),
            };
        }

        let pulsar = builder.build().await?;
        if let SerializerConfig::Avro { avro } = self.encoding.config() {
            pulsar
                .producer()
                .with_options(pulsar::producer::ProducerOptions {
                    schema: Some(proto::Schema {
                        schema_data: avro.schema.as_bytes().into(),
                        r#type: proto::schema::Type::Avro as i32,
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .with_topic(&self.topic)
                .build()
                .await
        } else {
            pulsar.producer().with_topic(&self.topic).build().await
        }
    }
}

async fn healthcheck(producer: PulsarProducer) -> crate::Result<()> {
    producer.check_connection().await.map_err(Into::into)
}

impl PulsarSink {
    fn new(
        producer: PulsarProducer,
        transformer: Transformer,
        encoder: Encoder<()>,
    ) -> crate::Result<Self> {
        Ok(Self {
            transformer,
            encoder,
            state: PulsarSinkState::Ready(Box::new(producer)),
            in_flight: FuturesUnordered::new(),
            bytes_sent: register!(BytesSent::from(Protocol::TCP)),
        })
    }

    fn poll_in_flight_prepare(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if let PulsarSinkState::Sending(fut) = &mut self.state {
            let (producer, result, metadata, finalizers) = ready!(fut.as_mut().poll(cx));

            self.state = PulsarSinkState::Ready(producer);
            self.in_flight.push(Box::pin(async move {
                let result = match result {
                    Ok(fut) => fut.await,
                    Err(error) => Err(error),
                };
                (result, metadata, finalizers)
            }));
        }

        Poll::Ready(())
    }
}

impl Sink<Event> for PulsarSink {
    type Error = ();

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.poll_in_flight_prepare(cx));
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, mut event: Event) -> Result<(), Self::Error> {
        assert!(
            matches!(self.state, PulsarSinkState::Ready(_)),
            "Expected `poll_ready` to be called first."
        );

        let event_time = event.maybe_as_log().and_then(|log| {
            log.get(log_schema().timestamp_key())
                .and_then(|v| v.as_timestamp().map(|dt| dt.timestamp_millis()))
        });

        let metadata_builder = RequestMetadata::builder(&event);
        self.transformer.transform(&mut event);

        let finalizers = event.take_finalizers();
        let mut bytes = BytesMut::new();
        self.encoder.encode(event, &mut bytes).map_err(|_| {
            finalizers.update_status(EventStatus::Errored);
            // Error is handled by `Encoder`.
        })?;

        let bytes_len =
            NonZeroUsize::new(bytes.len()).expect("payload should never be zero length");
        let metadata = metadata_builder.with_request_size(bytes_len);

        let mut producer = match std::mem::replace(&mut self.state, PulsarSinkState::None) {
            PulsarSinkState::Ready(producer) => producer,
            _ => unreachable!(),
        };

        let _ = std::mem::replace(
            &mut self.state,
            PulsarSinkState::Sending(Box::pin(async move {
                let mut builder = producer.create_message().with_content(bytes.as_ref());
                if let Some(et) = event_time {
                    builder = builder.event_time(et as u64);
                }
                let result = builder.send().await;
                (producer, result, metadata, finalizers)
            })),
        );

        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.poll_in_flight_prepare(cx));

        let this = Pin::into_inner(self);
        while !this.in_flight.is_empty() {
            match ready!(Pin::new(&mut this.in_flight).poll_next(cx)) {
                Some((Ok(result), metadata, finalizers)) => {
                    trace!(
                        message = "Pulsar sink produced message.",
                        message_id = ?result.message_id,
                        producer_id = %result.producer_id,
                        sequence_id = %result.sequence_id,
                    );

                    finalizers.update_status(EventStatus::Delivered);

                    emit!(EventsSent {
                        count: metadata.event_count(),
                        byte_size: metadata.events_byte_size(),
                        output: None,
                    });

                    this.bytes_sent
                        .emit(ByteSize(metadata.request_encoded_size()));
                }
                Some((Err(error), metadata, finalizers)) => {
                    finalizers.update_status(EventStatus::Errored);
                    emit!(PulsarSendingError {
                        error: Box::new(error),
                        count: metadata.event_count() as u64,
                    });
                    return Poll::Ready(Err(()));
                }
                None => break,
            }
        }

        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PulsarSinkConfig>();
    }
}

#[cfg(feature = "pulsar-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use futures::StreamExt;
    use pulsar::SubType;

    use super::*;
    use crate::sinks::VectorSink;
    use crate::test_util::{
        components::{assert_sink_compliance, SINK_TAGS},
        random_lines_with_stream, random_string, trace_init,
    };

    fn pulsar_address() -> String {
        std::env::var("PULSAR_ADDRESS").unwrap_or_else(|_| "pulsar://127.0.0.1:6650".into())
    }

    #[tokio::test]
    async fn pulsar_happy() {
        trace_init();

        let num_events = 1_000;
        let (input, events) = random_lines_with_stream(100, num_events, None);

        let topic = format!("test-{}", random_string(10));
        let cnf = PulsarSinkConfig {
            endpoint: pulsar_address(),
            topic: topic.clone(),
            encoding: TextSerializerConfig::new().into(),
            auth: None,
            acknowledgements: Default::default(),
        };

        let pulsar = Pulsar::<TokioExecutor>::builder(&cnf.endpoint, TokioExecutor)
            .build()
            .await
            .unwrap();
        let mut consumer = pulsar
            .consumer()
            .with_topic(&topic)
            .with_consumer_name("VectorTestConsumer")
            .with_subscription_type(SubType::Shared)
            .with_subscription("VectorTestSub")
            .with_options(pulsar::consumer::ConsumerOptions {
                read_compacted: Some(false),
                ..Default::default()
            })
            .build::<String>()
            .await
            .unwrap();

        let producer = cnf.create_pulsar_producer().await.unwrap();
        let transformer = cnf.encoding.transformer();
        let serializer = cnf.encoding.build().unwrap();
        let encoder = Encoder::<()>::new(serializer);

        assert_sink_compliance(&SINK_TAGS, async move {
            let sink = PulsarSink::new(producer, transformer, encoder).unwrap();
            VectorSink::from_event_sink(sink).run(events).await
        })
        .await
        .expect("Running sink failed");

        for line in input {
            let msg = match consumer.next().await.unwrap() {
                Ok(msg) => msg,
                Err(error) => panic!("{:?}", error),
            };
            consumer.ack(&msg).await.unwrap();
            assert_eq!(String::from_utf8_lossy(&msg.payload.data), line);
        }
    }
}
