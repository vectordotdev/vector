use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    pin::Pin,
    sync::{
        Arc, OnceLock, Weak,
        mpsc::{SyncSender, sync_channel},
    },
    time::Duration,
};

use async_stream::stream;
use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use futures::{Stream, StreamExt};
use futures_util::future::OptionFuture;
use rdkafka::{
    ClientConfig, ClientContext, Statistics, TopicPartitionList,
    consumer::{
        BaseConsumer, CommitMode, Consumer, ConsumerContext, Rebalance, StreamConsumer,
        stream_consumer::StreamPartitionQueue,
    },
    error::KafkaError,
    message::{BorrowedMessage, Headers as _, Message},
    types::RDKafkaErrorCode,
};
use serde_with::serde_as;
use snafu::{ResultExt, Snafu};
use tokio::{
    runtime::Handle,
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    task::JoinSet,
    time::Sleep,
};
use tracing::{Instrument, Span};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{
        DecoderFramedRead, StreamDecodingError,
        decoding::{DecompressionConfig, Decompressor, DeserializerConfig, FramingConfig},
    },
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    finalizer::OrderedFinalizer,
    lookup::{OwnedValuePath, lookup_v2::OptionalValuePath, owned_value_path, path},
};
use vrl::value::{Kind, ObjectMap, kind::Collection};

use crate::{
    SourceSender,
    codecs::{Decoder, DecodingConfig},
    config::{
        LogSchema, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
        log_schema,
    },
    event::{BatchNotifier, BatchStatus, Event, Value},
    internal_events::{
        KafkaBytesReceived, KafkaEventsReceived, KafkaOffsetUpdateError,
        KafkaPayloadDecompressionError, KafkaReadError, StreamClosedError,
    },
    kafka,
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("The drain_timeout_ms ({}) must be less than session_timeout_ms ({})", value, session_timeout_ms.as_millis()))]
    InvalidDrainTimeout {
        value: u64,
        session_timeout_ms: Duration,
    },
    #[snafu(display("Could not create Kafka consumer: {}", source))]
    CreateError { source: rdkafka::error::KafkaError },
    #[snafu(display("Could not subscribe to Kafka topics: {}", source))]
    SubscribeError { source: rdkafka::error::KafkaError },
}

/// Metrics (beta) configuration.
#[configurable_component]
#[derive(Clone, Debug, Default)]
struct Metrics {
    /// Expose topic lag metrics for all topics and partitions. Metric names are `kafka_consumer_lag`.
    pub topic_lag_metric: bool,
}

