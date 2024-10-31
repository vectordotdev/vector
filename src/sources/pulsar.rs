//! `Pulsar` source.
//! Accepts log events streamed from [`Apache Pulsar`][pulsar].
//!
//! [pulsar]: https://pulsar.apache.org/
use chrono::TimeZone;
use futures_util::StreamExt;
use pulsar::{
    authentication::oauth2::{OAuth2Authentication, OAuth2Params},
    consumer::Message,
    message::proto::MessageIdData,
    Authentication, Consumer, Pulsar, SubType, TokioExecutor,
};
use tokio_util::codec::FramedRead;

use vector_lib::{
    codecs::{
        decoding::{DeserializerConfig, FramingConfig},
        StreamDecodingError,
    },
    config::{LegacyKey, LogNamespace, SourceAcknowledgementsConfig, SourceOutput},
    configurable::configurable_component,
    event::Event,
    finalization::BatchStatus,
    finalizer::OrderedFinalizer,
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, EventsReceived, InternalEventHandle, Protocol,
        Registered,
    },
    sensitive_string::SensitiveString,
    shutdown::ShutdownSignal,
    EstimatedJsonEncodedSizeOf,
};
use vrl::{owned_value_path, path, value::Kind};

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{SourceConfig, SourceContext},
    event::BatchNotifier,
    internal_events::{
        PulsarErrorEvent, PulsarErrorEventData, PulsarErrorEventType, StreamClosedError,
    },
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    SourceSender,
};

/// Configuration for the `pulsar` source.
#[configurable_component(source("pulsar", "Collect logs from Apache Pulsar."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct PulsarSourceConfig {
    /// The endpoint to which the Pulsar client should connect to.
    #[configurable(metadata(docs::examples = "pulsar://127.0.0.1:6650"))]
    #[serde(alias = "address")]
    endpoint: String,

    /// The Pulsar topic names to read events from.
    #[configurable(metadata(docs::examples = "[persistent://public/default/my-topic]"))]
    topics: Vec<String>,

    /// The Pulsar consumer name.
    #[configurable(metadata(docs::examples = "consumer-name"))]
    consumer_name: Option<String>,

    /// The Pulsar subscription name.
    #[configurable(metadata(docs::examples = "subscription_name"))]
    subscription_name: Option<String>,

    /// The consumer's priority level.
    ///
    /// The broker follows descending priorities. For example, 0=max-priority, 1, 2,...
    ///
    /// In Shared subscription type, the broker first dispatches messages to the max priority level consumers if they have permits. Otherwise, the broker considers next priority level consumers.
    priority_level: Option<i32>,

    /// Max count of messages in a batch.
    batch_size: Option<u32>,

    #[configurable(derived)]
    auth: Option<AuthConfig>,

    #[configurable(derived)]
    dead_letter_queue_policy: Option<DeadLetterQueuePolicy>,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    decoding: DeserializerConfig,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    log_namespace: Option<bool>,
}

/// Authentication configuration.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, untagged)]
enum AuthConfig {
    /// Basic authentication.
    Basic {
        /// Basic authentication name/username.
        ///
        /// This can be used either for basic authentication (username/password) or JWT authentication.
        /// When used for JWT, the value should be `token`.
        #[configurable(metadata(docs::examples = "${PULSAR_NAME}"))]
        #[configurable(metadata(docs::examples = "name123"))]
        name: String,

        /// Basic authentication password/token.
        ///
        /// This can be used either for basic authentication (username/password) or JWT authentication.
        /// When used for JWT, the value should be the signed JWT, in the compact representation.
        #[configurable(metadata(docs::examples = "${PULSAR_TOKEN}"))]
        #[configurable(metadata(docs::examples = "123456789"))]
        token: SensitiveString,
    },

    /// OAuth authentication.
    OAuth {
        #[configurable(derived)]
        oauth2: OAuth2Config,
    },
}

