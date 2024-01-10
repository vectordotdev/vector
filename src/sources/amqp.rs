//! `AMQP` source.
//! Handles version AMQP 0.9.1 which is used by RabbitMQ.
use crate::{
    amqp::AmqpConfig,
    codecs::{Decoder, DecodingConfig},
    config::{SourceConfig, SourceContext, SourceOutput},
    event::{BatchNotifier, BatchStatus},
    internal_events::{
        source::{AmqpAckError, AmqpBytesReceived, AmqpEventError, AmqpRejectError},
        StreamClosedError,
    },
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    SourceSender,
};
use async_stream::stream;
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{FutureExt, StreamExt};
use futures_util::Stream;
use lapin::{acker::Acker, message::Delivery, Channel};
use snafu::Snafu;
use std::{io::Cursor, pin::Pin};
use tokio_util::codec::FramedRead;
use vector_lib::codecs::decoding::{DeserializerConfig, FramingConfig};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::{lookup_v2::OptionalValuePath, metadata_path, owned_value_path, path};
use vector_lib::{
    config::{log_schema, LegacyKey, LogNamespace, SourceAcknowledgementsConfig},
    event::Event,
    EstimatedJsonEncodedSizeOf,
};
use vector_lib::{
    finalizer::UnorderedFinalizer,
    internal_event::{CountByteSize, EventsReceived, InternalEventHandle as _},
};
use vrl::value::Kind;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Could not create AMQP consumer: {}", source))]
    AmqpCreateError {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[snafu(display("Could not subscribe to AMQP queue: {}", source))]
    AmqpSubscribeError { source: lapin::Error },
}

/// Configuration for the `amqp` source.
///
/// Supports AMQP version 0.9.1
#[configurable_component(source(
    "amqp",
    "Collect events from AMQP 0.9.1 compatible brokers like RabbitMQ."
))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct AmqpSourceConfig {
    /// The name of the queue to consume.
    #[serde(default = "default_queue")]
    pub(crate) queue: String,

    /// The identifier for the consumer.
    #[serde(default = "default_consumer")]
    #[configurable(metadata(docs::examples = "consumer-group-name"))]
    pub(crate) consumer: String,

    #[serde(flatten)]
    pub(crate) connection: AmqpConfig,

    /// The `AMQP` routing key.
    #[serde(default = "default_routing_key_field")]
    #[derivative(Default(value = "default_routing_key_field()"))]
    pub(crate) routing_key_field: OptionalValuePath,

    /// The `AMQP` exchange key.
    #[serde(default = "default_exchange_key")]
    #[derivative(Default(value = "default_exchange_key()"))]
    pub(crate) exchange_key: OptionalValuePath,

    /// The `AMQP` offset key.
    #[serde(default = "default_offset_key")]
    #[derivative(Default(value = "default_offset_key()"))]
    pub(crate) offset_key: OptionalValuePath,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
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
    pub(crate) acknowledgements: SourceAcknowledgementsConfig,
}

fn default_queue() -> String {
    "vector".into()
}

fn default_consumer() -> String {
    "vector".into()
}

fn default_routing_key_field() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("routing"))
}

fn default_exchange_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("exchange"))
}

fn default_offset_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("offset"))
}

impl_generate_config_from_default!(AmqpSourceConfig);

impl AmqpSourceConfig {
    fn decoder(&self, log_namespace: LogNamespace) -> vector_lib::Result<Decoder> {
        DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace).build()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "amqp")]
