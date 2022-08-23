use crate::{
    amqp::AMQPConfig,
    codecs::{Decoder, DecodingConfig},
    config::SourceContext,
    config::{log_schema, Output, SourceConfig, SourceDescription},
    event::{BatchNotifier, BatchStatus},
    internal_events::{
        source::{AMQPEventError, AMQPEventReceived},
        StreamClosedError,
    },
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    SourceSender,
};
use async_stream::stream;
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use codecs::decoding::{DeserializerConfig, FramingConfig};
use futures::{FutureExt, StreamExt};
use lapin::{acker::Acker, message::Delivery, Channel};
use snafu::Snafu;
use std::io::Cursor;
use tokio_util::codec::FramedRead;
use vector_common::finalizer::UnorderedFinalizer;
use vector_config::configurable_component;
use vector_core::{
    config::{AcknowledgementsConfig, LogNamespace},
    event::Event,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Could not create AMQP consumer: {}", source))]
    AMQPCreateError {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[snafu(display("Could not subscribe to AMQP queue: {}", source))]
    AMQPSubscribeError { source: lapin::Error },
}

/// Configuration for the `amqp` source.
#[configurable_component(source)]
#[derive(Clone, Debug, Derivative)]
#[serde(deny_unknown_fields)]
pub struct AMQPSourceConfig {
    /// The queue.
    pub(crate) queue: String,

    /// The consumer.
    pub(crate) consumer: String,

    /// Connection options for AMQP source.
    pub(crate) connection: AMQPConfig,

    /// The AMQP routing key.
    #[serde(default = "default_routing_key")]
    pub(crate) routing_key: String,

    /// The AMQP exchange key.
    #[serde(default = "default_exchange_key")]
    pub(crate) exchange_key: String,

    /// The AMQP offset key.
    #[serde(default = "default_offset_key")]
    pub(crate) offset_key: String,

    /// The namespace to use. This overrides the global setting.
    #[serde(default)]
    pub log_namespace: Option<bool>,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub(crate) framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub(crate) decoding: DeserializerConfig,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    pub(crate) acknowledgements: AcknowledgementsConfig,
}

fn default_routing_key() -> String {
    "routing".into()
}

fn default_exchange_key() -> String {
    "exchange".into()
}

fn default_offset_key() -> String {
    "offset".into()
}

impl Default for AMQPSourceConfig {
    fn default() -> Self {
        Self {
            queue: "vector".to_string(),
            consumer: "vector".to_string(),
            routing_key: default_routing_key(),
            exchange_key: default_exchange_key(),
            offset_key: default_offset_key(),
            connection: AMQPConfig::default(),
            framing: default_framing_message_based(),
            decoding: default_decoding(),
            log_namespace: None,
            acknowledgements: AcknowledgementsConfig::default(),
        }
    }
}

inventory::submit! {
    SourceDescription::new::<AMQPSourceConfig>("amqp")
}

impl_generate_config_from_default!(AMQPSourceConfig);

impl AMQPSourceConfig {
    fn decoder(&self, log_namespace: LogNamespace) -> Decoder {
        DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace).build()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "amqp")]
impl SourceConfig for AMQPSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let acknowledgements = cx.do_acknowledgements(&self.acknowledgements);

        amqp_source(self, cx.shutdown, cx.out, log_namespace, acknowledgements).await
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<Output> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();

        vec![Output::default(self.decoding.output_type()).with_schema_definition(schema_definition)]
    }

    fn source_type(&self) -> &'static str {
        "amqp"
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[derive(Debug)]
struct FinalizerEntry {
    acker: Acker,
}

impl From<Delivery> for FinalizerEntry {
    fn from(delivery: Delivery) -> Self {
        Self {
            acker: delivery.acker,
        }
    }
}

pub(crate) async fn amqp_source(
    config: &AMQPSourceConfig,
    shutdown: ShutdownSignal,
    out: SourceSender,
    log_namespace: LogNamespace,
    acknowledgements: bool,
) -> crate::Result<super::Source> {
    let config = config.clone();
    let (_conn, channel) = config
        .connection
        .connect()
        .await
        .map_err(|source| BuildError::AMQPCreateError { source })?;

    Ok(Box::pin(run_amqp_source(
        config,
        shutdown,
        out,
        channel,
        log_namespace,
        acknowledgements,
    )))
}

struct Keys<'a> {
    routing_key: &'a str,
    routing: &'a str,
    exchange_key: &'a str,
    exchange: &'a str,
    offset_key: &'a str,
    delivery_tag: i64,
}

/// Populates the decoded event with extra metadata.
fn populate_event(
    event: &mut Event,
    timestamp: chrono::DateTime<Utc>,
    keys: &Keys<'_>,
    log_namespace: LogNamespace,
) {
    let log = event.as_mut_log();

    log_namespace.insert_vector_metadata(
        log,
        log_schema().timestamp_key(),
        "ingest_timestamp",
        timestamp,
    );

    log_namespace.insert_vector_metadata(
        log,
        log_schema().source_type_key(),
        "source_type",
        "amqp",
    );

    log_namespace.insert_source_metadata(
        "amqp",
        log,
        keys.routing_key,
        "routing",
        keys.routing.to_string(),
    );

    log_namespace.insert_source_metadata(
        "amqp",
        log,
        keys.exchange_key,
        "exchange",
        keys.exchange.to_string(),
    );

    log_namespace.insert_source_metadata("amqp", log, keys.offset_key, "offset", keys.delivery_tag);
}