/// Configuration for the `kafka` source.
#[serde_as]
#[configurable_component(source("kafka", "Collect logs from Apache Kafka."))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct KafkaSourceConfig {
    /// A comma-separated list of Kafka bootstrap servers.
    ///
    /// These are the servers in a Kafka cluster that a client should use to bootstrap its connection to the cluster,
    /// allowing discovery of all the other hosts in the cluster.
    ///
    /// Must be in the form of `host:port`, and comma-separated.
    #[configurable(metadata(docs::examples = "10.14.22.123:9092,10.14.23.332:9092"))]
    bootstrap_servers: String,

    /// The Kafka topics names to read events from.
    ///
    /// Regular expression syntax is supported if the topic begins with `^`.
    #[configurable(metadata(
        docs::examples = "^(prefix1|prefix2)-.+",
        docs::examples = "topic-1",
        docs::examples = "topic-2"
    ))]
    topics: Vec<String>,

    /// The consumer group name to be used to consume events from Kafka.
    #[configurable(metadata(docs::examples = "consumer-group-name"))]
    group_id: String,

    /// If offsets for consumer group do not exist, set them using this strategy.
    ///
    /// See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for the `auto.offset.reset` option for further clarification.
    #[serde(default = "default_auto_offset_reset")]
    #[configurable(metadata(docs::examples = "example_auto_offset_reset_values()"))]
    auto_offset_reset: String,

    /// The Kafka session timeout.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::examples = 5000, docs::examples = 10000))]
    #[serde(default = "default_session_timeout_ms")]
    #[configurable(metadata(docs::human_name = "Session Timeout"))]
    session_timeout_ms: Duration,

    /// Timeout to drain pending acknowledgements during shutdown or a Kafka
    /// consumer group rebalance.
    ///
    /// When Vector shuts down or the Kafka consumer group revokes partitions from this
    /// consumer, wait a maximum of `drain_timeout_ms` for the source to
    /// process pending acknowledgements. Must be less than `session_timeout_ms`
    /// to ensure the consumer is not excluded from the group during a rebalance.
    ///
    /// Default value is half of `session_timeout_ms`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[configurable(metadata(docs::examples = 2500, docs::examples = 5000))]
    #[configurable(metadata(docs::human_name = "Drain Timeout"))]
    drain_timeout_ms: Option<u64>,

    /// Timeout for network requests.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::examples = 30000, docs::examples = 60000))]
    #[serde(default = "default_socket_timeout_ms")]
    #[configurable(metadata(docs::human_name = "Socket Timeout"))]
    socket_timeout_ms: Duration,

    /// Maximum time the broker may wait to fill the response.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::examples = 50, docs::examples = 100))]
    #[serde(default = "default_fetch_wait_max_ms")]
    #[configurable(metadata(docs::human_name = "Max Fetch Wait Time"))]
    fetch_wait_max_ms: Duration,

    /// The frequency that the consumer offsets are committed (written) to offset storage.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[serde(default = "default_commit_interval_ms")]
    #[configurable(metadata(docs::examples = 5000, docs::examples = 10000))]
    #[configurable(metadata(docs::human_name = "Commit Interval"))]
    commit_interval_ms: Duration,

    /// Overrides the name of the log field used to add the message key to each event.
    ///
    /// The value is the message key of the Kafka message itself.
    ///
    /// By default, `"message_key"` is used.
    #[serde(default = "default_key_field")]
    #[configurable(metadata(docs::examples = "message_key"))]
    key_field: OptionalValuePath,

    /// Overrides the name of the log field used to add the topic to each event.
    ///
    /// The value is the topic from which the Kafka message was consumed from.
    ///
    /// By default, `"topic"` is used.
    #[serde(default = "default_topic_key")]
    #[configurable(metadata(docs::examples = "topic"))]
    topic_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the partition to each event.
    ///
    /// The value is the partition from which the Kafka message was consumed from.
    ///
    /// By default, `"partition"` is used.
    #[serde(default = "default_partition_key")]
    #[configurable(metadata(docs::examples = "partition"))]
    partition_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the offset to each event.
    ///
    /// The value is the offset of the Kafka message itself.
    ///
    /// By default, `"offset"` is used.
    #[serde(default = "default_offset_key")]
    #[configurable(metadata(docs::examples = "offset"))]
    offset_key: OptionalValuePath,

    /// Overrides the name of the log field used to add the headers to each event.
    ///
    /// The value is the headers of the Kafka message itself.
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

    /// Configuration for decompressing message payloads that were compressed by the producer.
    ///
    /// This applies to application-level compression, where the producer compressed each message
    /// payload before sending it. Compression negotiated at the Kafka protocol level is handled
    /// transparently by the underlying client library and does not require this option.
    ///
    /// Payloads are decompressed before `framing` and `decoding` are applied.
    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    decompression: Option<DecompressionConfig>,

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

    #[configurable(derived)]
    #[serde(default)]
    metrics: Metrics,
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
    HashMap::<_, _>::from_iter([
        ("client.id".to_string(), "${ENV_VAR}".to_string()),
        ("fetch.error.backoff.ms".to_string(), "1000".to_string()),
        ("socket.send.buffer.bytes".to_string(), "100".to_string()),
    ])
}

impl_generate_config_from_default!(KafkaSourceConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "kafka")]
impl SourceConfig for KafkaSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;
        let decompressor = self
            .decompression
            .as_ref()
            .map(DecompressionConfig::build)
            .transpose()?;
        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        if let Some(d) = self.drain_timeout_ms {
            snafu::ensure!(
                Duration::from_millis(d) <= self.session_timeout_ms,
                InvalidDrainTimeoutSnafu {
                    value: d,
                    session_timeout_ms: self.session_timeout_ms
                }
            );
        }

        let (consumer, callback_rx) = create_consumer(self, acknowledgements)?;

        Ok(Box::pin(kafka_source(
            self.clone(),
            consumer,
            callback_rx,
            decoder,
            decompressor,
            cx.out,
            cx.shutdown,
            false,
            log_namespace,
        )))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let keys = self.keys();

        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                Self::NAME,
                keys.timestamp.map(LegacyKey::Overwrite),
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

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[allow(clippy::too_many_arguments)]
async fn kafka_source(
    config: KafkaSourceConfig,
    consumer: StreamConsumer<KafkaSourceContext>,
    callback_rx: UnboundedReceiver<KafkaCallback>,
    decoder: Decoder,
    decompressor: Option<Decompressor>,
    out: SourceSender,
    shutdown: ShutdownSignal,
    eof: bool,
    log_namespace: LogNamespace,
) -> Result<(), ()> {
    let span = info_span!("kafka_source");
    let consumer = Arc::new(consumer);

    consumer
        .context()
        .consumer
        .set(Arc::downgrade(&consumer))
        .expect("Error setting up consumer context.");

    // EOF signal allowing the coordination task to tell the kafka client task when all partitions have reached EOF
    let (eof_tx, eof_rx) = eof.then(oneshot::channel::<()>).unzip();

    let topics: Vec<&str> = config.topics.iter().map(|s| s.as_str()).collect();
    if let Err(e) = consumer.subscribe(&topics).context(SubscribeSnafu) {
        error!("{}", e);
        return Err(());
    }

    let coordination_task = {
        let span = span.clone();
        let consumer = Arc::clone(&consumer);
        let drain_timeout_ms = config
            .drain_timeout_ms
            .map_or(config.session_timeout_ms / 2, Duration::from_millis);
        let consumer_state = ConsumerStateInner::<Consuming>::new(
            config,
            decoder,
            decompressor,
            out,
            log_namespace,
            span,
        );
        crate::spawn_in_current_span(async move {
            coordinate_kafka_callbacks(
                consumer,
                callback_rx,
                consumer_state,
                drain_timeout_ms,
                eof_tx,
            )
            .await;
        })
    };

    let client_task = {
        let consumer = Arc::clone(&consumer);
        tokio::task::spawn_blocking(move || {
            let _enter = span.enter();
            drive_kafka_consumer(consumer, shutdown, eof_rx);
        })
    };

    _ = tokio::join!(client_task, coordination_task);
    consumer.context().commit_consumer_state();

    Ok(())
}

