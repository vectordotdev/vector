//! `AMQP` source.
//! Handles version AMQP 0.9.1 which is used by RabbitMQ.
use std::{collections::BTreeMap, io::Cursor, pin::Pin};

use async_stream::stream;
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::{FutureExt, StreamExt};
use futures_util::Stream;
use lapin::{
    Acker, BasicProperties, Channel,
    message::Delivery,
    options::BasicQosOptions,
    types::{AMQPValue, LongString, ShortString},
};
use snafu::Snafu;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{
        DecoderFramedRead,
        decoding::{DeserializerConfig, FramingConfig},
    },
    config::{LegacyKey, LogNamespace, SourceAcknowledgementsConfig, log_schema},
    configurable::configurable_component,
    event::{Event, LogEvent},
    finalizer::UnorderedFinalizer,
    internal_event::{CountByteSize, EventsReceived, InternalEventHandle as _},
    lookup::{lookup_v2::OptionalValuePath, metadata_path, owned_value_path, path},
};
use vrl::value::{Kind, kind::Collection};
use vrl::value::{ObjectMap, Value};

use crate::{
    SourceSender,
    amqp::AmqpConfig,
    codecs::{Decoder, DecodingConfig},
    config::{SourceConfig, SourceContext, SourceOutput},
    event::{BatchNotifier, BatchStatus},
    internal_events::{
        StreamClosedError,
        source::{AmqpAckError, AmqpBytesReceived, AmqpEventError, AmqpRejectError},
    },
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Could not create AMQP consumer: {}", source))]
    AmqpCreateError {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
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

    /// The key where `AMQP` message headers are added to events.
    #[serde(default = "default_headers_key")]
    #[derivative(Default(value = "default_headers_key()"))]
    pub(crate) headers_key: OptionalValuePath,

    /// The key where `AMQP` message properties are added to events.
    #[serde(default = "default_properties_key")]
    #[derivative(Default(value = "default_properties_key()"))]
    pub(crate) properties_key: OptionalValuePath,

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

    /// Maximum number of unacknowledged messages the broker will deliver to this consumer.
    ///
    /// This controls flow control via AMQP QoS prefetch. Lower values limit memory usage and
    /// prevent overwhelming slow consumers, but may reduce throughput. Higher values increase
    /// throughput but consume more memory.
    ///
    /// If not set, the broker/client default applies (often unlimited).
    #[serde(default)]
    #[configurable(metadata(docs::examples = 100))]
    pub(crate) prefetch_count: Option<u16>,
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

fn default_headers_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("headers"))
}

fn default_properties_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("properties"))
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
            )
            .with_source_metadata(
                AmqpSourceConfig::NAME,
                self.headers_key.path.clone().map(LegacyKey::InsertIfEmpty),
                &owned_value_path!("headers"),
                Kind::object(Collection::empty().with_unknown(Kind::any())),
                None,
            )
            .with_source_metadata(
                AmqpSourceConfig::NAME,
                self.properties_key
                    .path
                    .clone()
                    .map(LegacyKey::InsertIfEmpty),
                &owned_value_path!("properties"),
                Kind::object(Collection::empty().with_unknown(Kind::any())),
                None,
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
    headers_key: &'a OptionalValuePath,
    headers: Value,
    properties_key: &'a OptionalValuePath,
    properties: Value,
}

fn long_string_to_value(value: &LongString) -> Value {
    match std::str::from_utf8(value.as_bytes()) {
        Ok(value) => value.to_owned().into(),
        Err(_) => Bytes::copy_from_slice(value.as_bytes()).into(),
    }
}

fn amqp_timestamp_to_datetime(value: u64) -> Option<chrono::DateTime<Utc>> {
    Utc.timestamp_opt(value as i64, 0).latest()
}