/// Runs the AMQP source involving the main loop pulling data from the server.
async fn run_amqp_source(
    config: AMQPSourceConfig,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
    channel: Channel,
    log_namespace: LogNamespace,
    acknowledgements: bool,
) -> Result<(), ()> {
    let (finalizer, mut ack_stream) =
        UnorderedFinalizer::<FinalizerEntry>::maybe_new(acknowledgements, shutdown.clone());

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
            error!(message = "Failed to consume.", error = ?error, internal_log_rate_secs = 10);
        })?
        .fuse();
    let mut shutdown = shutdown.fuse();
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            entry = ack_stream.next() => {
                if let Some((status, entry)) = entry {
                    handle_ack(status, entry).await;
                }
            },
            opt_m = consumer.next() => {
                if let Some(try_m) = opt_m {
                    match try_m {
                        Err(error) => {
                            emit!(AMQPEventError { error });
                            return Err(());
                        }
                        Ok(msg) => {
                            emit!(AMQPEventReceived {
                                byte_size: msg.data.len()
                            });

                            if msg.data.is_empty() {
                                return Err(());
                            }

                            let payload = Cursor::new(Bytes::copy_from_slice(&msg.data));
                            let mut stream = FramedRead::new(payload, config.decoder(log_namespace));

                            // Extract timestamp from amqp message
                            let timestamp = msg
                                .properties
                                .timestamp()
                                .and_then(|millis| Utc.timestamp_millis_opt(millis as _).latest())
                                .unwrap_or_else(Utc::now);

                            let routing = msg.routing_key.to_string();
                            let exchange = msg.exchange.to_string();
                            let keys = Keys {
                                routing_key: config.routing_key.as_str(),
                                exchange_key: config.exchange_key.as_str(),
                                offset_key: config.offset_key.as_str(),
                                routing: &routing,
                                exchange: &exchange,
                                delivery_tag: msg.delivery_tag as i64,
                            };

                            let out = &mut out;

                            let mut stream = stream! {
                                while let Some(result) = stream.next().await {
                                    match result {
                                        Ok((events, _byte_size)) => {
                                            for mut event in events {
                                                populate_event(&mut event,
                                                               timestamp,
                                                               &keys,
                                                               log_namespace);

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

                            match finalizer {
                                Some(ref finalizer) => {
                                    let (batch, receiver) = BatchNotifier::new_with_receiver();
                                    let mut stream = stream.map(|event| event.with_batch_notifier(&batch));

                                    match out.send_event_stream(&mut stream).await {
                                        Err(error) => {
                                            emit!(StreamClosedError { error, count: 1 });
                                        }
                                        Ok(_) => {
                                            // TODO Is this needed?
                                            // drop(stream);
                                            finalizer.add(msg.into(), receiver);
                                        }
                                    }
                                }
                                None => {
                                    match out.send_event_stream(&mut stream).await {
                                        Err(error) => {
                                            emit!(StreamClosedError { error, count: 1 });
                                        }
                                        Ok(_) => {
                                            let ack_options = lapin::options::BasicAckOptions::default();
                                            if let Err(error) = msg.acker.ack(ack_options).await {
                                                error!(message = "Unable to ack", error = ?error, internal_log_rate_secs = 10);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    break
                }
            }
        };
    }

    Ok(())
}

async fn handle_ack(status: BatchStatus, entry: FinalizerEntry) {
    match status {
        BatchStatus::Delivered => {
            let ack_options = lapin::options::BasicAckOptions::default();
            if let Err(error) = entry.acker.ack(ack_options).await {
                error!(message = "Unable to ack", error = ?error, internal_log_rate_secs = 10);
            }
        }
        BatchStatus::Errored => {
            let ack_options = lapin::options::BasicRejectOptions::default();
            if let Err(error) = entry.acker.reject(ack_options).await {
                error!(message = "Unable to reject", error = ?error, internal_log_rate_secs = 10);
            }
        }
        BatchStatus::Rejected => {
            let ack_options = lapin::options::BasicRejectOptions::default();
            if let Err(error) = entry.acker.reject(ack_options).await {
                error!(message = "Unable to reject", error = ?error, internal_log_rate_secs = 10);
            }
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AMQPSourceConfig>();
    }

    pub fn make_config() -> AMQPSourceConfig {
        let mut config = AMQPSourceConfig {
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

    #[tokio::test]
    async fn amqp_source_create_ok() {
        let config = make_config();
        assert!(amqp_source(
            &config,
            ShutdownSignal::noop(),
            SourceSender::new_test().0,
            LogNamespace::Legacy,
            false,
        )
        .await
        .is_ok());
    }

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
        config.routing_key = "message_key".to_string();
        config.exchange_key = "exchange".to_string();
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
            amqp_source(
                &config,
                ShutdownSignal::noop(),
                tx,
                LogNamespace::Legacy,
                false,
            )
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