impl SourceConfig for AmqpSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        amqp_source(self, cx.shutdown, cx.out, log_namespace, acknowledgements).await
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                AmqpSourceConfig::NAME,
                None,
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some("timestamp"),
            )
            .with_source_metadata(
                AmqpSourceConfig::NAME,
                self.routing_key_field
                    .path
                    .clone()
                    .map(LegacyKey::InsertIfEmpty),
                &owned_value_path!("routing"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                AmqpSourceConfig::NAME,
                self.exchange_key.path.clone().map(LegacyKey::InsertIfEmpty),
                &owned_value_path!("exchange"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                AmqpSourceConfig::NAME,
                self.offset_key.path.clone().map(LegacyKey::InsertIfEmpty),
                &owned_value_path!("offset"),
                Kind::integer(),
                None,
            );

        vec![SourceOutput::new_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
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
    config: &AmqpSourceConfig,
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
        .map_err(|source| BuildError::AmqpCreateError { source })?;

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
    routing_key_field: &'a OptionalValuePath,
    routing: &'a str,
    exchange_key: &'a OptionalValuePath,
    exchange: &'a str,
    offset_key: &'a OptionalValuePath,
    delivery_tag: i64,
}

/// Populates the decoded event with extra metadata.
fn populate_event(
    event: &mut Event,
    timestamp: Option<chrono::DateTime<Utc>>,
    keys: &Keys<'_>,
    log_namespace: LogNamespace,
) {
    let log = event.as_mut_log();

    log_namespace.insert_source_metadata(
        AmqpSourceConfig::NAME,
        log,
        keys.routing_key_field
            .path
            .as_ref()
            .map(LegacyKey::InsertIfEmpty),
        path!("routing"),
        keys.routing.to_string(),
    );

    log_namespace.insert_source_metadata(
        AmqpSourceConfig::NAME,
        log,
        keys.exchange_key
            .path
            .as_ref()
            .map(LegacyKey::InsertIfEmpty),
        path!("exchange"),
        keys.exchange.to_string(),
    );

    log_namespace.insert_source_metadata(
        AmqpSourceConfig::NAME,
        log,
        keys.offset_key.path.as_ref().map(LegacyKey::InsertIfEmpty),
        path!("offset"),
        keys.delivery_tag,
    );

    log_namespace.insert_vector_metadata(
        log,
        log_schema().source_type_key(),
        path!("source_type"),
        Bytes::from_static(AmqpSourceConfig::NAME.as_bytes()),
    );

    // This handles the transition from the original timestamp logic. Originally the
    // `timestamp_key` was populated by the `properties.timestamp()` time on the message, falling
    // back to calling `now()`.
    match log_namespace {
        LogNamespace::Vector => {
            if let Some(timestamp) = timestamp {
                log.insert(
                    metadata_path!(AmqpSourceConfig::NAME, "timestamp"),
                    timestamp,
                );
            };

            log.insert(metadata_path!("vector", "ingest_timestamp"), Utc::now());
        }
        LogNamespace::Legacy => {
            if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
                log.try_insert(timestamp_key, timestamp.unwrap_or_else(Utc::now));
            }
        }
    };
}

/// Receives an event from `AMQP` and pushes it along the pipeline.
async fn receive_event(
    config: &AmqpSourceConfig,
    out: &mut SourceSender,
    log_namespace: LogNamespace,
    finalizer: Option<&UnorderedFinalizer<FinalizerEntry>>,
    msg: Delivery,
) -> Result<(), ()> {
    let payload = Cursor::new(Bytes::copy_from_slice(&msg.data));
    let decoder = config.decoder(log_namespace).map_err(|_e| ())?;
    let mut stream = FramedRead::new(payload, decoder);

    // Extract timestamp from AMQP message
    let timestamp = msg
        .properties
        .timestamp()
        .and_then(|millis| Utc.timestamp_millis_opt(millis as _).latest());

    let routing = msg.routing_key.to_string();
    let exchange = msg.exchange.to_string();
    let keys = Keys {
        routing_key_field: &config.routing_key_field,
        exchange_key: &config.exchange_key,
        offset_key: &config.offset_key,
        routing: &routing,
        exchange: &exchange,
        delivery_tag: msg.delivery_tag as i64,
    };
    let events_received = register!(EventsReceived);

    let stream = stream! {
        while let Some(result) = stream.next().await {
            match result {
                Ok((events, byte_size)) => {
                    emit!(AmqpBytesReceived {
                        byte_size,
                        protocol: "amqp_0_9_1",
                    });

                    events_received.emit(CountByteSize(
                        events.len(),
                        events.estimated_json_encoded_size_of(),
                    ));

                    for mut event in events {
                        populate_event(&mut event,
                                       timestamp,
                                       &keys,
                                       log_namespace);

                        yield event;
                    }
                }
                Err(error) => {
                    use vector_lib::codecs::StreamDecodingError as _;

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

    finalize_event_stream(finalizer, out, stream, msg).await;

    Ok(())
}

/// Send the event stream created by the framed read to the `out` stream.
async fn finalize_event_stream(
    finalizer: Option<&UnorderedFinalizer<FinalizerEntry>>,
    out: &mut SourceSender,
    mut stream: Pin<Box<dyn Stream<Item = Event> + Send + '_>>,
    msg: Delivery,
) {
    match finalizer {
        Some(finalizer) => {
            let (batch, receiver) = BatchNotifier::new_with_receiver();
            let mut stream = stream.map(|event| event.with_batch_notifier(&batch));

            match out.send_event_stream(&mut stream).await {
                Err(_) => {
                    emit!(StreamClosedError { count: 1 });
                }
                Ok(_) => {
                    finalizer.add(msg.into(), receiver);
                }
            }
        }
        None => match out.send_event_stream(&mut stream).await {
            Err(_) => {
                emit!(StreamClosedError { count: 1 });
            }
            Ok(_) => {
                let ack_options = lapin::options::BasicAckOptions::default();
                if let Err(error) = msg.acker.ack(ack_options).await {
                    emit!(AmqpAckError { error });
                }
            }
        },
    }
}

/// Runs the `AMQP` source involving the main loop pulling data from the server.
async fn run_amqp_source(
    config: AmqpSourceConfig,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
    channel: Channel,
    log_namespace: LogNamespace,
    acknowledgements: bool,
) -> Result<(), ()> {
    let (finalizer, mut ack_stream) =
        UnorderedFinalizer::<FinalizerEntry>::maybe_new(acknowledgements, Some(shutdown.clone()));

    debug!("Starting amqp source, listening to queue {}.", config.queue);
    let mut consumer = channel
        .basic_consume(
            &config.queue,
            &config.consumer,
            lapin::options::BasicConsumeOptions::default(),
            lapin::types::FieldTable::default(),
        )
        .await
        .map_err(|error| {
            error!(message = "Failed to consume.", error = ?error, internal_log_rate_limit = true);
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
                            emit!(AmqpEventError { error });
                            return Err(());
                        }
                        Ok(msg) => {
                            receive_event(&config, &mut out, log_namespace, finalizer.as_ref(), msg).await?
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
                emit!(AmqpAckError { error });
            }
        }
        BatchStatus::Errored => {
            let ack_options = lapin::options::BasicRejectOptions::default();
            if let Err(error) = entry.acker.reject(ack_options).await {
                emit!(AmqpRejectError { error });
            }
        }
        BatchStatus::Rejected => {
            let ack_options = lapin::options::BasicRejectOptions::default();
            if let Err(error) = entry.acker.reject(ack_options).await {
                emit!(AmqpRejectError { error });
            }
        }
    }
}

#[cfg(test)]
pub mod test {
    use vector_lib::lookup::OwnedTargetPath;
    use vector_lib::schema::Definition;
    use vector_lib::tls::TlsConfig;
    use vrl::value::kind::Collection;

    use super::*;

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
        let host = std::env::var("AMQP_HOST").unwrap_or_else(|_| "rabbitmq".to_string());
        let vhost = std::env::var("AMQP_VHOST").unwrap_or_else(|_| "%2f".to_string());
        config.connection.connection_string =
            format!("amqp://{}:{}@{}:5672/{}", user, pass, host, vhost);

        config
    }

    pub fn make_tls_config() -> AmqpSourceConfig {
        let mut config = AmqpSourceConfig {
            queue: "it".to_string(),
            ..Default::default()
        };
        let user = std::env::var("AMQP_USER").unwrap_or_else(|_| "guest".to_string());
        let pass = std::env::var("AMQP_PASSWORD").unwrap_or_else(|_| "guest".to_string());
        let vhost = std::env::var("AMQP_VHOST").unwrap_or_else(|_| "%2f".to_string());
        let host = std::env::var("AMQP_HOST").unwrap_or_else(|_| "rabbitmq".to_string());
        let ca_file =
            std::env::var("AMQP_CA_FILE").unwrap_or_else(|_| "/certs/ca.cert.pem".to_string());
        config.connection.connection_string =
            format!("amqps://{}:{}@{}/{}", user, pass, host, vhost);
        let tls = TlsConfig {
            ca_file: Some(ca_file.as_str().into()),
            ..Default::default()
        };
        config.connection.tls = Some(tls);
        config
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = AmqpSourceConfig {
            log_namespace: Some(true),
            ..Default::default()
        };

        let definition = config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        let expected_definition =
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                .with_meaning(OwnedTargetPath::event_root(), "message")
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("amqp", "timestamp"),
                    Kind::timestamp(),
                    Some("timestamp"),
                )
                .with_metadata_field(&owned_value_path!("amqp", "routing"), Kind::bytes(), None)
                .with_metadata_field(&owned_value_path!("amqp", "exchange"), Kind::bytes(), None)
                .with_metadata_field(&owned_value_path!("amqp", "offset"), Kind::integer(), None);

        assert_eq!(definition, Some(expected_definition));
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = AmqpSourceConfig::default();

        let definition = config
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        let expected_definition = Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Legacy],
        )
        .with_event_field(
            &owned_value_path!("message"),
            Kind::bytes(),
            Some("message"),
        )
        .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
        .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
        .with_event_field(&owned_value_path!("routing"), Kind::bytes(), None)
        .with_event_field(&owned_value_path!("exchange"), Kind::bytes(), None)
        .with_event_field(&owned_value_path!("offset"), Kind::integer(), None);

        assert_eq!(definition, Some(expected_definition));
    }
}