fn amqp_value_to_value(value: &AMQPValue) -> Value {
    match value {
        AMQPValue::Boolean(value) => (*value).into(),
        AMQPValue::ShortShortInt(value) => (*value as i64).into(),
        AMQPValue::ShortShortUInt(value) => (*value as i64).into(),
        AMQPValue::ShortInt(value) => (*value as i64).into(),
        AMQPValue::ShortUInt(value) => (*value as i64).into(),
        AMQPValue::LongInt(value) => (*value as i64).into(),
        AMQPValue::LongUInt(value) => (*value as i64).into(),
        AMQPValue::LongLongInt(value) => (*value).into(),
        AMQPValue::Float(value) => (*value as f64).into(),
        AMQPValue::Double(value) => (*value).into(),
        AMQPValue::DecimalValue(value) => Value::Object(BTreeMap::from([
            ("scale".into(), (value.scale as i64).into()),
            ("value".into(), (value.value as i64).into()),
        ])),
        AMQPValue::ShortString(value) => value.to_string().into(),
        AMQPValue::LongString(value) => long_string_to_value(value),
        AMQPValue::FieldArray(value) => {
            Value::Array(value.as_slice().iter().map(amqp_value_to_value).collect())
        }
        AMQPValue::Timestamp(value) => amqp_timestamp_to_datetime(*value)
            .map(Value::from)
            .unwrap_or_else(|| (*value as i64).into()),
        AMQPValue::FieldTable(value) => field_table_to_value(value),
        AMQPValue::ByteArray(value) => Bytes::copy_from_slice(value.as_slice()).into(),
        AMQPValue::Void => Value::Null,
    }
}

fn field_table_to_value<'a>(
    table: impl IntoIterator<Item = (&'a ShortString, &'a AMQPValue)>,
) -> Value {
    Value::Object(
        table
            .into_iter()
            .map(|(key, value)| (key.to_string().into(), amqp_value_to_value(value)))
            .collect(),
    )
}

fn insert_short_string(properties: &mut ObjectMap, key: &str, value: &Option<ShortString>) {
    if let Some(value) = value {
        properties.insert(key.into(), value.to_string().into());
    }
}

fn basic_properties_to_value(properties: &BasicProperties) -> Value {
    let mut values = BTreeMap::new();

    insert_short_string(&mut values, "content_type", properties.content_type());
    insert_short_string(
        &mut values,
        "content_encoding",
        properties.content_encoding(),
    );
    if let Some(value) = properties.delivery_mode() {
        values.insert("delivery_mode".into(), (*value as i64).into());
    }
    if let Some(value) = properties.priority() {
        values.insert("priority".into(), (*value as i64).into());
    }
    insert_short_string(&mut values, "correlation_id", properties.correlation_id());
    insert_short_string(&mut values, "reply_to", properties.reply_to());
    insert_short_string(&mut values, "expiration", properties.expiration());
    insert_short_string(&mut values, "message_id", properties.message_id());
    if let Some(value) = properties.timestamp() {
        values.insert(
            "timestamp".into(),
            amqp_timestamp_to_datetime(*value)
                .map(Value::from)
                .unwrap_or_else(|| (*value as i64).into()),
        );
    }
    insert_short_string(&mut values, "type", properties.kind());
    insert_short_string(&mut values, "user_id", properties.user_id());
    insert_short_string(&mut values, "app_id", properties.app_id());
    insert_short_string(&mut values, "cluster_id", properties.cluster_id());

    Value::Object(values)
}

