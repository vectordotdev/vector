use crate::codecs::{Decoder, DecodingConfig};
use crate::config::{SourceConfig, SourceContext};
use crate::internal_events::StreamClosedError;
use crate::serde::{default_decoding, default_framing_message_based};
use crate::SourceSender;
use bytes::Bytes;
use chrono::Utc;
use codecs::decoding::{DeserializerConfig, FramingConfig};
use codecs::StreamDecodingError;
use metrics::counter;
use pulsar::SubType;
use pulsar::{Consumer, Pulsar, TokioExecutor};
use tokio_stream::StreamExt;
use tokio_util::codec::FramedRead;
use vector_common::internal_event::{error_stage, error_type};
use vector_common::internal_event::{ByteSize, InternalEvent};
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

    /// The Pulsar topic name to read events from.
    topic: String,

    /// The Pulsar consumer name.
    consumer_name: String,

    /// The Pulsar subscription name.
    subscription_name: String,

    #[configurable(derived)]
    auth: Option<AuthConfig>,

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

impl GenerateConfig for PulsarSourceConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            topic = "from.vector"
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
            self.clone(),
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
    let pulsar = Pulsar::<TokioExecutor>::builder(&config.endpoint, TokioExecutor)
        .build()
        .await?;
    let consumer = pulsar
        .consumer()
        .with_topic(&config.topic)
        .with_consumer_name(&config.consumer_name)
        .with_subscription_type(SubType::Shared)
        .with_subscription(&config.subscription_name)
        .build::<String>()
        .await?;

    Ok(consumer)
}

#[derive(Debug)]
pub struct PulsarReadError {
    pub error: pulsar::Error,
}

impl InternalEvent for PulsarReadError {
    fn emit(self) {
        error!(
            message = "Failed to read message.",
            error = %self.error,
            error_code = "reading_message",
            error_type = error_type::READER_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "reading_message",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("events_failed_total", 1);
    }
}

async fn pulsar_source(
    _: PulsarSourceConfig,
    mut consumer: Consumer<String, TokioExecutor>,
    decoder: Decoder,
    _: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let bytes_received = register!(BytesReceived::from(Protocol::TCP));
    while let Some(msg) = consumer.next().await {
        match msg {
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
    }
    Ok(())
}