/// OAuth2-specific authentication configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct OAuth2Config {
    /// The issuer URL.
    #[configurable(metadata(docs::examples = "${OAUTH2_ISSUER_URL}"))]
    #[configurable(metadata(docs::examples = "https://oauth2.issuer"))]
    issuer_url: String,

    /// The credentials URL.
    ///
    /// A data URL is also supported.
    #[configurable(metadata(docs::examples = "${OAUTH2_CREDENTIALS_URL}"))]
    #[configurable(metadata(docs::examples = "file:///oauth2_credentials"))]
    #[configurable(metadata(docs::examples = "data:application/json;base64,cHVsc2FyCg=="))]
    credentials_url: String,

    /// The OAuth2 audience.
    #[configurable(metadata(docs::examples = "${OAUTH2_AUDIENCE}"))]
    #[configurable(metadata(docs::examples = "pulsar"))]
    audience: Option<String>,

    /// The OAuth2 scope.
    #[configurable(metadata(docs::examples = "${OAUTH2_SCOPE}"))]
    #[configurable(metadata(docs::examples = "admin"))]
    scope: Option<String>,
}

/// Dead Letter Queue policy configuration.
#[configurable_component]
#[derive(Clone, Debug)]
struct DeadLetterQueuePolicy {
    /// Maximum number of times that a message will be redelivered before being sent to the dead letter queue.
    pub max_redeliver_count: usize,

    /// Name of the dead letter topic where the failing messages will be sent.
    pub dead_letter_topic: String,
}

#[derive(Debug)]
struct FinalizerEntry {
    topic: String,
    message_id: MessageIdData,
}

impl_generate_config_from_default!(PulsarSourceConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "pulsar")]
impl SourceConfig for PulsarSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let consumer = self.create_consumer().await?;
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;
        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        Ok(Box::pin(pulsar_source(
            consumer,
            decoder,
            cx.shutdown,
            cx.out,
            acknowledgements,
            log_namespace,
        )))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("publish_time"))),
                &owned_value_path!("publish_time"),
                Kind::timestamp(),
                Some("publish_time"),
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("topic"))),
                &owned_value_path!("topic"),
                Kind::bytes(),
                Some("topic"),
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::InsertIfEmpty(owned_value_path!("producer_name"))),
                &owned_value_path!("producer_name"),
                Kind::bytes(),
                Some("producer_name"),
            );
        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

impl PulsarSourceConfig {
    async fn create_consumer(
        &self,
    ) -> crate::Result<pulsar::consumer::Consumer<String, TokioExecutor>> {
        let mut builder = Pulsar::builder(&self.endpoint, TokioExecutor);

        if let Some(auth) = &self.auth {
            builder = match auth {
                AuthConfig::Basic { name, token } => builder.with_auth(Authentication {
                    name: name.clone(),
                    data: token.inner().as_bytes().to_vec(),
                }),
                AuthConfig::OAuth { oauth2 } => builder.with_auth_provider(
                    OAuth2Authentication::client_credentials(OAuth2Params {
                        issuer_url: oauth2.issuer_url.clone(),
                        credentials_url: oauth2.credentials_url.clone(),
                        audience: oauth2.audience.clone(),
                        scope: oauth2.scope.clone(),
                    }),
                ),
            };
        }

        let pulsar = builder.build().await?;

        let mut consumer_builder = pulsar
            .consumer()
            .with_topics(&self.topics)
            .with_subscription_type(SubType::Shared)
            .with_options(pulsar::consumer::ConsumerOptions {
                priority_level: self.priority_level,
                ..Default::default()
            });

        if let Some(dead_letter_queue_policy) = &self.dead_letter_queue_policy {
            consumer_builder =
                consumer_builder.with_dead_letter_policy(pulsar::consumer::DeadLetterPolicy {
                    max_redeliver_count: dead_letter_queue_policy.max_redeliver_count,
                    dead_letter_topic: dead_letter_queue_policy.dead_letter_topic.clone(),
                });
        }

        if let Some(batch_size) = self.batch_size {
            consumer_builder = consumer_builder.with_batch_size(batch_size);
        }
        if let Some(consumer_name) = &self.consumer_name {
            consumer_builder = consumer_builder.with_consumer_name(consumer_name);
        }
        if let Some(subscription_name) = &self.subscription_name {
            consumer_builder = consumer_builder.with_subscription(subscription_name);
        }

        let consumer = consumer_builder.build::<String>().await?;

        Ok(consumer)
    }
}