/// Populates the decoded event with extra metadata.
fn populate_log_event(
    log: &mut LogEvent,
    timestamp: Option<chrono::DateTime<Utc>>,
    keys: &Keys<'_>,
    log_namespace: LogNamespace,
) {
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

    log_namespace.insert_source_metadata(
        AmqpSourceConfig::NAME,
        log,
        keys.headers_key.path.as_ref().map(LegacyKey::InsertIfEmpty),
        path!("headers"),
        keys.headers.clone(),
    );

    log_namespace.insert_source_metadata(
        AmqpSourceConfig::NAME,
        log,
        keys.properties_key
            .path
            .as_ref()
            .map(LegacyKey::InsertIfEmpty),
        path!("properties"),
        keys.properties.clone(),
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
    let mut stream = DecoderFramedRead::new(payload, decoder);

    // Extract AMQP message timestamp. AMQP timestamps are Unix timestamps in seconds.
    let timestamp = msg
        .properties
        .timestamp()
        .and_then(amqp_timestamp_to_datetime);

    let routing = msg.routing_key.to_string();
    let exchange = msg.exchange.to_string();
    let headers = msg
        .properties
        .headers()
        .as_ref()
        .map(field_table_to_value)
        .unwrap_or_else(|| Value::Object(BTreeMap::new()));
    let properties = basic_properties_to_value(&msg.properties);
    let keys = Keys {
        routing_key_field: &config.routing_key_field,
        exchange_key: &config.exchange_key,
        offset_key: &config.offset_key,
        headers_key: &config.headers_key,
        properties_key: &config.properties_key,
        routing: &routing,
        exchange: &exchange,
        delivery_tag: msg.delivery_tag as i64,
        headers,
        properties,
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
                        if let Event::Log(ref mut log) = event {
                            populate_log_event(log,
                                        timestamp,
                                        &keys,
                                        log_namespace);
                        }

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

    // Apply AMQP QoS (prefetch) before starting consumption.
    if let Some(count) = config.prefetch_count {
        // per-consumer prefetch (global = false)
        channel
            .basic_qos(count, BasicQosOptions { global: false })
            .await
            .map_err(|error| {
                error!(message = "Failed to apply basic_qos.", ?error);
            })?;
    }

    debug!("Starting amqp source, listening to queue {}.", config.queue);
    let mut consumer = channel
        .basic_consume(
            config.queue.clone().into(),
            config.consumer.clone().into(),
            lapin::options::BasicConsumeOptions::default(),
            lapin::types::FieldTable::default(),
        )
        .await
        .map_err(|error| {
            error!(message = "Failed to consume.", ?error);
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
    use std::collections::BTreeMap;

    use chrono::TimeZone;
    use lapin::{
        BasicProperties,
        types::{AMQPValue, FieldArray, FieldTable, ShortString},
    };
    use vector_lib::{lookup::OwnedTargetPath, schema::Definition, tls::TlsConfig};
    use vrl::value::Value;

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
        config.connection.connection_string = format!("amqp://{user}:{pass}@{host}:5672/{vhost}");

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
        config.connection.connection_string = format!("amqps://{user}:{pass}@{host}/{vhost}");
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
                .with_metadata_field(&owned_value_path!("amqp", "offset"), Kind::integer(), None)
                .with_metadata_field(
                    &owned_value_path!("amqp", "headers"),
                    Kind::object(Collection::empty().with_unknown(Kind::any())),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("amqp", "properties"),
                    Kind::object(Collection::empty().with_unknown(Kind::any())),
                    None,
                );

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
        .with_event_field(&owned_value_path!("offset"), Kind::integer(), None)
        .with_event_field(
            &owned_value_path!("headers"),
            Kind::object(Collection::empty().with_unknown(Kind::any())),
            None,
        )
        .with_event_field(
            &owned_value_path!("properties"),
            Kind::object(Collection::empty().with_unknown(Kind::any())),
            None,
        );

        assert_eq!(definition, Some(expected_definition));
    }

    #[test]
    fn amqp_field_table_to_value_preserves_supported_types() {
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();

        let mut nested = FieldTable::default();
        nested.insert("nested-key".into(), AMQPValue::Boolean(true));

        let mut table = FieldTable::default();
        table.insert(
            "string".into(),
            AMQPValue::LongString(String::from("value").into()),
        );
        table.insert("bytes".into(), AMQPValue::LongString(vec![0, 159].into()));
        table.insert("bool".into(), AMQPValue::Boolean(true));
        table.insert("int".into(), AMQPValue::LongLongInt(42));
        table.insert("uint".into(), AMQPValue::LongUInt(7));
        table.insert("timestamp".into(), AMQPValue::Timestamp(1_700_000_000));
        table.insert("nested".into(), AMQPValue::FieldTable(nested));
        table.insert(
            "array".into(),
            AMQPValue::FieldArray(FieldArray::from(vec![
                AMQPValue::ShortString(ShortString::from("one")),
                AMQPValue::Boolean(false),
            ])),
        );

        assert_eq!(
            field_table_to_value(&table),
            Value::Object(BTreeMap::from([
                (
                    "array".into(),
                    Value::Array(vec!["one".into(), false.into()])
                ),
                ("bool".into(), true.into()),
                ("bytes".into(), Value::Bytes(vec![0, 159].into())),
                ("int".into(), 42.into()),
                (
                    "nested".into(),
                    Value::Object(BTreeMap::from([("nested-key".into(), true.into())])),
                ),
                ("string".into(), "value".into()),
                ("timestamp".into(), timestamp.into()),
                ("uint".into(), 7.into()),
            ]))
        );
    }

    #[test]
    fn basic_properties_to_value_includes_scalar_properties() {
        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();

        let properties = BasicProperties::default()
            .with_content_type("application/json".into())
            .with_content_encoding("gzip".into())
            .with_delivery_mode(2)
            .with_priority(5)
            .with_correlation_id("correlation".into())
            .with_reply_to("reply".into())
            .with_expiration("60000".into())
            .with_message_id("message-id".into())
            .with_timestamp(1_700_000_000)
            .with_type("type".into())
            .with_user_id("user".into())
            .with_app_id("app".into());

        assert_eq!(
            basic_properties_to_value(&properties),
            Value::Object(BTreeMap::from([
                ("app_id".into(), "app".into()),
                ("content_encoding".into(), "gzip".into()),
                ("content_type".into(), "application/json".into()),
                ("correlation_id".into(), "correlation".into()),
                ("delivery_mode".into(), 2.into()),
                ("expiration".into(), "60000".into()),
                ("message_id".into(), "message-id".into()),
                ("priority".into(), 5.into()),
                ("reply_to".into(), "reply".into()),
                ("timestamp".into(), timestamp.into()),
                ("type".into(), "type".into()),
                ("user_id".into(), "user".into()),
            ]))
        );
    }

    #[test]
    fn amqp_timestamp_to_datetime_uses_unix_seconds() {
        assert_eq!(
            amqp_timestamp_to_datetime(1_700_000_000),
            Utc.timestamp_opt(1_700_000_000, 0).latest()
        );
    }
}

/// Integration tests use the docker compose files in `tests/integration/docker-compose.amqp.yml`.
#[cfg(feature = "amqp-integration-tests")]
#[cfg(test)]
mod integration_test {
    use chrono::Utc;
    use lapin::types::{AMQPValue, FieldTable, ShortString};
    use lapin::{BasicProperties, options::*};
    use tokio::time::Duration;
    use vector_lib::config::log_schema;

    use super::{test::*, *};
    use crate::{
        SourceSender,
        amqp::await_connection,
        shutdown::ShutdownSignal,
        test_util::{
            components::{SOURCE_TAGS, run_and_assert_source_compliance},
            random_string,
        },
    };

    #[tokio::test]
    async fn amqp_source_create_ok() {
        let config = make_config();
        await_connection(&config.connection).await;
        assert!(
            amqp_source(
                &config,
                ShutdownSignal::noop(),
                SourceSender::new_test().0,
                LogNamespace::Legacy,
                false,
            )
            .await
            .is_ok()
        );
    }

    #[tokio::test]
    async fn amqp_tls_source_create_ok() {
        let config = make_tls_config();
        await_connection(&config.connection).await;

        assert!(
            amqp_source(
                &config,
                ShutdownSignal::noop(),
                SourceSender::new_test().0,
                LogNamespace::Legacy,
                false,
            )
            .await
            .is_ok()
        );
    }

    async fn send_event(
        channel: &lapin::Channel,
        exchange: &str,
        routing_key: &str,
        text: &str,
        properties: BasicProperties,
    ) {
        let payload = text.as_bytes();
        let payload_len = payload.len();
        trace!("Sending message of length {} to {}.", payload_len, exchange,);

        channel
            .basic_publish(
                exchange.into(),
                routing_key.into(),
                BasicPublishOptions::default(),
                payload.as_ref(),
                properties,
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
        let exchange: ShortString = exchange.into();
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
                exchange.clone(),
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
        let queue: ShortString = config.queue.clone().into();
        channel
            .queue_declare(
                queue.clone(),
                queue_opts,
                lapin::types::FieldTable::default(),
            )
            .await
            .unwrap();

        channel
            .queue_bind(
                queue,
                exchange.clone(),
                "".into(),
                lapin::options::QueueBindOptions::default(),
                lapin::types::FieldTable::default(),
            )
            .await
            .unwrap();

        trace!("Sending event...");
        let now = Utc::now();
        send_event(
            &channel,
            exchange.as_str(),
            routing_key,
            "my message",
            BasicProperties::default(),
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
        assert_eq!(log["exchange"], exchange.as_str().into());
    }

    async fn source_consume_event_with_headers_and_properties(mut config: AmqpSourceConfig) {
        let exchange = format!("test-{}-exchange", random_string(10));
        let queue = format!("test-{}-queue", random_string(10));
        let routing_key = "my_key";
        let exchange: ShortString = exchange.into();

        config.consumer = format!("test-consumer-{}", random_string(10));
        config.queue = queue;

        let (_conn, channel) = config.connection.connect().await.unwrap();
        channel
            .exchange_declare(
                exchange.clone(),
                lapin::ExchangeKind::Fanout,
                ExchangeDeclareOptions {
                    auto_delete: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .unwrap();

        let queue: ShortString = config.queue.clone().into();
        channel
            .queue_declare(
                queue.clone(),
                QueueDeclareOptions {
                    auto_delete: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .unwrap();

        channel
            .queue_bind(
                queue,
                exchange.clone(),
                "".into(),
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .unwrap();

        let mut headers = FieldTable::default();
        headers.insert(
            "x-request-id".into(),
            AMQPValue::LongString(String::from("abc123").into()),
        );
        headers.insert("retry".into(), AMQPValue::Boolean(true));

        let timestamp = Utc.timestamp_opt(1_700_000_000, 0).unwrap();

        let properties = BasicProperties::default()
            .with_headers(headers)
            .with_content_type("application/json".into())
            .with_priority(7)
            .with_timestamp(1_700_000_000);

        send_event(
            &channel,
            exchange.as_str(),
            routing_key,
            "my message",
            properties,
        )
        .await;

        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(1), &SOURCE_TAGS).await;
        let log = events[0].as_log();

        assert_eq!(log["headers.x-request-id"], "abc123".into());
        assert_eq!(log["headers.retry"], true.into());
        assert_eq!(log["properties.content_type"], "application/json".into());
        assert_eq!(log["properties.priority"], 7.into());
        assert_eq!(
            log[log_schema().timestamp_key().unwrap().to_string()]
                .as_timestamp()
                .copied(),
            Some(timestamp)
        );
        assert_eq!(log["properties.timestamp"], timestamp.into());
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

    #[tokio::test]
    async fn amqp_source_consume_event_with_headers_and_properties() {
        let config = make_config();
        await_connection(&config.connection).await;
        source_consume_event_with_headers_and_properties(config).await;
    }
}
