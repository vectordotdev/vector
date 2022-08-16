use std::io::Cursor;

use crate::codecs::{Decoder, DecodingConfig};
use crate::config::SourceContext;
use crate::{
    amqp::AmqpConfig,
    config::{log_schema, DataType, Output, SourceConfig, SourceDescription},
    event::Value,
    internal_events::source::{
        AmqpCommitFailed, AmqpConsumerFailed, AmqpDeliveryFailed, AmqpEventFailed,
        AmqpEventReceived,
    },
    serde::{default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    SourceSender,
};
use async_stream::stream;
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use codecs::decoding::{DeserializerConfig, FramingConfig};
use futures::{FutureExt, StreamExt};
use lapin::Channel;
use snafu::Snafu;
use tokio_util::codec::FramedRead;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Could not create Amqp consumer: {}", source))]
    AmqpCreateError {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[snafu(display("Could not subscribe to Amqp queue: {}", source))]
    AmqpSubscribeError { source: lapin::Error },
}

/// Configuration for the `amqp` source.
#[configurable_component(source)]
#[derive(Clone, Debug, Derivative)]
#[serde(deny_unknown_fields)]
pub struct AmqpSourceConfig {
    /// The queue.
    pub(crate) queue: String,

    /// The consumer.
    pub(crate) consumer: String,

    /// The log field name to use for the Amqp routing key.
    pub(crate) routing_key: Option<String>,

    /// The log field name to use for the Amqp exchange key.
    pub(crate) exchange_key: Option<String>,

    /// The log field name to use for the Amqp offset key.
    pub(crate) offset_key: Option<String>,

    /// Connection options for Amqp source.
    pub(crate) connection: AmqpConfig,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub(crate) framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub(crate) decoding: DeserializerConfig,
}

impl Default for AmqpSourceConfig {
    fn default() -> Self {
        Self {
            queue: "vector".to_string(),
            consumer: "vector".to_string(),
            routing_key: None,
            exchange_key: None,
            offset_key: None,
            connection: AmqpConfig::default(),
            framing: default_framing_message_based(),
            decoding: default_decoding(),
        }
    }
}

inventory::submit! {
    SourceDescription::new::<AmqpSourceConfig>("amqp")
}

impl_generate_config_from_default!(AmqpSourceConfig);

impl AmqpSourceConfig {
    fn decoder(&self) -> Decoder {
        DecodingConfig::new(
            self.framing.clone(),
            self.decoding.clone(),
            LogNamespace::Legacy,
        )
        .build()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "amqp")]
impl SourceConfig for AmqpSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        amqp_source(self, cx.shutdown, cx.out).await
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "amqp"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

pub(crate) async fn amqp_source(
    config: &AmqpSourceConfig,
    shutdown: ShutdownSignal,
    out: SourceSender,
) -> crate::Result<super::Source> {
    let config = config.clone();
    let (_conn, channel) = config
        .connection
        .connect()
        .await
        .map_err(|e| BuildError::AmqpCreateError { source: e })?;

    Ok(Box::pin(run_amqp_source(config, shutdown, out, channel)))
}

async fn run_amqp_source(
    config: AmqpSourceConfig,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
    channel: Channel,
) -> Result<(), ()> {
    let ack_options = lapin::options::BasicAckOptions::default();
    debug!("Starting amqp source, listening to queue {}", config.queue);
    let mut consumer = channel
        .basic_consume(
            &config.queue,
            &config.consumer,
            lapin::options::BasicConsumeOptions::default(),
            lapin::types::FieldTable::default(),
        )
        .await
        .map_err(|error| {
            emit!(AmqpConsumerFailed { error });
        })?
        .fuse();
    let mut shutdown = shutdown.fuse();
    loop {
        let should_break = futures::select! {
            _ = shutdown => true,
            opt_m = consumer.next() => {
                if let Some(try_m) = opt_m {
                    match try_m {
                        Err(error) => {
                            emit!(AmqpEventFailed { error });
                            return Err(());
                        }
                        Ok(msg) => {
                            emit!(AmqpEventReceived {
                                byte_size: msg.data.len()
                            });

                            if msg.data.is_empty() {
                                return Err(());
                            }

                            let payload = Cursor::new(Bytes::copy_from_slice(&msg.data));
                            let mut stream = FramedRead::new(payload, config.decoder());

                            let routing_key = config.routing_key.as_ref();
                            let exchange_key = config.exchange_key.as_ref();
                            let offset_key = config.offset_key.as_ref();
                            let out = &mut out;

                            let mut stream = stream! {
                                while let Some(result) = stream.next().await {
                                    match result {
                                        Ok((events, _byte_size)) => {
                                            for mut event in events {
                                                let log = event.as_mut_log();

                                                // Extract timestamp from amqp message
                                                let timestamp = msg
                                                    .properties
                                                    .timestamp()
                                                    .and_then(|millis| Utc.timestamp_millis_opt(millis as _).latest())
                                                    .unwrap_or_else(Utc::now);
                                                log.insert(log_schema().timestamp_key(), timestamp);

                                                // Add source type
                                                log.insert(log_schema().source_type_key(), Bytes::from("amqp"));

                                                if let Some(key_field) = routing_key {
                                                    log.insert(key_field.as_str(), Value::from(msg.routing_key.to_string()));
                                                }

                                                if let Some(exchange_key) = exchange_key {
                                                    log.insert(exchange_key.as_str(), Value::from(msg.exchange.to_string()));
                                                }

                                                if let Some(offset_key) = offset_key {
                                                    log.insert(offset_key.as_str(), Value::from(msg.delivery_tag as i64));
                                                }

                                                if let Err(error) = msg.acker.ack(ack_options).await {
                                                    emit!(AmqpCommitFailed { error });
                                                }

                                                yield event;
                                            }
                                        }
                                        Err(error) => {
                                            use codecs::StreamDecodingError as _;

                                            // Error is logged by `codecs::Decoder`, no further handling
                                            // is needed here.
                                            if !error.can_continue() {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            .boxed();

                            if let Err(error) = out.send_event_stream(&mut stream).await {
                                emit!(AmqpDeliveryFailed { error });
                            }
                        }
                    }
                    false
                } else {
                    true
                }
            }
        };

        if should_break {
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
pub mod test {
    use super::{amqp_source, AmqpSourceConfig};
    use crate::{shutdown::ShutdownSignal, SourceSender};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AmqpSourceConfig>();
    }

    pub fn make_config() -> AmqpSourceConfig {
        let mut config = AmqpSourceConfig {
            queue: "it".to_string(),
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
    async fn amqp_source_create_ok() {
        let config = make_config();
        assert!(
            amqp_source(&config, ShutdownSignal::noop(), SourceSender::new_test().0)
                .await
                .is_ok()
        );
    }
}

#[cfg(feature = "amqp-integration-tests")]
#[cfg(test)]
mod integration_test {
    use super::test::*;
    use super::*;
    use crate::{
        shutdown::ShutdownSignal,
        test_util::{collect_n, random_string},
        SourceSender,
    };
    use chrono::Utc;
    use lapin::options::*;
    use lapin::BasicProperties;

    async fn send_event(
        channel: &lapin::Channel,
        exchange: &str,
        routing_key: &str,
        text: &str,
        _timestamp: i64,
    ) {
        let payload = text.as_bytes();
        let payload_len = payload.len();
        trace!("Sending message of length {} to {}", payload_len, exchange,);

        channel
            .basic_publish(
                exchange,
                routing_key,
                BasicPublishOptions::default(),
                payload.as_ref(),
                BasicProperties::default(),
            )
            .await
            .unwrap()
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn amqp_source_consume_event() {
        let exchange = format!("test-{}-exchange", random_string(10));
        let queue = format!("test-{}-queue", random_string(10));
        let routing_key = "my_key";
        trace!("Test exchange name: {}", exchange);
        let consumer = format!("test-consumer-{}", random_string(10));

        let mut config = make_config();
        config.consumer = consumer;
        config.queue = queue;
        config.routing_key = Some("message_key".to_string());
        config.exchange_key = Some("exchange".to_string());
        let (_conn, channel) = config.connection.connect().await.unwrap();

        let exchange_opts = lapin::options::ExchangeDeclareOptions {
            auto_delete: true,
            ..Default::default()
        };
        channel
            .exchange_declare(
                &exchange,
                lapin::ExchangeKind::Fanout,
                exchange_opts,
                lapin::types::FieldTable::default(),
            )
            .await
            .unwrap();

        let queue_opts = QueueDeclareOptions {
            auto_delete: true,
            ..Default::default()
        };
        channel
            .queue_declare(
                &config.queue,
                queue_opts,
                lapin::types::FieldTable::default(),
            )
            .await
            .unwrap();

        channel
            .queue_bind(
                &config.queue,
                &exchange,
                "",
                lapin::options::QueueBindOptions::default(),
                lapin::types::FieldTable::default(),
            )
            .await
            .unwrap();

        trace!("Sending event...");
        let now = Utc::now();
        send_event(
            &channel,
            &exchange,
            routing_key,
            "my message",
            now.timestamp_millis(),
        )
        .await;

        trace!("Receiving event...");
        let (tx, rx) = SourceSender::new_test();
        tokio::spawn(
            amqp_source(&config, ShutdownSignal::noop(), tx)
                .await
                .unwrap(),
        );
        let events = collect_n(rx, 1).await;

        assert!(!events.is_empty());

        let log = events[0].as_log();
        trace!("{:?}", log);
        assert_eq!(log[log_schema().message_key()], "my message".into());
        assert_eq!(log["message_key"], routing_key.into());
        assert_eq!(log[log_schema().source_type_key()], "amqp".into());
        let log_ts = log[log_schema().timestamp_key()].as_timestamp().unwrap();
        assert!(log_ts.signed_duration_since(now) < chrono::Duration::seconds(1));
        assert_eq!(log["exchange"], exchange.into());
    }
}
