use crate::codecs::{Decoder, DecodingConfig};
use crate::config::{SourceConfig, SourceContext};
use crate::internal_events::PulsarAcknowledgmentError;
use crate::internal_events::PulsarReadError;
use crate::internal_events::StreamClosedError;
use crate::serde::{default_decoding, default_framing_message_based};
use crate::SourceSender;
use bytes::Bytes;
use chrono::Utc;
use codecs::decoding::{DeserializerConfig, FramingConfig};
use codecs::StreamDecodingError;
use futures_util::StreamExt;
use pulsar::authentication::oauth2::{OAuth2Authentication, OAuth2Params};
use pulsar::error::AuthenticationError;
use pulsar::{Authentication, SubType};
use pulsar::{Consumer, Pulsar, TokioExecutor};
use tokio_util::codec::FramedRead;
use vector_common::internal_event::ByteSize;
use vector_common::internal_event::{
    BytesReceived, EventsReceived, InternalEventHandle as _, Protocol,
};
use vector_common::sensitive_string::SensitiveString;
use vector_common::shutdown::ShutdownSignal;
use vector_config::component::GenerateConfig;
use vector_config_macros::configurable_component;
use vector_core::config::{log_schema, LogNamespace, Output};
use vector_core::event::Event;
use vector_core::ByteSizeOf;

/// Configuration for the `pulsar` source.
#[configurable_component(source("pulsar"))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct PulsarSourceConfig {
    /// The endpoint to which the Pulsar client should connect to.
    #[serde(alias = "address")]
    endpoint: String,

    /// The Pulsar topic names to read events from.
    topics: Vec<String>,

    /// The Pulsar consumer name.
    consumer_name: Option<String>,

    /// The Pulsar subscription name.
    subscription_name: Option<String>,

    /// Priority level for a consumer to which a broker gives more priority while dispatching messages in Shared subscription type.
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
    token: Option<SensitiveString>,

    #[configurable(derived)]
    oauth2: Option<OAuth2Config>,
}

/// OAuth2-specific authentication configuration.
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

/// Dead Letter Queue policy configuration.
#[configurable_component]
#[derive(Clone, Debug)]
struct DeadLetterQueuePolicy {
    /// Maximum number of times that a message will be redelivered before being sent to the dead letter queue.
    pub max_redeliver_count: usize,

    /// Name of the dead topic where the failing messages will be sent.
    pub dead_letter_topic: String,
}

impl GenerateConfig for PulsarSourceConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            topics = ["topic1", "topic2"]
            endpoint = "pulsar://127.0.0.1:6650""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SourceConfig for PulsarSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let consumer = create_consumer(self).await?;
        let decoder = DecodingConfig::new(
            self.framing.clone(),
            self.decoding.clone(),
            LogNamespace::Legacy,
        )
        .build();

        Ok(Box::pin(pulsar_source(
            consumer,
            decoder,
            cx.shutdown,
            cx.out,
        )))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(self.decoding.output_type())]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