/// Integration tests use the docker compose files in `scripts/integration/docker-compose.amqp.yml`.
#[cfg(feature = "amqp-integration-tests")]
#[cfg(test)]
mod integration_test {
    use super::test::*;
    use super::*;
    use crate::{
        amqp::await_connection,
        shutdown::ShutdownSignal,
        test_util::{
            components::{run_and_assert_source_compliance, SOURCE_TAGS},
            random_string,
        },
        SourceSender,
    };
    use chrono::Utc;
    use lapin::options::*;
    use lapin::BasicProperties;
    use tokio::time::Duration;
    use vector_lib::config::log_schema;

    #[tokio::test]
    async fn amqp_source_create_ok() {
        let config = make_config();
        await_connection(&config.connection).await;
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

    #[tokio::test]
    async fn amqp_tls_source_create_ok() {
        let config = make_tls_config();
        await_connection(&config.connection).await;

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
        trace!("Sending message of length {} to {}.", payload_len, exchange,);

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

    async fn source_consume_event(mut config: AmqpSourceConfig) {
        let exchange = format!("test-{}-exchange", random_string(10));
        let queue = format!("test-{}-queue", random_string(10));
        let routing_key = "my_key";
        trace!("Test exchange name: {}.", exchange);
        let consumer = format!("test-consumer-{}", random_string(10));

        config.consumer = consumer;
        config.queue = queue;

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
        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(1), &SOURCE_TAGS).await;
        assert!(!events.is_empty());

        let log = events[0].as_log();
        trace!("{:?}", log);
        assert_eq!(*log.get_message().unwrap(), "my message".into());
        assert_eq!(log["routing"], routing_key.into());
        assert_eq!(*log.get_source_type().unwrap(), "amqp".into());
        let log_ts = log[log_schema().timestamp_key().unwrap().to_string()]
            .as_timestamp()
            .unwrap();
        assert!(log_ts.signed_duration_since(now) < chrono::Duration::seconds(1));
        assert_eq!(log["exchange"], exchange.into());
    }

    #[tokio::test]
    async fn amqp_source_consume_event() {
        let config = make_config();
        await_connection(&config.connection).await;
        source_consume_event(config).await;
    }

    #[tokio::test]
    async fn amqp_tls_source_consume_event() {
        let config = make_tls_config();
        await_connection(&config.connection).await;
        source_consume_event(config).await;
    }
}