/// ConsumerStateInner implements a small struct/enum-based state machine.
///
/// With a ConsumerStateInner<Consuming>, the client is able to spawn new tasks
/// when partitions are assigned. When a shutdown signal is received, or
/// partitions are being revoked, the Consuming state is traded for a Draining
/// state (and associated drain deadline future) via the `begin_drain` method
///
/// A ConsumerStateInner<Draining> keeps track of partitions that are expected
/// to complete, and also owns the signal that, when dropped, indicates to the
/// client driver task that it is safe to proceed with the rebalance or shutdown.
/// When draining is complete, or the deadline is reached, Draining is traded in for
/// either a Consuming (after a revoke) or Complete (in the case of shutdown) state,
/// via the `finish_drain` method.
///
/// A ConsumerStateInner<Complete> is the final state, reached after a shutdown
/// signal is received. This can not be traded for another state, and the
/// coordination task should exit when this state is reached.
struct ConsumerStateInner<S> {
    config: KafkaSourceConfig,
    decoder: Decoder,
    decompressor: Option<Decompressor>,
    out: SourceSender,
    log_namespace: LogNamespace,
    consumer_state: S,
}
struct Consuming {
    /// The source's tracing Span used to instrument metrics emitted by consumer tasks
    span: Span,
}
struct Draining {
    /// The rendezvous channel sender from the revoke or shutdown callback. Sending on this channel
    /// indicates to the kafka client task that one or more partitions have been drained, while
    /// closing this channel indicates that all expected partitions have drained, or the drain
    /// timeout has been reached.
    signal: SyncSender<()>,

    /// The set of topic-partition tasks that are required to complete during
    /// the draining phase, populated at the beginning of a rebalance or shutdown.
    /// Partitions that are being revoked, but not being actively consumed
    /// (e.g. due to the consumer task exiting early) should not be included.
    /// The draining phase is considered complete when this set is empty.
    expect_drain: HashSet<TopicPartition>,

    /// Whether the client is shutting down after draining. If set to true,
    /// the `finish_drain` method will return a Complete state, otherwise
    /// a Consuming state.
    shutdown: bool,

    /// The source's tracing Span used to instrument metrics emitted by consumer tasks
    span: Span,
}
type OptionDeadline = OptionFuture<Pin<Box<Sleep>>>;
enum ConsumerState {
    Consuming(ConsumerStateInner<Consuming>),
    Draining(ConsumerStateInner<Draining>),
    Complete,
}
impl Draining {
    fn new(signal: SyncSender<()>, shutdown: bool, span: Span) -> Self {
        Self {
            signal,
            shutdown,
            expect_drain: HashSet::new(),
            span,
        }
    }

    fn is_complete(&self) -> bool {
        self.expect_drain.is_empty()
    }
}

impl<C> ConsumerStateInner<C> {
    fn complete(self, _deadline: OptionDeadline) -> (OptionDeadline, ConsumerState) {
        (None.into(), ConsumerState::Complete)
    }
}

impl ConsumerStateInner<Consuming> {
    const fn new(
        config: KafkaSourceConfig,
        decoder: Decoder,
        decompressor: Option<Decompressor>,
        out: SourceSender,
        log_namespace: LogNamespace,
        span: Span,
    ) -> Self {
        Self {
            config,
            decoder,
            decompressor,
            out,
            log_namespace,
            consumer_state: Consuming { span },
        }
    }