async fn create_consumer(
    config: &PulsarSourceConfig,
) -> crate::Result<pulsar::consumer::Consumer<String, TokioExecutor>> {
    let mut builder = Pulsar::builder(&config.endpoint, TokioExecutor);

    if let Some(auth) = &config.auth {
        builder = match (
            auth.name.as_ref(),
            auth.token.as_ref(),
            auth.oauth2.as_ref(),
        ) {
            (Some(name), Some(token), None) => builder.with_auth(Authentication {
                name: name.clone(),
                data: token.inner().as_bytes().to_vec(),
            }),
            (None, None, Some(oauth2)) => {
                builder.with_auth_provider(OAuth2Authentication::client_credentials(OAuth2Params {
                    issuer_url: oauth2.issuer_url.clone(),
                    credentials_url: oauth2.credentials_url.clone(),
                    audience: oauth2.audience.clone(),
                    scope: oauth2.scope.clone(),
                }))
            }
            _ => return Err(Box::new(pulsar::error::Error::Authentication(
                AuthenticationError::Custom(
                    "Invalid auth config: can only specify name and token or oauth2 configuration"
                        .to_string(),
                ),
            ))),
        };
    }

    let pulsar = builder.build().await?;

    let mut consumer_builder = pulsar
        .consumer()
        .with_topics(&config.topics)
        .with_subscription_type(SubType::Shared)
        .with_options(pulsar::consumer::ConsumerOptions {
            priority_level: config.priority_level,
            ..Default::default()
        });

    if let Some(dead_letter_queue_policy) = &config.dead_letter_queue_policy {
        consumer_builder =
            consumer_builder.with_dead_letter_policy(pulsar::consumer::DeadLetterPolicy {
                max_redeliver_count: dead_letter_queue_policy.max_redeliver_count,
                dead_letter_topic: dead_letter_queue_policy.dead_letter_topic.clone(),
            });
    }

    if let Some(batch_size) = config.batch_size {
        consumer_builder = consumer_builder.with_batch_size(batch_size);
    }
    if let Some(consumer_name) = &config.consumer_name {
        consumer_builder = consumer_builder.with_consumer_name(consumer_name);
    }
    if let Some(subscription_name) = &config.subscription_name {
        consumer_builder = consumer_builder.with_subscription(subscription_name);
    }

    let consumer = consumer_builder.build::<String>().await?;

    Ok(consumer)
}

async fn pulsar_source(
    mut consumer: Consumer<String, TokioExecutor>,
    decoder: Decoder,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let bytes_received = register!(BytesReceived::from(Protocol::TCP));
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            Some(maybe_message) = consumer.next() => {
                match maybe_message {
                    Ok(msg) => {
                        bytes_received.emit(ByteSize(msg.payload.data.len()));
                        let mut stream = FramedRead::new(msg.payload.data.as_ref(), decoder.clone());
                        while let Some(next) = stream.next().await {
                            match next {
                                Ok((events, _byte_size)) => {
                                    let count = events.len();
                                    emit!(EventsReceived {
                                        count,
                                        byte_size: events.size_of()
                                    });

                                    let now = Utc::now();

                                    let events = events.into_iter().map(|mut event| {
                                        if let Event::Log(ref mut log) = event {
                                            log.try_insert(
                                                log_schema().source_type_key(),
                                                Bytes::from("pulsar"),
                                            );
                                            log.try_insert(log_schema().timestamp_key(), now);
                                        }
                                        event
                                    });

                                    out.send_batch(events).await.map_err(|error| {
                                        emit!(StreamClosedError { error, count });
                                    })?;

                                    if let Err(error) = consumer.ack(&msg).await {
                                        emit!(PulsarAcknowledgmentError { error });
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
                    Err(error) => {
                        emit!(PulsarReadError { error })
                    }
                }
            },
        }
    }

    Ok(())
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
    use crate::test_util::components::{assert_source_compliance, SOURCE_TAGS};
    use crate::test_util::{collect_n, random_string, trace_init};

    fn pulsar_address() -> String {
        std::env::var("PULSAR_ADDRESS").unwrap_or_else(|_| "pulsar://127.0.0.1:6650".into())
    }

    #[tokio::test]
    async fn pulsar_happy() {
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
        };

        let pulsar = Pulsar::<TokioExecutor>::builder(&cnf.endpoint, TokioExecutor)
            .build()
            .await
            .unwrap();

        let consumer = create_consumer(&cnf).await.unwrap();
        let decoder = DecodingConfig::new(
            cnf.framing.clone(),
            cnf.decoding.clone(),
            LogNamespace::Legacy,
        )
        .build();

        let mut producer = pulsar.producer().with_topic(topic).build().await.unwrap();

        let msg = "test message";

        let events = assert_source_compliance(&SOURCE_TAGS, async move {
            let (tx, rx) = SourceSender::new_test();
            tokio::spawn(pulsar_source(consumer, decoder, ShutdownSignal::noop(), tx));
            producer.send(msg).await.unwrap();

            collect_n(rx, 1).await
        })
        .await;

        assert_eq!(events[0].as_log()[log_schema().message_key()], msg.into());
    }
}