async fn pulsar_source(
    mut consumer: Consumer<String, TokioExecutor>,
    decoder: Decoder,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
    acknowledgements: bool,
    log_namespace: LogNamespace,
) -> Result<(), ()> {
    let (finalizer, mut ack_stream) =
        OrderedFinalizer::<FinalizerEntry>::maybe_new(acknowledgements, Some(shutdown.clone()));

    let bytes_received = register!(BytesReceived::from(Protocol::TCP));
    let events_received = register!(EventsReceived);
    let pulsar_error_events = register!(PulsarErrorEvent);

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            entry = ack_stream.next() => {
                if let Some((status, entry)) = entry {
                    handle_ack(&mut consumer, status, entry, &pulsar_error_events).await;
                }
            },
            Some(maybe_message) = consumer.next() => {
                match maybe_message {
                    Ok(msg) => {
                        bytes_received.emit(ByteSize(msg.payload.data.len()));
                        parse_message(msg, &decoder, &finalizer, &mut out, &mut consumer, log_namespace, &events_received, &pulsar_error_events).await;
                    }
                    Err(error) => {
                        pulsar_error_events.emit(PulsarErrorEventData{
                            msg: error.to_string(),
                            error_type:PulsarErrorEventType::Read,
                        });
                    }
                }
            },
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn parse_message(
    msg: Message<String>,
    decoder: &Decoder,
    finalizer: &Option<OrderedFinalizer<FinalizerEntry>>,
    out: &mut SourceSender,
    consumer: &mut Consumer<String, TokioExecutor>,
    log_namespace: LogNamespace,
    events_received: &Registered<EventsReceived>,
    pulsar_error_events: &Registered<PulsarErrorEvent>,
) {
    let publish_time = i64::try_from(msg.payload.metadata.publish_time)
        .ok()
        .and_then(|millis| chrono::Utc.timestamp_millis_opt(millis).latest());
    let topic = msg.topic.clone();
    let producer_name = msg.payload.metadata.producer_name.clone();

    let mut stream = FramedRead::new(msg.payload.data.as_ref(), decoder.clone());
    let stream = async_stream::stream! {
        while let Some(next) = stream.next().await {
            match next {
                Ok((events, _byte_size)) => {
                    events_received.emit(CountByteSize(
                        events.len(),
                        events.estimated_json_encoded_size_of(),
                    ));

                    let now = chrono::Utc::now();

                    let events = events.into_iter().map(|mut event| {
                        if let Event::Log(ref mut log) = event {
                            log_namespace.insert_standard_vector_source_metadata(
                                log,
                                PulsarSourceConfig::NAME,
                                now,
                            );

                            log_namespace.insert_source_metadata(
                                PulsarSourceConfig::NAME,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!("publish_time"))),
                                path!("publish_time"),
                                publish_time,
                            );

                            log_namespace.insert_source_metadata(
                                PulsarSourceConfig::NAME,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!("topic"))),
                                path!("topic"),
                                topic.clone(),
                            );

                            log_namespace.insert_source_metadata(
                                PulsarSourceConfig::NAME,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!("producer_name"))),
                                path!("producer_name"),
                                producer_name.clone(),
                            );
                        }
                        event
                    });

                    for event in events {
                        yield event;
                    }
                }
                Err(error) => {
                    // Error is logged by `crate::codecs`, no further
                    // handling is needed here.
                    if !error.can_continue() {
                        break;
                    }
                }
            }
        }
    }
    .boxed();

    finalize_event_stream(
        consumer,
        finalizer,
        out,
        stream,
        msg.topic.clone(),
        msg.message_id().clone(),
        pulsar_error_events,
    )
    .await;
}

/// Send the event stream created by the framed read to the `out` stream.
async fn finalize_event_stream(
    consumer: &mut Consumer<String, TokioExecutor>,
    finalizer: &Option<OrderedFinalizer<FinalizerEntry>>,
    out: &mut SourceSender,
    mut stream: std::pin::Pin<Box<dyn futures_util::Stream<Item = Event> + Send + '_>>,
    topic: String,
    message_id: MessageIdData,
    pulsar_error_events: &Registered<PulsarErrorEvent>,
) {
    match finalizer {
        Some(finalizer) => {
            let (batch, receiver) = BatchNotifier::new_with_receiver();
            let mut stream = stream.map(|event| event.with_batch_notifier(&batch));

            match out.send_event_stream(&mut stream).await {
                Err(_error) => {
                    emit!(StreamClosedError { count: 1 });
                }
                Ok(_) => {
                    finalizer.add(FinalizerEntry { topic, message_id }, receiver);
                }
            }
        }
        None => match out.send_event_stream(&mut stream).await {
            Err(_error) => {
                emit!(StreamClosedError { count: 1 });
            }
            Ok(_) => {
                if let Err(error) = consumer.ack_with_id(topic.as_str(), message_id).await {
                    pulsar_error_events.emit(PulsarErrorEventData {
                        msg: error.to_string(),
                        error_type: PulsarErrorEventType::Ack,
                    });
                }
            }
        },
    }
}