    /// Spawn a task on the provided JoinSet to consume the kafka StreamPartitionQueue, and handle
    /// acknowledgements for the messages consumed Returns a channel sender that can be used to
    /// signal that the consumer should stop and drain pending acknowledgements, and an AbortHandle
    /// that can be used to forcefully end the task.
    fn consume_partition(
        &self,
        join_set: &mut JoinSet<(TopicPartition, PartitionConsumerStatus)>,
        tp: TopicPartition,
        consumer: Arc<StreamConsumer<KafkaSourceContext>>,
        p: StreamPartitionQueue<KafkaSourceContext>,
        acknowledgements: bool,
        exit_eof: bool,
    ) -> (oneshot::Sender<()>, tokio::task::AbortHandle) {
        let keys = self.config.keys();
        let decoder = self.decoder.clone();
        let decompressor = self.decompressor.clone();
        let log_namespace = self.log_namespace;
        let mut out = self.out.clone();

        let (end_tx, mut end_signal) = oneshot::channel::<()>();

        let handle = join_set.spawn(async move {
            let mut messages = p.stream();
            let (finalizer, mut ack_stream) = OrderedFinalizer::<FinalizerEntry>::new(None);

            // finalizer is the entry point for new pending acknowledgements;
            // when it is dropped, no new messages will be consumed, and the
            // task will end when it reaches the end of ack_stream
            let mut finalizer = Some(finalizer);

            let mut status = PartitionConsumerStatus::NormalExit;

            loop {
                tokio::select!(
                    // Make sure to handle the acknowledgement stream before new messages to prevent
                    // unbounded memory growth caused by those acks being handled slower than
                    // incoming messages when the load is high.
                    biased;

                    // is_some() checks prevent polling end_signal after it completes
                    _ = &mut end_signal, if finalizer.is_some() => {
                        finalizer.take();
                    },

                    ack = ack_stream.next() => match ack {
                        Some((status, entry)) => {
                            if status == BatchStatus::Delivered
                                && let Err(error) =  consumer.store_offset(&entry.topic, entry.partition, entry.offset) {
                                    emit!(KafkaOffsetUpdateError { error });
                                }
                        }
                        None if finalizer.is_none() => {
                            debug!("Acknowledgement stream complete for partition {}:{}.", &tp.0, tp.1);
                            break
                        }
                        None => {
                            debug!("Acknowledgement stream empty for {}:{}", &tp.0, tp.1);
                        }
                    },

                    message = messages.next(), if finalizer.is_some() => match message {
                        None => unreachable!("MessageStream never calls Ready(None)"),
                        Some(Err(error)) => match error {
                            rdkafka::error::KafkaError::PartitionEOF(partition) if exit_eof => {
                                debug!("EOF for partition {}.", partition);
                                status = PartitionConsumerStatus::PartitionEOF;
                                finalizer.take();
                            },
                            _ => emit!(KafkaReadError { error }),
                        },
                        Some(Ok(msg)) => {
                            emit!(KafkaBytesReceived {
                                byte_size: msg.payload_len(),
                                protocol: "tcp",
                                topic: msg.topic(),
                                partition: msg.partition(),
                            });
                            parse_message(msg, decoder.clone(), decompressor.as_ref(), &keys, &mut out, acknowledgements, &finalizer, log_namespace).await;
                        }
                    },
                )
            }
            (tp, status)
        }.instrument(self.consumer_state.span.clone()));
        (end_tx, handle)
    }

    /// Consume self, and return a "Draining" ConsumerState, along with a Future
    /// representing a drain deadline, based on max_drain_ms
    fn begin_drain(
        self,
        max_drain_ms: Duration,
        sig: SyncSender<()>,
        shutdown: bool,
    ) -> (OptionDeadline, ConsumerStateInner<Draining>) {
        let deadline = Box::pin(tokio::time::sleep(max_drain_ms));

        let draining = ConsumerStateInner {
            config: self.config,
            decoder: self.decoder,
            decompressor: self.decompressor,
            out: self.out,
            log_namespace: self.log_namespace,
            consumer_state: Draining::new(sig, shutdown, self.consumer_state.span),
        };

        (Some(deadline).into(), draining)
    }

    pub const fn keep_consuming(self, deadline: OptionDeadline) -> (OptionDeadline, ConsumerState) {
        (deadline, ConsumerState::Consuming(self))
    }
}

impl ConsumerStateInner<Draining> {
    /// Mark the given TopicPartition as being revoked, adding it to the set of
    /// partitions expected to drain
    fn revoke_partition(&mut self, tp: TopicPartition, end_signal: oneshot::Sender<()>) {
        // Note that if this send() returns Err, it means the task has already
        // ended, but the completion has not been processed yet (otherwise we wouldn't have access to the end_signal),
        // so we should still add it to the "expect to drain" set
        _ = end_signal.send(());
        self.consumer_state.expect_drain.insert(tp);
    }

    /// Add the given TopicPartition to the set of known "drained" partitions,
    /// i.e. the consumer has drained the acknowledgement channel. A signal is
    /// sent on the signal channel, indicating to the client that offsets may be committed
    fn partition_drained(&mut self, tp: TopicPartition) {
        // This send() will only return Err if the receiver has already been disconnected (i.e. the
        // kafka client task is no longer running)
        _ = self.consumer_state.signal.send(());
        self.consumer_state.expect_drain.remove(&tp);
    }

    /// Return true if all expected partitions have drained
    fn is_drain_complete(&self) -> bool {
        self.consumer_state.is_complete()
    }

