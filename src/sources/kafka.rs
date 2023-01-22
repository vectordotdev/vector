use std::{
    collections::{BTreeMap, HashMap},
    io::Cursor,
    sync::Arc,
    time::Duration,
};

use async_stream::stream;
use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use futures::{Stream, StreamExt};
use lookup::{lookup_v2::OptionalValuePath, owned_value_path, path, OwnedValuePath};
use once_cell::sync::OnceCell;
use rdkafka::{
    consumer::{Consumer, ConsumerContext, Rebalance, StreamConsumer},
    message::{BorrowedMessage, Headers as _, Message},
    ClientConfig, ClientContext, Statistics,
};
use serde_with::serde_as;
use snafu::{ResultExt, Snafu};
use tokio_util::codec::FramedRead;

use value::{kind::Collection, Kind};
use vector_common::finalizer::OrderedFinalizer;
use vector_config::{configurable_component, NamedComponent};
use vector_core::{
    config::{LegacyKey, LogNamespace},
    EstimatedJsonEncodedSizeOf,
};

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{
        log_schema, LogSchema, Output, SourceAcknowledgementsConfig, SourceConfig, SourceContext,
    },
    event::{BatchNotifier, BatchStatus, Event, Value},
    internal_events::{
        KafkaBytesReceived, KafkaEventsReceived, KafkaOffsetUpdateError, KafkaReadError,
        StreamClosedError,
    },
    kafka,
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    SourceSender,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Could not create Kafka consumer: {}", source))]
    KafkaCreateError { source: rdkafka::error::KafkaError },
    #[snafu(display("Could not subscribe to Kafka topics: {}", source))]
    KafkaSubscribeError { source: rdkafka::error::KafkaError },
}

/// Configuration for the `kafka` source.
#[serde_as]
#[configurable_component(source("kafka"))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct KafkaSourceConfig {
    /// A comma-separated list of Kafka bootstrap servers.
    ///
    /// These are the servers in a Kafka cluster that a client should use to "bootstrap" its connection to the cluster,
    /// allowing discovering all other hosts in the cluster.
    ///
    /// Must be in the form of `host:port`, and comma-separated.
    #[configurable(metadata(docs::examples = "10.14.22.123:9092,10.14.23.332:9092"))]
    bootstrap_servers: String,

    /// The Kafka topics names to read events from.
    ///
    /// Regular expression syntax is supported if the topic begins with `^`.
    #[configurable(metadata(docs::examples = "^(prefix1|prefix2)-.+"))]
    #[configurable(metadata(docs::examples = "topic-1"))]
    #[configurable(metadata(docs::examples = "topic-2"))]
    topics: Vec<String>,

    /// The consumer group name to be used to consume events from Kafka.
    #[configurable(metadata(docs::examples = "consumer-group-name"))]
    group_id: String,

    /// Override dynamic membership and broker assignment behavior with static membership, using a group instance (member) id.
    #[configurable(metadata(docs::examples = "kafka-streams-instance-1"))]
    group_instance_id: Option<String>,

    /// If offsets for consumer group do not exist, set them using this strategy.
    ///
    /// See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for the `auto.offset.reset` option for further clarification.
    #[serde(default = "default_auto_offset_reset")]
    #[configurable(metadata(docs::examples = "example_auto_offset_reset_values()"))]
    auto_offset_reset: String,

    /// The Kafka session timeout.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::examples = 5000))]
    #[configurable(metadata(docs::examples = 10000))]
    #[serde(default = "default_session_timeout_ms")]
    session_timeout_ms: Duration,

    /// Timeout for network requests.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::examples = 30000))]
    #[configurable(metadata(docs::examples = 60000))]
    #[serde(default = "default_socket_timeout_ms")]
    socket_timeout_ms: Duration,

    /// Maximum time the broker may wait to fill the response.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::examples = 50))]
    #[configurable(metadata(docs::examples = 100))]
    #[serde(default = "default_fetch_wait_max_ms")]
    fetch_wait_max_ms: Duration,

    /// The frequency that the consumer offsets are committed (written) to offset storage.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[serde(default = "default_commit_interval_ms")]
    #[configurable(metadata(docs::examples = 5000))]
    #[configurable(metadata(docs::examples = 10000))]
    commit_interval_ms: Duration,

    /// Overrides the name of the log field used to add the message key to each event.
    ///
    /// The value will be the message key of the Kafka message itself.
    ///
    /// By default, `"message_key"` is used.
    #[serde(default = "default_key_field")]
    #[configurable(metadata(docs::examples = "message_key"))]
    key_field: OptionalValuePath,

    /// Overrides the name of the log field used to add the topic to each event.
    ///
    /// The value will be the topic from which the Kafka message was consumed from.
    ///
    /// By default, `"topic"` is used.
    #[serde(default = "default_topic_key")]
    #[configurable(metadata(docs::examples = "topic"))]
    topic_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the partition to each event.
    ///
    /// The value will be the partition from which the Kafka message was consumed from.
    ///
    /// By default, `"partition"` is used.
    #[serde(default = "default_partition_key")]
    #[configurable(metadata(docs::examples = "partition"))]
    partition_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the offset to each event.
    ///
    /// The value will be the offset of the Kafka message itself.
    ///
    /// By default, `"offset"` is used.
    #[serde(default = "default_offset_key")]
    #[configurable(metadata(docs::examples = "offset"))]
    offset_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the headers to each event.
    ///
    /// The value will be the headers of the Kafka message itself.
    ///
    /// By default, `"headers"` is used.
    #[serde(default = "default_headers_key")]
    #[configurable(metadata(docs::examples = "headers"))]
    headers_key: OptionalValuePath,

    /// Advanced options set directly on the underlying `librdkafka` client.
    ///
    /// See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for details.
    #[configurable(metadata(docs::examples = "example_librdkafka_options()"))]
    #[configurable(metadata(
        docs::additional_props_description = "A librdkafka configuration option."
    ))]
    librdkafka_options: Option<HashMap<String, String>>,

    #[serde(flatten)]
    auth: kafka::KafkaAuthConfig,

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