async fn handle_ack(
    consumer: &mut Consumer<String, TokioExecutor>,
    status: BatchStatus,
    entry: FinalizerEntry,
    pulsar_error_events: &Registered<PulsarErrorEvent>,
) {
    match status {
        BatchStatus::Delivered => {
            if let Err(error) = consumer
                .ack_with_id(entry.topic.as_str(), entry.message_id)
                .await
            {
                pulsar_error_events.emit(PulsarErrorEventData {
                    msg: error.to_string(),
                    error_type: PulsarErrorEventType::Ack,
                });
            }
        }
        BatchStatus::Errored | BatchStatus::Rejected => {
            if let Err(error) = consumer
                .nack_with_id(entry.topic.as_str(), entry.message_id)
                .await
            {
                pulsar_error_events.emit(PulsarErrorEventData {
                    msg: error.to_string(),
                    error_type: PulsarErrorEventType::NAck,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::sources::pulsar::PulsarSourceConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PulsarSourceConfig>();
    }
}

#[cfg(feature = "pulsar-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::config::log_schema;
    use crate::test_util::components::{assert_source_compliance, SOURCE_TAGS};
    use crate::test_util::{collect_n, random_string, trace_init};

    fn pulsar_address() -> String {
        std::env::var("PULSAR_ADDRESS").unwrap_or_else(|_| "pulsar://127.0.0.1:6650".into())
    }

    #[tokio::test]
    async fn consumes_event_with_acknowledgements() {
        pulsar_send_receive(true, LogNamespace::Legacy).await;
    }

    #[tokio::test]
    async fn consumes_event_with_acknowledgements_vector_namespace() {
        pulsar_send_receive(true, LogNamespace::Vector).await;
    }

    #[tokio::test]
    async fn consumes_event_without_acknowledgements() {
        pulsar_send_receive(false, LogNamespace::Legacy).await;
    }

    #[tokio::test]
    async fn consumes_event_without_acknowledgements_vector_namespace() {
        pulsar_send_receive(false, LogNamespace::Vector).await;
    }

    async fn pulsar_send_receive(acknowledgements: bool, log_namespace: LogNamespace) {
        trace_init();

        let topic = format!("test-{}", random_string(10));
        let cnf = PulsarSourceConfig {
            endpoint: pulsar_address(),
            topics: vec![topic.clone()],
            consumer_name: None,
            subscription_name: None,
            priority_level: None,
            batch_size: None,
            auth: None,
            dead_letter_queue_policy: None,
            framing: FramingConfig::Bytes,
            decoding: DeserializerConfig::Bytes,
            acknowledgements: acknowledgements.into(),
            log_namespace: None,
        };

        let pulsar = Pulsar::<TokioExecutor>::builder(&cnf.endpoint, TokioExecutor)
            .build()
            .await
            .unwrap();

        let consumer = cnf.create_consumer().await.unwrap();
        let decoder = DecodingConfig::new(
            cnf.framing.clone(),
            cnf.decoding.clone(),
            LogNamespace::Legacy,
        )
        .build()
        .unwrap();

        let mut producer = pulsar.producer().with_topic(topic).build().await.unwrap();

        let msg = "test message";

        let events = assert_source_compliance(&SOURCE_TAGS, async move {
            let (tx, rx) = SourceSender::new_test();
            tokio::spawn(pulsar_source(
                consumer,
                decoder,
                ShutdownSignal::noop(),
                tx,
                acknowledgements,
                log_namespace,
            ));
            producer.send_non_blocking(msg).await.unwrap();

            collect_n(rx, 1).await
        })
        .await;

        assert_eq!(
            events[0].as_log()[log_schema().message_key().unwrap().to_string()],
            msg.into()
        );
    }
}