    /// Finish partition drain mode. Consumes self and the drain deadline
    /// future, and returns a "Consuming" or "Complete" ConsumerState
    fn finish_drain(self, deadline: OptionDeadline) -> (OptionDeadline, ConsumerState) {
        if self.consumer_state.shutdown {
            self.complete(deadline)
        } else {
            (
                None.into(),
                ConsumerState::Consuming(ConsumerStateInner {
                    config: self.config,
                    decoder: self.decoder,
                    decompressor: self.decompressor,
                    out: self.out,
                    log_namespace: self.log_namespace,
                    consumer_state: Consuming {
                        span: self.consumer_state.span,
                    },
                }),
            )
        }
    }

    pub const fn keep_draining(self, deadline: OptionDeadline) -> (OptionDeadline, ConsumerState) {
        (deadline, ConsumerState::Draining(self))
    }
}

async fn coordinate_kafka_callbacks(
    consumer: Arc<StreamConsumer<KafkaSourceContext>>,
    mut callbacks: UnboundedReceiver<KafkaCallback>,
    consumer_state: ConsumerStateInner<Consuming>,
    max_drain_ms: Duration,
    mut eof: Option<oneshot::Sender<()>>,
) {
    let mut drain_deadline: OptionFuture<_> = None.into();
    let mut consumer_state = ConsumerState::Consuming(consumer_state);

    // A oneshot channel is used for each consumed partition, so that we can
    // signal to that task to stop consuming, drain pending acks, and exit
    let mut end_signals: HashMap<TopicPartition, oneshot::Sender<()>> = HashMap::new();

    // The set of consumer tasks, each consuming a specific partition. The task
    // is both consuming the messages (passing them to the output stream) _and_
    // processing the corresponding acknowledgement stream. A consumer task
    // should completely drain its acknowledgement stream after receiving an end signal
    let mut partition_consumers: JoinSet<(TopicPartition, PartitionConsumerStatus)> =
        Default::default();

    // Handles that will let us end any consumer task that exceeds a drain deadline
    let mut abort_handles: HashMap<TopicPartition, tokio::task::AbortHandle> = HashMap::new();

    let exit_eof = eof.is_some();

    while let ConsumerState::Consuming(_) | ConsumerState::Draining(_) = consumer_state {
        tokio::select! {
            Some(Ok((finished_partition, status))) = partition_consumers.join_next(), if !partition_consumers.is_empty() => {
                debug!("Partition consumer finished for {}:{}", &finished_partition.0, finished_partition.1);
                // If this task ended on its own, the end_signal for it will still be in here.
                end_signals.remove(&finished_partition);
                abort_handles.remove(&finished_partition);

                (drain_deadline, consumer_state) = match consumer_state {
                    ConsumerState::Complete => unreachable!("Partition consumer finished after completion."),
                    ConsumerState::Draining(mut state) => {
                        state.partition_drained(finished_partition);

                        if state.is_drain_complete() {
                            debug!("All expected partitions have drained.");
                            state.finish_drain(drain_deadline)
                        } else {
                            state.keep_draining(drain_deadline)
                        }
                    },
                    ConsumerState::Consuming(state) => {
                        // If we are here, it is likely because the consumer
                        // tasks are set up to exit upon reaching the end of the
                        // partition.
                        if !exit_eof {
                            debug!("Partition consumer task finished, while not in draining mode.");
                        }
                        state.keep_consuming(drain_deadline)
                    },
                };

                // PartitionConsumerStatus differentiates between a task that exited after
                // being signaled to end, and one that reached the end of its partition and
                // was configured to exit. After the last such task ends, we signal the kafka
                // driver task to shut down the main consumer too. Note this is only used in tests.
                if exit_eof && status == PartitionConsumerStatus::PartitionEOF && partition_consumers.is_empty() {
                    debug!("All partitions have exited or reached EOF.");
                    let _ = eof.take().map(|e| e.send(()));
                }
            },
            Some(callback) = callbacks.recv() => match callback {
                KafkaCallback::PartitionsAssigned(mut assigned_partitions, done) => match consumer_state {
                    ConsumerState::Complete => unreachable!("Partition assignment received after completion."),
                    ConsumerState::Draining(_) => error!("Partition assignment received while draining revoked partitions, maybe an invalid assignment."),
                    ConsumerState::Consuming(ref consumer_state) => {
                        let acks = consumer.context().acknowledgements;
                        for tp in assigned_partitions.drain(0..) {
                            let topic = tp.0.as_str();
                            let partition = tp.1;
                            match consumer.split_partition_queue(topic, partition) { Some(pq) => {
                                debug!("Consuming partition {}:{}.", &tp.0, tp.1);
                                let (end_tx, handle) = consumer_state.consume_partition(&mut partition_consumers, tp.clone(), Arc::clone(&consumer), pq, acks, exit_eof);
                                abort_handles.insert(tp.clone(), handle);
                                end_signals.insert(tp, end_tx);
                            } _ => {
                                warn!("Failed to get queue for assigned partition {}:{}.", &tp.0, tp.1);
                            }}
                        }
                        // ensure this is retained until all individual queues are set up
                        drop(done);
                    }
                },
                KafkaCallback::PartitionsRevoked(mut revoked_partitions, drain) => (drain_deadline, consumer_state) = match consumer_state {
                    ConsumerState::Complete => unreachable!("Partitions revoked after completion."),
                    ConsumerState::Draining(d) => {
                        // NB: This would only happen if the task driving the kafka client (i.e. rebalance handlers)
                        // is not handling shutdown signals, and a revoke happens during a shutdown drain; otherwise
                        // this is unreachable code.
                        warn!("Kafka client is already draining revoked partitions.");
                        d.keep_draining(drain_deadline)
                    },
                    ConsumerState::Consuming(state) => {
                        let (deadline, mut state) = state.begin_drain(max_drain_ms, drain, false);

                        for tp in revoked_partitions.drain(0..) {
                            match end_signals.remove(&tp) { Some(end) => {
                                debug!("Revoking partition {}:{}", &tp.0, tp.1);
                                state.revoke_partition(tp, end);
                            } _ => {
                                debug!("Consumer task for partition {}:{} already finished.", &tp.0, tp.1);
                            }}
                        }

                        state.keep_draining(deadline)
                    }
                },
                KafkaCallback::ShuttingDown(drain) => (drain_deadline, consumer_state) = match consumer_state {
                    ConsumerState::Complete => unreachable!("Shutdown received after completion."),
                    // Shutting down is just like a full assignment revoke, but we also close the
                    // callback channels, since we don't expect additional assignments or rebalances
                    ConsumerState::Draining(state) => {
                        // NB: This would only happen if the task driving the kafka client is
                        // not handling shutdown signals; otherwise this is unreachable code
                        error!("Kafka client handled a shutdown signal while a rebalance was in progress.");
                        callbacks.close();
                        state.keep_draining(drain_deadline)
                    },
                    ConsumerState::Consuming(state) => {
                        callbacks.close();
                        let (deadline, mut state) = state.begin_drain(max_drain_ms, drain, true);
                        if let Ok(tpl) = consumer.assignment() {
                            // TODO  workaround for https://github.com/fede1024/rust-rdkafka/issues/681
                            if tpl.capacity() == 0 {
                                return;
                            }
                            tpl.elements()
                                .iter()
                                .for_each(|el| {

                                let tp: TopicPartition = (el.topic().into(), el.partition());
                                match end_signals.remove(&tp) { Some(end) => {
                                    debug!("Shutting down and revoking partition {}:{}", &tp.0, tp.1);
                                    state.revoke_partition(tp, end);
                                } _ => {
                                    debug!("Consumer task for partition {}:{} already finished.", &tp.0, tp.1);
                                }}
                            });
                        }
                        // If shutdown was initiated by partition EOF mode, the drain phase
                        // will already be complete and would time out if not accounted for here
                        if state.is_drain_complete() {
                            state.finish_drain(deadline)
                        } else {
                            state.keep_draining(deadline)
                        }
                    }
                },
            },

            Some(_) = &mut drain_deadline => (drain_deadline, consumer_state) = match consumer_state {
                ConsumerState::Complete => unreachable!("Drain deadline received after completion."),
                ConsumerState::Consuming(state) => {
                    warn!("A drain deadline fired outside of draining mode.");
                    state.keep_consuming(None.into())
                },
                ConsumerState::Draining(mut draining) => {
                    debug!("Acknowledgement drain deadline reached. Dropping any pending ack streams for revoked partitions.");
                    for tp in draining.consumer_state.expect_drain.drain() {
                        if let Some(handle) = abort_handles.remove(&tp) {
                            handle.abort();
                        }
                    }
                    draining.finish_drain(drain_deadline)
                }
            },
        }
    }
}