impl KafkaSourceConfig {
    fn keys(&self) -> Keys {
        Keys::from(log_schema(), self)
    }
}

const fn default_session_timeout_ms() -> Duration {
    Duration::from_millis(10000) // default in librdkafka
}

const fn default_socket_timeout_ms() -> Duration {
    Duration::from_millis(60000) // default in librdkafka
}

const fn default_fetch_wait_max_ms() -> Duration {
    Duration::from_millis(100) // default in librdkafka
}

const fn default_commit_interval_ms() -> Duration {
    Duration::from_millis(5000)
}

fn default_auto_offset_reset() -> String {
    "largest".into() // default in librdkafka
}

fn default_key_field() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("message_key"))
}

fn default_topic_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("topic"))
}

fn default_partition_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("partition"))
}

fn default_offset_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("offset"))
}

fn default_headers_key() -> OptionalValuePath {
    OptionalValuePath::from(owned_value_path!("headers"))
}

const fn example_auto_offset_reset_values() -> [&'static str; 7] {
    [
        "smallest",
        "earliest",
        "beginning",
        "largest",
        "latest",
        "end",
        "error",
    ]
}

fn example_librdkafka_options() -> HashMap<String, String> {
    HashMap::<_, _>::from_iter(
        [
            ("client.id".to_string(), "${ENV_VAR}".to_string()),
            ("fetch.error.backoff.ms".to_string(), "1000".to_string()),
            ("socket.send.buffer.bytes".to_string(), "100".to_string()),
        ]
        .into_iter(),
    )
}

impl_generate_config_from_default!(KafkaSourceConfig);