fn drive_kafka_consumer(
    consumer: Arc<StreamConsumer<KafkaSourceContext>>,
    mut shutdown: ShutdownSignal,
    eof: Option<oneshot::Receiver<()>>,
) {
    Handle::current().block_on(async move {
        let mut eof: OptionFuture<_> = eof.into();
        let mut stream = consumer.stream();
        loop {
            tokio::select! {
                _ = &mut shutdown => {
                    consumer.context().shutdown();
                    break
                },

                Some(_) = &mut eof => {
                    consumer.context().shutdown();
                    break
                },

                // NB: messages are not received on this thread, however we poll
                // the consumer to serve client callbacks, such as rebalance notifications
                message = stream.next() => match message {
                    None => unreachable!("MessageStream never returns Ready(None)"),
                    Some(Err(error)) => emit!(KafkaReadError { error }),
                    Some(Ok(_msg)) => {
                        unreachable!("Messages are consumed in dedicated tasks for each partition.")
                    }
                },
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
async fn parse_message(
    msg: BorrowedMessage<'_>,
    decoder: Decoder,
    decompressor: Option<&Decompressor>,
    keys: &'_ Keys,
    out: &mut SourceSender,
    acknowledgements: bool,
    finalizer: &Option<OrderedFinalizer<FinalizerEntry>>,
    log_namespace: LogNamespace,
) {
    if let Some((count, stream)) = parse_stream(&msg, decoder, decompressor, keys, log_namespace) {
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let mut stream = stream.map(|event| {
            // All acknowledgements flow through the normal Finalizer stream so
            // that they can be handled in one place, but are only tied to the
            // batch when acknowledgements are enabled
            if acknowledgements {
                event.with_batch_notifier(&batch)
            } else {
                event
            }
        });
        match out.send_event_stream(&mut stream).await {
            Err(_) => {
                emit!(StreamClosedError { count });
            }
            Ok(_) => {
                // Drop stream to avoid borrowing `msg`: "[...] borrow might be used
                // here, when `stream` is dropped and runs the destructor [...]".
                drop(stream);
                if let Some(f) = finalizer.as_ref() {
                    f.add(msg.into(), receiver)
                }
            }
        }
    }
}

// Turn the received message into a stream of parsed events.
fn parse_stream<'a>(
    msg: &BorrowedMessage<'a>,
    decoder: Decoder,
    decompressor: Option<&Decompressor>,
    keys: &'a Keys,
    log_namespace: LogNamespace,
) -> Option<(usize, impl Stream<Item = Event> + 'a + use<'a>)> {
    let payload = msg.payload()?; // skip messages with empty payload

    let rmsg = ReceivedMessage::from(msg);

    let payload = match decompressor {
        Some(decompressor) => match decompressor.decompress(payload) {
            Ok(decompressed) => Bytes::from(decompressed),
            Err(error) => {
                emit!(KafkaPayloadDecompressionError {
                    error: &error,
                    topic: msg.topic(),
                    partition: msg.partition(),
                    offset: msg.offset(),
                });
                // Skip messages that cannot be decompressed, but still return an (empty) stream to commit the offset.
                // Decompression failures are generally deterministic, so redelivery doesn't help.
                return Some((0, futures::stream::empty().boxed()));
            }
        },
        None => Bytes::copy_from_slice(payload),
    };

    let payload_len = payload.len();
    let payload = Cursor::new(payload);

    let mut stream = DecoderFramedRead::with_capacity(payload, decoder, payload_len);
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
                        rmsg.apply(keys, &mut event, log_namespace);
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

#[derive(Clone, Debug)]
struct Keys {
    timestamp: Option<OwnedValuePath>,
    key_field: Option<OwnedValuePath>,
    topic: Option<OwnedValuePath>,
    partition: Option<OwnedValuePath>,
    offset: Option<OwnedValuePath>,
    headers: Option<OwnedValuePath>,
}

impl Keys {
    fn from(schema: &LogSchema, config: &KafkaSourceConfig) -> Self {
        Self {
            timestamp: schema.timestamp_key().cloned(),
            key_field: config.key_field.path.clone(),
            topic: config.topic_key.path.clone(),
            partition: config.partition_key.path.clone(),
            offset: config.offset_key.path.clone(),
            headers: config.headers_key.path.clone(),
        }
    }
}

struct ReceivedMessage {
    timestamp: Option<DateTime<Utc>>,
    key: Value,
    headers: ObjectMap,
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

        let mut headers_map = ObjectMap::new();
        if let Some(headers) = msg.headers() {
            for header in headers.iter() {
                if let Some(value) = header.value {
                    headers_map.insert(
                        header.key.into(),
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

    fn apply(&self, keys: &Keys, event: &mut Event, log_namespace: LogNamespace) {
        if let Event::Log(log) = event {
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
                    if let Some(source_type_key) = log_schema().source_type_key_target_path() {
                        log.insert(source_type_key, KafkaSourceConfig::NAME);
                    }
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
                keys.timestamp.as_ref().map(LegacyKey::Overwrite),
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

#[derive(Debug, Eq, PartialEq, Hash)]
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

fn create_consumer(
    config: &KafkaSourceConfig,
    acknowledgements: bool,
) -> crate::Result<(
    StreamConsumer<KafkaSourceContext>,
    UnboundedReceiver<KafkaCallback>,
)> {
    let mut client_config = ClientConfig::new();
    client_config
        .set("group.id", &config.group_id)
        .set("bootstrap.servers", &config.bootstrap_servers)
        .set("auto.offset.reset", &config.auto_offset_reset)
        .set(
            "session.timeout.ms",
            config.session_timeout_ms.as_millis().to_string(),
        )
        .set(
            "socket.timeout.ms",
            config.socket_timeout_ms.as_millis().to_string(),
        )
        .set(
            "fetch.wait.max.ms",
            config.fetch_wait_max_ms.as_millis().to_string(),
        )
        .set("enable.partition.eof", "false")
        .set("enable.auto.commit", "true")
        .set(
            "auto.commit.interval.ms",
            config.commit_interval_ms.as_millis().to_string(),
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

    let (callbacks, callback_rx) = mpsc::unbounded_channel();
    let consumer = client_config
        .create_with_context::<_, StreamConsumer<_>>(KafkaSourceContext::new(
            config.metrics.topic_lag_metric,
            acknowledgements,
            callbacks,
            Span::current(),
        ))
        .context(CreateSnafu)?;

    Ok((consumer, callback_rx))
}

type TopicPartition = (String, i32);

/// Status returned by partition consumer tasks, allowing the coordination task
/// to differentiate between a consumer exiting normally (after receiving an end
/// signal) and exiting when it reaches the end of a partition
#[derive(PartialEq)]
enum PartitionConsumerStatus {
    NormalExit,
    PartitionEOF,
}

enum KafkaCallback {
    PartitionsAssigned(Vec<TopicPartition>, SyncSender<()>),
    PartitionsRevoked(Vec<TopicPartition>, SyncSender<()>),
    ShuttingDown(SyncSender<()>),
}

struct KafkaSourceContext {
    acknowledgements: bool,
    stats: kafka::KafkaStatisticsContext,

    /// A callback channel used to coordinate between the main consumer task and the acknowledgement task
    callbacks: UnboundedSender<KafkaCallback>,

    /// A weak reference to the consumer, so that we can commit offsets during a rebalance operation
    consumer: OnceLock<Weak<StreamConsumer<KafkaSourceContext>>>,
}

impl KafkaSourceContext {
    fn new(
        expose_lag_metrics: bool,
        acknowledgements: bool,
        callbacks: UnboundedSender<KafkaCallback>,
        span: Span,
    ) -> Self {
        Self {
            stats: kafka::KafkaStatisticsContext {
                expose_lag_metrics,
                span,
            },
            acknowledgements,
            consumer: OnceLock::default(),
            callbacks,
        }
    }

    fn shutdown(&self) {
        let (send, rendezvous) = sync_channel(0);
        if self
            .callbacks
            .send(KafkaCallback::ShuttingDown(send))
            .is_ok()
        {
            while rendezvous.recv().is_ok() {
                self.commit_consumer_state();
            }
        }
    }

    /// Emit a PartitionsAssigned callback with the topic-partitions to be consumed,
    /// and block until confirmation is received that a stream and consumer for
    /// each topic-partition has been set up. This function blocks until the
    /// rendezvous channel sender is dropped by the callback handler.
    fn consume_partitions(&self, tpl: &TopicPartitionList) {
        // TODO  workaround for https://github.com/fede1024/rust-rdkafka/issues/681
        if tpl.capacity() == 0 {
            return;
        }
        let (send, rendezvous) = sync_channel(0);
        self.callbacks
            .send(KafkaCallback::PartitionsAssigned(
                tpl.elements()
                    .iter()
                    .map(|tp| (tp.topic().into(), tp.partition()))
                    .collect(),
                send,
            ))
            .ok();

        while rendezvous.recv().is_ok() {
            // no-op: wait for partition assignment handler to complete
        }
    }

    /// Emit a PartitionsRevoked callback and block until confirmation is
    /// received that acknowledgements have been processed for each of them.
    /// The rendezvous channel used in the callback can send multiple times to
    /// signal individual partitions completing. This function blocks until the
    /// sender is dropped by the callback handler.
    fn revoke_partitions(&self, tpl: &TopicPartitionList) {
        let (send, rendezvous) = sync_channel(0);
        self.callbacks
            .send(KafkaCallback::PartitionsRevoked(
                tpl.elements()
                    .iter()
                    .map(|tp| (tp.topic().into(), tp.partition()))
                    .collect(),
                send,
            ))
            .ok();

        while rendezvous.recv().is_ok() {
            self.commit_consumer_state();
        }
    }

    fn commit_consumer_state(&self) {
        if let Some(consumer) = self
            .consumer
            .get()
            .expect("Consumer reference was not initialized.")
            .upgrade()
        {
            match consumer.commit_consumer_state(CommitMode::Sync) {
                Ok(_) | Err(KafkaError::ConsumerCommit(RDKafkaErrorCode::NoOffset)) => {
                    /* Success, or nothing to do - yay \0/ */
                }
                Err(error) => emit!(KafkaOffsetUpdateError { error }),
            }
        }
    }
}

impl ClientContext for KafkaSourceContext {
    fn stats(&self, statistics: Statistics) {
        self.stats.stats(statistics)
    }
}

impl ConsumerContext for KafkaSourceContext {
    fn pre_rebalance(&self, _base_consumer: &BaseConsumer<Self>, rebalance: &Rebalance) {
        match rebalance {
            Rebalance::Assign(tpl) => self.consume_partitions(tpl),

            Rebalance::Revoke(tpl) => {
                self.revoke_partitions(tpl);
                self.commit_consumer_state();
            }

            Rebalance::Error(message) => {
                error!("Error during Kafka consumer group rebalance: {}.", message);
            }
        }
    }
}

#[cfg(test)]
mod test;

#[cfg(feature = "kafka-integration-tests")]
#[cfg(test)]
mod integration_test;