#[async_trait::async_trait]
impl SourceConfig for KafkaSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let consumer = create_consumer(self)?;
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace).build();
        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        Ok(Box::pin(kafka_source(
            self.clone(),
            consumer,
            decoder,
            cx.shutdown,
            cx.out,
            acknowledgements,
            log_namespace,
        )))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<Output> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let keys = self.keys();

        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(keys.timestamp))),
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some("timestamp"),
            )
            .with_source_metadata(
                Self::NAME,
                keys.topic.clone().map(LegacyKey::Overwrite),
                &owned_value_path!("topic"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                keys.partition.clone().map(LegacyKey::Overwrite),
                &owned_value_path!("partition"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                keys.offset.clone().map(LegacyKey::Overwrite),
                &owned_value_path!("offset"),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                keys.headers.clone().map(LegacyKey::Overwrite),
                &owned_value_path!("headers"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                keys.key_field.clone().map(LegacyKey::Overwrite),
                &owned_value_path!("message_key"),
                Kind::bytes(),
                None,
            );

        vec![Output::default(self.decoding.output_type()).with_schema_definition(schema_definition)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

async fn kafka_source(
    config: KafkaSourceConfig,
    consumer: StreamConsumer<CustomContext>,
    decoder: Decoder,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
    acknowledgements: bool,
    log_namespace: LogNamespace,
) -> Result<(), ()> {
    let consumer = Arc::new(consumer);
    let (finalizer, mut ack_stream) =
        OrderedFinalizer::<FinalizerEntry>::maybe_new(acknowledgements, shutdown.clone());
    let finalizer = finalizer.map(Arc::new);
    if let Some(finalizer) = &finalizer {
        consumer
            .context()
            .finalizer
            .set(Arc::clone(finalizer))
            .expect("Finalizer is only set once");
    }

    let mut stream = consumer.stream();

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            entry = ack_stream.next() => if let Some((status, entry)) = entry {
                if status == BatchStatus::Delivered {
                    if let Err(error) =
                        consumer.store_offset(&entry.topic, entry.partition, entry.offset)
                    {
                        emit!(KafkaOffsetUpdateError { error });
                    }
                }
            },
            message = stream.next() => match message {
                None => break,  // WHY?
                Some(Err(error)) => emit!(KafkaReadError { error }),
                Some(Ok(msg)) => {
                    emit!(KafkaBytesReceived {
                        byte_size: msg.payload_len(),
                        protocol: "tcp",
                        topic: msg.topic(),
                        partition: msg.partition(),
                    });

                    parse_message(msg, decoder.clone(), config.keys(), &finalizer, &mut out, &consumer, log_namespace).await;
                }
            },
        }
    }

    Ok(())
}

async fn parse_message(
    msg: BorrowedMessage<'_>,
    decoder: Decoder,
    keys: Keys<'_>,
    finalizer: &Option<Arc<OrderedFinalizer<FinalizerEntry>>>,
    out: &mut SourceSender,
    consumer: &Arc<StreamConsumer<CustomContext>>,
    log_namespace: LogNamespace,
) {
    if let Some((count, mut stream)) = parse_stream(&msg, decoder, keys, log_namespace) {
        match finalizer {
            Some(finalizer) => {
                let (batch, receiver) = BatchNotifier::new_with_receiver();
                let mut stream = stream.map(|event| event.with_batch_notifier(&batch));
                match out.send_event_stream(&mut stream).await {
                    Err(error) => {
                        emit!(StreamClosedError { error, count });
                    }
                    Ok(_) => {
                        // Drop stream to avoid borrowing `msg`: "[...] borrow might be used
                        // here, when `stream` is dropped and runs the destructor [...]".
                        drop(stream);
                        finalizer.add(msg.into(), receiver);
                    }
                }
            }
            None => match out.send_event_stream(&mut stream).await {
                Err(error) => {
                    emit!(StreamClosedError { error, count });
                }
                Ok(_) => {
                    if let Err(error) =
                        consumer.store_offset(msg.topic(), msg.partition(), msg.offset())
                    {
                        emit!(KafkaOffsetUpdateError { error });
                    }
                }
            },
        }
    }
}

// Turn the received message into a stream of parsed events.
fn parse_stream<'a>(
    msg: &BorrowedMessage<'a>,
    decoder: Decoder,
    keys: Keys<'a>,
    log_namespace: LogNamespace,
) -> Option<(usize, impl Stream<Item = Event> + 'a)> {
    let payload = msg.payload()?; // skip messages with empty payload

    let rmsg = ReceivedMessage::from(msg);

    let payload = Cursor::new(Bytes::copy_from_slice(payload));

    let mut stream = FramedRead::new(payload, decoder);
    let (count, _) = stream.size_hint();
    let stream = stream! {
        while let Some(result) = stream.next().await {
            match result {
                Ok((events, _byte_size)) => {
                    emit!(KafkaEventsReceived {
                        count: events.len(),
                        byte_size: events.estimated_json_encoded_size_of(),
                        topic: &rmsg.topic,
                        partition: rmsg.partition,
                    });
                    for mut event in events {
                        rmsg.apply(&keys, &mut event, log_namespace);
                        yield event;
                    }
                },
                Err(error) => {
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
    Some((count, stream))
}

#[derive(Clone, Copy, Debug)]
struct Keys<'a> {
    timestamp: &'a str,
    key_field: &'a Option<OwnedValuePath>,
    topic: &'a Option<OwnedValuePath>,
    partition: &'a Option<OwnedValuePath>,
    offset: &'a Option<OwnedValuePath>,
    headers: &'a Option<OwnedValuePath>,
}

impl<'a> Keys<'a> {
    fn from(schema: &'a LogSchema, config: &'a KafkaSourceConfig) -> Self {
        Self {
            timestamp: schema.timestamp_key(),
            key_field: &config.key_field.path,
            topic: &config.topic_key.path,
            partition: &config.partition_key.path,
            offset: &config.offset_key.path,
            headers: &config.headers_key.path,
        }
    }
}

struct ReceivedMessage {
    timestamp: Option<DateTime<Utc>>,
    key: Value,
    headers: BTreeMap<String, Value>,
    topic: String,
    partition: i32,
    offset: i64,
}

impl ReceivedMessage {
    fn from(msg: &BorrowedMessage<'_>) -> Self {
        // Extract timestamp from kafka message
        let timestamp = msg
            .timestamp()
            .to_millis()
            .and_then(|millis| Utc.timestamp_millis_opt(millis).latest());

        let key = msg
            .key()
            .map(|key| Value::from(Bytes::from(key.to_owned())))
            .unwrap_or(Value::Null);

        let mut headers_map = BTreeMap::new();
        if let Some(headers) = msg.headers() {
            for header in headers.iter() {
                if let Some(value) = header.value {
                    headers_map.insert(
                        header.key.to_string(),
                        Value::from(Bytes::from(value.to_owned())),
                    );
                }
            }
        }

        Self {
            timestamp,
            key,
            headers: headers_map,
            topic: msg.topic().to_string(),
            partition: msg.partition(),
            offset: msg.offset(),
        }
    }

    fn apply(&self, keys: &Keys<'_>, event: &mut Event, log_namespace: LogNamespace) {
        if let Event::Log(ref mut log) = event {
            match log_namespace {
                LogNamespace::Vector => {
                    // We'll only use this function in Vector namespaces because we don't want
                    // "timestamp" to be set automatically in legacy namespaces. In legacy
                    // namespaces, the "timestamp" field corresponds to the Kafka message, not the
                    // timestamp when the event was processed.
                    log_namespace.insert_standard_vector_source_metadata(
                        log,
                        KafkaSourceConfig::NAME,
                        Utc::now(),
                    );
                }
                LogNamespace::Legacy => {
                    log.insert(log_schema().source_type_key(), KafkaSourceConfig::NAME);
                }
            }

            log_namespace.insert_source_metadata(
                KafkaSourceConfig::NAME,
                log,
                keys.key_field.as_ref().map(LegacyKey::Overwrite),
                path!("message_key"),
                self.key.clone(),
            );

            log_namespace.insert_source_metadata(
                KafkaSourceConfig::NAME,
                log,
                Some(LegacyKey::Overwrite(keys.timestamp)),
                path!("timestamp"),
                self.timestamp,
            );

            log_namespace.insert_source_metadata(
                KafkaSourceConfig::NAME,
                log,
                keys.topic.as_ref().map(LegacyKey::Overwrite),
                path!("topic"),
                self.topic.clone(),
            );

            log_namespace.insert_source_metadata(
                KafkaSourceConfig::NAME,
                log,
                keys.partition.as_ref().map(LegacyKey::Overwrite),
                path!("partition"),
                self.partition,
            );

            log_namespace.insert_source_metadata(
                KafkaSourceConfig::NAME,
                log,
                keys.offset.as_ref().map(LegacyKey::Overwrite),
                path!("offset"),
                self.offset,
            );

            log_namespace.insert_source_metadata(
                KafkaSourceConfig::NAME,
                log,
                keys.headers.as_ref().map(LegacyKey::Overwrite),
                path!("headers"),
                self.headers.clone(),
            );
        }
    }
}

#[derive(Debug)]
struct FinalizerEntry {
    topic: String,
    partition: i32,
    offset: i64,
}

impl<'a> From<BorrowedMessage<'a>> for FinalizerEntry {
    fn from(msg: BorrowedMessage<'a>) -> Self {
        Self {
            topic: msg.topic().into(),
            partition: msg.partition(),
            offset: msg.offset(),
        }
    }
}

fn create_consumer(config: &KafkaSourceConfig) -> crate::Result<StreamConsumer<CustomContext>> {
    let mut client_config = ClientConfig::new();
    client_config
        .set("group.id", &config.group_id)
        .set("bootstrap.servers", &config.bootstrap_servers)
        .set("auto.offset.reset", &config.auto_offset_reset)
        .set(
            "session.timeout.ms",
            &config.session_timeout_ms.as_millis().to_string(),
        )
        .set(
            "socket.timeout.ms",
            &config.socket_timeout_ms.as_millis().to_string(),
        )
        .set(
            "fetch.wait.max.ms",
            &config.fetch_wait_max_ms.as_millis().to_string(),
        )
        .set("enable.partition.eof", "false")
        .set("enable.auto.commit", "true")
        .set(
            "auto.commit.interval.ms",
            &config.commit_interval_ms.as_millis().to_string(),
        )
        .set("enable.auto.offset.store", "false")
        .set("statistics.interval.ms", "1000")
        .set("client.id", "vector");

    config.auth.apply(&mut client_config)?;

    if let Some(librdkafka_options) = &config.librdkafka_options {
        for (key, value) in librdkafka_options {
            client_config.set(key.as_str(), value.as_str());
        }
    }

    if let Some(group_instance_id) = &config.group_instance_id {
        client_config.set("group.instance.id", group_instance_id);
    }

    let consumer = client_config
        .create_with_context::<_, StreamConsumer<_>>(CustomContext::default())
        .context(KafkaCreateSnafu)?;
    let topics: Vec<&str> = config.topics.iter().map(|s| s.as_str()).collect();
    consumer.subscribe(&topics).context(KafkaSubscribeSnafu)?;

    Ok(consumer)
}

#[derive(Default)]
struct CustomContext {
    stats: kafka::KafkaStatisticsContext,
    finalizer: OnceCell<Arc<OrderedFinalizer<FinalizerEntry>>>,
}

impl ClientContext for CustomContext {
    fn stats(&self, statistics: Statistics) {
        self.stats.stats(statistics)
    }
}

impl ConsumerContext for CustomContext {
    fn post_rebalance(&self, rebalance: &Rebalance) {
        if matches!(rebalance, Rebalance::Revoke(_)) {
            if let Some(finalizer) = self.finalizer.get() {
                finalizer.flush();
            }
        }
    }
}

#[cfg(test)]
mod test {
    use lookup::LookupBuf;
    use vector_core::schema::Definition;

    use super::*;

    pub fn kafka_host() -> String {
        std::env::var("KAFKA_HOST").unwrap_or_else(|_| "localhost".into())
    }

    pub fn kafka_address(port: u16) -> String {
        format!("{}:{}", kafka_host(), port)
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<KafkaSourceConfig>();
    }

    pub(super) fn make_config(
        topic: &str,
        group: &str,
        log_namespace: LogNamespace,
    ) -> KafkaSourceConfig {
        KafkaSourceConfig {
            bootstrap_servers: kafka_address(9091),
            topics: vec![topic.into()],
            group_id: group.into(),
            auto_offset_reset: "beginning".into(),
            session_timeout_ms: Duration::from_millis(6000),
            commit_interval_ms: Duration::from_millis(1),
            key_field: default_key_field(),
            topic_key: default_topic_key(),
            partition_key: default_partition_key(),
            offset_key: default_offset_key(),
            headers_key: default_headers_key(),
            socket_timeout_ms: Duration::from_millis(60000),
            fetch_wait_max_ms: Duration::from_millis(100),
            log_namespace: Some(log_namespace == LogNamespace::Vector),
            ..Default::default()
        }
    }

    #[test]
    fn test_output_schema_definition_vector_namespace() {
        let definition = make_config("topic", "group", LogNamespace::Vector)
            .outputs(LogNamespace::Vector)[0]
            .clone()
            .log_schema_definition
            .unwrap();

        assert_eq!(
            definition,
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                .with_meaning(LookupBuf::root(), "message")
                .with_metadata_field(&owned_value_path!("kafka", "timestamp"), Kind::timestamp())
                .with_metadata_field(&owned_value_path!("kafka", "message_key"), Kind::bytes())
                .with_metadata_field(&owned_value_path!("kafka", "topic"), Kind::bytes())
                .with_metadata_field(&owned_value_path!("kafka", "partition"), Kind::bytes())
                .with_metadata_field(&owned_value_path!("kafka", "offset"), Kind::bytes())
                .with_metadata_field(
                    &owned_value_path!("kafka", "headers"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes()))
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp()
                )
                .with_metadata_field(&owned_value_path!("vector", "source_type"), Kind::bytes())
        )
    }

    #[test]
    fn test_output_schema_definition_legacy_namespace() {
        let definition = make_config("topic", "group", LogNamespace::Legacy)
            .outputs(LogNamespace::Legacy)[0]
            .clone()
            .log_schema_definition
            .unwrap();

        assert_eq!(
            definition,
            Definition::new_with_default_metadata(Kind::json(), [LogNamespace::Legacy])
                .unknown_fields(Kind::undefined())
                .with_event_field(
                    &owned_value_path!("message"),
                    Kind::bytes(),
                    Some("message")
                )
                .with_event_field(
                    &owned_value_path!("timestamp"),
                    Kind::timestamp(),
                    Some("timestamp")
                )
                .with_event_field(&owned_value_path!("message_key"), Kind::bytes(), None)
                .with_event_field(&owned_value_path!("topic"), Kind::bytes(), None)
                .with_event_field(&owned_value_path!("partition"), Kind::bytes(), None)
                .with_event_field(&owned_value_path!("offset"), Kind::bytes(), None)
                .with_event_field(
                    &owned_value_path!("headers"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())),
                    None
                )
                .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
        )
    }

    #[tokio::test]
    async fn consumer_create_ok() {
        let config = make_config("topic", "group", LogNamespace::Legacy);
        assert!(create_consumer(&config).is_ok());
    }

    #[tokio::test]
    async fn consumer_create_incorrect_auto_offset_reset() {
        let config = KafkaSourceConfig {
            auto_offset_reset: "incorrect-auto-offset-reset".to_string(),
            ..make_config("topic", "group", LogNamespace::Legacy)
        };
        assert!(create_consumer(&config).is_err());
    }
}

#[cfg(feature = "kafka-integration-tests")]
#[cfg(test)]
mod integration_test {
    use std::time::Duration;

    use chrono::{DateTime, SubsecRound, Utc};
    use futures::Stream;
    use rdkafka::{
        admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
        client::DefaultClientContext,
        config::{ClientConfig, FromClientConfig},
        consumer::BaseConsumer,
        message::{Header, OwnedHeaders},
        producer::{FutureProducer, FutureRecord},
        util::Timeout,
        Offset, TopicPartitionList,
    };
    use stream_cancel::{Trigger, Tripwire};
    use tokio::time::sleep;
    use vector_buffers::topology::channel::BufferReceiver;
    use vector_core::event::EventStatus;

    use super::{test::*, *};
    use crate::{
        event::{EventArray, EventContainer},
        shutdown::ShutdownSignal,
        test_util::{collect_n, components::assert_source_compliance, random_string},
        SourceSender,
    };

    const KEY: &str = "my key";
    const TEXT: &str = "my message";
    const HEADER_KEY: &str = "my header";
    const HEADER_VALUE: &str = "my header value";

    fn client_config<T: FromClientConfig>(group: Option<&str>) -> T {
        let mut client = ClientConfig::new();
        client.set("bootstrap.servers", kafka_address(9091));
        client.set("produce.offset.report", "true");
        client.set("message.timeout.ms", "5000");
        client.set("auto.commit.interval.ms", "1");
        if let Some(group) = group {
            client.set("group.id", group);
        }
        client.create().expect("Producer creation error")
    }

    async fn send_events(topic: String, count: usize) -> DateTime<Utc> {
        let now = Utc::now();
        let timestamp = now.timestamp_millis();

        let producer: FutureProducer = client_config(None);

        for i in 0..count {
            let text = format!("{} {:03}", TEXT, i);
            let key = format!("{} {}", KEY, i);
            let record = FutureRecord::to(&topic)
                .payload(&text)
                .key(&key)
                .timestamp(timestamp)
                .headers(OwnedHeaders::new().insert(Header {
                    key: HEADER_KEY,
                    value: Some(HEADER_VALUE),
                }));

            if let Err(error) = producer.send(record, Timeout::Never).await {
                panic!("Cannot send event to Kafka: {:?}", error);
            }
        }

        now
    }

    #[tokio::test]
    async fn consumes_event_with_acknowledgements() {
        send_receive(true, |_| false, 10, LogNamespace::Legacy).await;
    }

    #[tokio::test]
    async fn consumes_event_with_acknowledgements_vector_namespace() {
        send_receive(true, |_| false, 10, LogNamespace::Vector).await;
    }

    #[tokio::test]
    async fn consumes_event_without_acknowledgements() {
        send_receive(false, |_| false, 10, LogNamespace::Legacy).await;
    }

    #[tokio::test]
    async fn consumes_event_without_acknowledgements_vector_namespace() {
        send_receive(false, |_| false, 10, LogNamespace::Vector).await;
    }

    #[tokio::test]
    async fn handles_one_negative_acknowledgement() {
        send_receive(true, |n| n == 2, 10, LogNamespace::Legacy).await;
    }

    #[tokio::test]
    async fn handles_one_negative_acknowledgement_vector_namespace() {
        send_receive(true, |n| n == 2, 10, LogNamespace::Vector).await;
    }

    #[tokio::test]
    async fn handles_permanent_negative_acknowledgement() {
        send_receive(true, |n| n >= 2, 2, LogNamespace::Legacy).await;
    }

    #[tokio::test]
    async fn handles_permanent_negative_acknowledgement_vector_namespace() {
        send_receive(true, |n| n >= 2, 2, LogNamespace::Vector).await;
    }

    async fn send_receive(
        acknowledgements: bool,
        error_at: impl Fn(usize) -> bool,
        receive_count: usize,
        log_namespace: LogNamespace,
    ) {
        const SEND_COUNT: usize = 10;

        let topic = format!("test-topic-{}", random_string(10));
        let group_id = format!("test-group-{}", random_string(10));
        let config = make_config(&topic, &group_id, log_namespace);

        let now = send_events(topic.clone(), 10).await;

        let events = assert_source_compliance(&["protocol", "topic", "partition"], async move {
            let (tx, rx) = SourceSender::new_test_errors(error_at);
            let (trigger_shutdown, shutdown_done) =
                spawn_kafka(tx, config, acknowledgements, log_namespace);
            let events = collect_n(rx, SEND_COUNT).await;
            // Yield to the finalization task to let it collect the
            // batch status receivers before signalling the shutdown.
            tokio::task::yield_now().await;
            drop(trigger_shutdown);
            shutdown_done.await;

            events
        })
        .await;

        let offset = fetch_tpl_offset(&group_id, &topic, 0);
        assert_eq!(offset, Offset::from_raw(receive_count as i64));

        assert_eq!(events.len(), SEND_COUNT);
        for (i, event) in events.into_iter().enumerate() {
            if let LogNamespace::Legacy = log_namespace {
                assert_eq!(
                    event.as_log()[log_schema().message_key()],
                    format!("{} {:03}", TEXT, i).into()
                );
                assert_eq!(
                    event.as_log()["message_key"],
                    format!("{} {}", KEY, i).into()
                );
                assert_eq!(
                    event.as_log()[log_schema().source_type_key()],
                    "kafka".into()
                );
                assert_eq!(
                    event.as_log()[log_schema().timestamp_key()],
                    now.trunc_subsecs(3).into()
                );
                assert_eq!(event.as_log()["topic"], topic.clone().into());
                assert!(event.as_log().contains("partition"));
                assert!(event.as_log().contains("offset"));
                let mut expected_headers = BTreeMap::new();
                expected_headers.insert(HEADER_KEY.to_string(), Value::from(HEADER_VALUE));
                assert_eq!(event.as_log()["headers"], Value::from(expected_headers));
            } else {
                let meta = event.as_log().metadata().value();

                assert_eq!(
                    meta.get(path!("vector", "source_type")).unwrap(),
                    &vrl::value!(KafkaSourceConfig::NAME)
                );
                assert!(meta
                    .get(path!("vector", "ingest_timestamp"))
                    .unwrap()
                    .is_timestamp());

                assert_eq!(
                    event.as_log().value(),
                    &vrl::value!(format!("{} {:03}", TEXT, i))
                );
                assert_eq!(
                    meta.get(path!("kafka", "message_key")).unwrap(),
                    &vrl::value!(format!("{} {}", KEY, i))
                );

                assert_eq!(
                    meta.get(path!("kafka", "timestamp")).unwrap(),
                    &vrl::value!(now.trunc_subsecs(3))
                );
                assert_eq!(
                    meta.get(path!("kafka", "topic")).unwrap(),
                    &vrl::value!(topic.clone())
                );
                assert!(meta.get(path!("kafka", "partition")).unwrap().is_integer(),);
                assert!(meta.get(path!("kafka", "offset")).unwrap().is_integer(),);

                let mut expected_headers = BTreeMap::new();
                expected_headers.insert(HEADER_KEY.to_string(), Value::from(HEADER_VALUE));
                assert_eq!(
                    meta.get(path!("kafka", "headers")).unwrap(),
                    &Value::from(expected_headers)
                );
            }
        }
    }

    fn make_rand_config() -> (String, String, KafkaSourceConfig) {
        let topic = format!("test-topic-{}", random_string(10));
        let group_id = format!("test-group-{}", random_string(10));
        let config = make_config(&topic, &group_id, LogNamespace::Legacy);
        (topic, group_id, config)
    }

    fn delay_pipeline(
        id: usize,
        delay: Duration,
        status: EventStatus,
    ) -> (SourceSender, impl Stream<Item = EventArray> + Unpin) {
        let (pipe, recv) = SourceSender::new_with_buffer(100);
        let recv = BufferReceiver::new(recv.into()).into_stream();
        let recv = recv.then(move |mut events| async move {
            events.iter_logs_mut().for_each(|log| {
                log.insert("pipeline_id", id.to_string());
            });
            sleep(delay).await;
            events.iter_events_mut().for_each(|mut event| {
                let metadata = event.metadata_mut();
                metadata.update_status(status);
                metadata.update_sources();
            });
            events
        });
        (pipe, Box::pin(recv))
    }

    fn spawn_kafka(
        tx: SourceSender,
        config: KafkaSourceConfig,
        acknowledgements: bool,
        log_namespace: LogNamespace,
    ) -> (Trigger, Tripwire) {
        let (trigger_shutdown, shutdown, shutdown_done) = ShutdownSignal::new_wired();
        let consumer = create_consumer(&config).unwrap();

        let decoder = DecodingConfig::new(
            config.framing.clone(),
            config.decoding.clone(),
            log_namespace,
        )
        .build();

        tokio::spawn(kafka_source(
            config,
            consumer,
            decoder,
            shutdown,
            tx,
            acknowledgements,
            log_namespace,
        ));
        (trigger_shutdown, shutdown_done)
    }

    fn fetch_tpl_offset(group_id: &str, topic: &str, partition: i32) -> Offset {
        let client: BaseConsumer = client_config(Some(group_id));
        client.subscribe(&[topic]).expect("Subscribing failed");

        let mut tpl = TopicPartitionList::new();
        tpl.add_partition(topic, partition);
        client
            .committed_offsets(tpl, Duration::from_secs(1))
            .expect("Getting committed offsets failed")
            .find_partition(topic, partition)
            .expect("Missing topic/partition")
            .offset()
    }

    async fn create_topic(group_id: &str, topic: &str, partitions: i32) {
        let client: AdminClient<DefaultClientContext> = client_config(Some(group_id));
        for result in client
            .create_topics(
                [&NewTopic {
                    name: topic,
                    num_partitions: partitions,
                    replication: TopicReplication::Fixed(1),
                    config: vec![],
                }],
                &AdminOptions::default(),
            )
            .await
            .expect("create_topics failed")
        {
            result.expect("Creating a topic failed");
        }
    }

    // Failure timeline:
    // - Topic exists on multiple partitions
    // - Consumer A connects to topic, is assigned both partitions
    // - Consumer A receives some messages
    // - Consumer B connects to topic
    // - Consumer A has one partition revoked (rebalance)
    // - Consumer B is assigned a partition
    // - Consumer A stores an order on the revoked partition
    // - Consumer B skips receiving messages?
    #[ignore]
    #[tokio::test]
    async fn handles_rebalance() {
        // The test plan here is to:
        // - Set up one source instance, feeding into a pipeline that delays acks.
        // - Wait a bit, and set up a second source instance. This should cause a rebalance.
        // - Wait further until all events will have been pulled down.
        // - Verify that all events are captured by the two sources, and that offsets are set right, etc.

        // However this test, as written, does not actually cause the
        // conditions required to test this. We have had external
        // validation that the sink behaves properly on rebalance
        // events.  This test also requires the insertion of a small
        // delay into the source to guarantee the timing, which is not
        // suitable for production code.

        const NEVENTS: usize = 200;
        const DELAY: u64 = 100;

        let (topic, group_id, config) = make_rand_config();
        create_topic(&group_id, &topic, 2).await;

        let _send_start = send_events(topic.clone(), NEVENTS).await;

        let (tx, rx1) = delay_pipeline(1, Duration::from_millis(200), EventStatus::Delivered);
        let (trigger_shutdown1, shutdown_done1) =
            spawn_kafka(tx, config.clone(), true, LogNamespace::Legacy);
        let events1 = tokio::spawn(collect_n(rx1, NEVENTS));

        sleep(Duration::from_secs(1)).await;

        let (tx, rx2) = delay_pipeline(2, Duration::from_millis(DELAY), EventStatus::Delivered);
        let (trigger_shutdown2, shutdown_done2) =
            spawn_kafka(tx, config, true, LogNamespace::Legacy);
        let events2 = tokio::spawn(collect_n(rx2, NEVENTS));

        sleep(Duration::from_secs(5)).await;

        drop(trigger_shutdown1);
        let events1 = events1.await.unwrap();
        shutdown_done1.await;

        sleep(Duration::from_secs(5)).await;

        drop(trigger_shutdown2);
        let events2 = events2.await.unwrap();
        shutdown_done2.await;

        sleep(Duration::from_secs(1)).await;

        assert!(!events1.is_empty());
        assert!(!events2.is_empty());

        match fetch_tpl_offset(&group_id, &topic, 0) {
            Offset::Offset(offset) => {
                assert!((offset as isize - events1.len() as isize).abs() <= 1)
            }
            o => panic!("Invalid offset for partition 0 {:?}", o),
        }

        match fetch_tpl_offset(&group_id, &topic, 1) {
            Offset::Offset(offset) => {
                assert!((offset as isize - events2.len() as isize).abs() <= 1)
            }
            o => panic!("Invalid offset for partition 0 {:?}", o),
        }

        let mut all_events = events1
            .into_iter()
            .chain(events2.into_iter())
            .flat_map(map_logs)
            .collect::<Vec<String>>();
        all_events.sort();

        // Assert they are all in sequential order and no dupes, TODO
    }

    fn map_logs(events: EventArray) -> impl Iterator<Item = String> {
        events.into_events().map(|event| {
            let log = event.into_log();
            format!(
                "{} {} {} {}",
                log["message"].to_string_lossy(),
                log["topic"].to_string_lossy(),
                log["partition"].to_string_lossy(),
                log["offset"].to_string_lossy(),
            )
        })
    }
}
