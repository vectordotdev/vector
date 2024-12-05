use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    pin::Pin,
    sync::{
        mpsc::{sync_channel, SyncSender},
        Arc, OnceLock, Weak,
    },
    time::Duration,
};

use async_stream::stream;
use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use futures::{Stream, StreamExt};
use futures_util::future::OptionFuture;
use rdkafka::{
    consumer::{
        stream_consumer::StreamPartitionQueue, CommitMode, Consumer, ConsumerContext, Rebalance,
        StreamConsumer,
    },
    error::KafkaError,
    message::{BorrowedMessage, Headers as _, Message},
    types::RDKafkaErrorCode,
    ClientConfig, ClientContext, Statistics, TopicPartitionList,
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
use tokio_util::codec::FramedRead;
use tracing::{Instrument, Span};
use vector_lib::codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path, path, OwnedValuePath};

use vector_lib::configurable::configurable_component;
use vector_lib::finalizer::OrderedFinalizer;
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    EstimatedJsonEncodedSizeOf,
};
use vrl::value::{kind::Collection, Kind, ObjectMap};

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{
        log_schema, LogSchema, SourceAcknowledgementsConfig, SourceConfig, SourceContext,
        SourceOutput,
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
    #[configurable(metadata(docs::advanced))]
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
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(docs::human_name = "Drain Timeout"))]
    drain_timeout_ms: Option<u64>,

    /// Timeout for network requests.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::examples = 30000, docs::examples = 60000))]
    #[configurable(metadata(docs::advanced))]
    #[serde(default = "default_socket_timeout_ms")]
    #[configurable(metadata(docs::human_name = "Socket Timeout"))]
    socket_timeout_ms: Duration,

    /// Maximum time the broker may wait to fill the response.
    #[serde_as(as = "serde_with::DurationMilliSeconds<u64>")]
    #[configurable(metadata(docs::examples = 50, docs::examples = 100))]
    #[configurable(metadata(docs::advanced))]
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
    #[configurable(metadata(docs::advanced))]
    #[configurable(metadata(
        docs::additional_props_description = "A librdkafka configuration option."
    ))]
    librdkafka_options: Option<HashMap<String, String>>,

    #[serde(flatten)]
    auth: kafka::KafkaAuthConfig,

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
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
        let consumer_state =
            ConsumerStateInner::<Consuming>::new(config, decoder, out, log_namespace, span);
        tokio::spawn(async move {
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
        out: SourceSender,
        log_namespace: LogNamespace,
        span: Span,
    ) -> Self {
        Self {
            config,
            decoder,
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
                            if status == BatchStatus::Delivered {
                                if let Err(error) =  consumer.store_offset(&entry.topic, entry.partition, entry.offset) {
                                    emit!(KafkaOffsetUpdateError { error });
                                }
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
                            parse_message(msg, decoder.clone(), &keys, &mut out, acknowledgements, &finalizer, log_namespace).await;
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
                            if let Some(pq) = consumer.split_partition_queue(topic, partition) {
                                debug!("Consuming partition {}:{}.", &tp.0, tp.1);
                                let (end_tx, handle) = consumer_state.consume_partition(&mut partition_consumers, tp.clone(), Arc::clone(&consumer), pq, acks, exit_eof);
                                abort_handles.insert(tp.clone(), handle);
                                end_signals.insert(tp, end_tx);
                            } else {
                                warn!("Failed to get queue for assigned partition {}:{}.", &tp.0, tp.1);
                            }
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
                            if let Some(end) = end_signals.remove(&tp) {
                                debug!("Revoking partition {}:{}", &tp.0, tp.1);
                                state.revoke_partition(tp, end);
                            } else {
                                debug!("Consumer task for partition {}:{} already finished.", &tp.0, tp.1);
                            }
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
                                if let Some(end) = end_signals.remove(&tp) {
                                    debug!("Shutting down and revoking partition {}:{}", &tp.0, tp.1);
                                    state.revoke_partition(tp, end);
                                } else {
                                    debug!("Consumer task for partition {}:{} already finished.", &tp.0, tp.1);
                                }
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

async fn parse_message(
    msg: BorrowedMessage<'_>,
    decoder: Decoder,
    keys: &'_ Keys,
    out: &mut SourceSender,
    acknowledgements: bool,
    finalizer: &Option<OrderedFinalizer<FinalizerEntry>>,
    log_namespace: LogNamespace,
) {
    if let Some((count, stream)) = parse_stream(&msg, decoder, keys, log_namespace) {
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
    keys: &'a Keys,
    log_namespace: LogNamespace,
) -> Option<(usize, impl Stream<Item = Event> + 'a)> {
    let payload = msg.payload()?; // skip messages with empty payload

    let rmsg = ReceivedMessage::from(msg);

    let payload = Cursor::new(Bytes::copy_from_slice(payload));

    let mut stream = FramedRead::with_capacity(payload, decoder, msg.payload_len());
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
        let _ = self.callbacks.send(KafkaCallback::PartitionsAssigned(
            tpl.elements()
                .iter()
                .map(|tp| (tp.topic().into(), tp.partition()))
                .collect(),
            send,
        ));

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
        let _ = self.callbacks.send(KafkaCallback::PartitionsRevoked(
            tpl.elements()
                .iter()
                .map(|tp| (tp.topic().into(), tp.partition()))
                .collect(),
            send,
        ));

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
    fn pre_rebalance(&self, rebalance: &Rebalance) {
        match rebalance {
            Rebalance::Assign(tpl) => self.consume_partitions(tpl),

            Rebalance::Revoke(tpl) => {
                // TODO  workaround for https://github.com/fede1024/rust-rdkafka/issues/681
                if tpl.capacity() == 0 {
                    return;
                }
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
mod test {
    use vector_lib::lookup::OwnedTargetPath;
    use vector_lib::schema::Definition;

    use super::*;

    pub fn kafka_host() -> String {
        std::env::var("KAFKA_HOST").unwrap_or_else(|_| "localhost".into())
    }
    pub fn kafka_port() -> u16 {
        let port = std::env::var("KAFKA_PORT").unwrap_or_else(|_| "9091".into());
        port.parse().expect("Invalid port number")
    }

    pub fn kafka_address() -> String {
        format!("{}:{}", kafka_host(), kafka_port())
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<KafkaSourceConfig>();
    }

    pub(super) fn make_config(
        topic: &str,
        group: &str,
        log_namespace: LogNamespace,
        librdkafka_options: Option<HashMap<String, String>>,
    ) -> KafkaSourceConfig {
        KafkaSourceConfig {
            bootstrap_servers: kafka_address(),
            topics: vec![topic.into()],
            group_id: group.into(),
            auto_offset_reset: "beginning".into(),
            session_timeout_ms: Duration::from_millis(6000),
            commit_interval_ms: Duration::from_millis(1),
            librdkafka_options,
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
        let definitions = make_config("topic", "group", LogNamespace::Vector, None)
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        assert_eq!(
            definitions,
            Some(
                Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                    .with_meaning(OwnedTargetPath::event_root(), "message")
                    .with_metadata_field(
                        &owned_value_path!("kafka", "timestamp"),
                        Kind::timestamp(),
                        Some("timestamp")
                    )
                    .with_metadata_field(
                        &owned_value_path!("kafka", "message_key"),
                        Kind::bytes(),
                        None
                    )
                    .with_metadata_field(&owned_value_path!("kafka", "topic"), Kind::bytes(), None)
                    .with_metadata_field(
                        &owned_value_path!("kafka", "partition"),
                        Kind::bytes(),
                        None
                    )
                    .with_metadata_field(&owned_value_path!("kafka", "offset"), Kind::bytes(), None)
                    .with_metadata_field(
                        &owned_value_path!("kafka", "headers"),
                        Kind::object(Collection::empty().with_unknown(Kind::bytes())),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("vector", "ingest_timestamp"),
                        Kind::timestamp(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("vector", "source_type"),
                        Kind::bytes(),
                        None
                    )
            )
        )
    }

    #[test]
    fn test_output_schema_definition_legacy_namespace() {
        let definitions = make_config("topic", "group", LogNamespace::Legacy, None)
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        assert_eq!(
            definitions,
            Some(
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
        )
    }

    #[tokio::test]
    async fn consumer_create_ok() {
        let config = make_config("topic", "group", LogNamespace::Legacy, None);
        assert!(create_consumer(&config, true).is_ok());
    }

    #[tokio::test]
    async fn consumer_create_incorrect_auto_offset_reset() {
        let config = KafkaSourceConfig {
            auto_offset_reset: "incorrect-auto-offset-reset".to_string(),
            ..make_config("topic", "group", LogNamespace::Legacy, None)
        };
        assert!(create_consumer(&config, true).is_err());
    }
}

#[cfg(feature = "kafka-integration-tests")]
#[cfg(test)]
mod integration_test {
    use std::time::Duration;

    use chrono::{DateTime, SubsecRound, Utc};
    use futures::Stream;
    use futures_util::stream::FuturesUnordered;
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
    use vector_lib::event::EventStatus;
    use vrl::{event_path, value};

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

    fn kafka_test_topic() -> String {
        std::env::var("KAFKA_TEST_TOPIC")
            .unwrap_or_else(|_| format!("test-topic-{}", random_string(10)))
    }
    fn kafka_max_bytes() -> String {
        std::env::var("KAFKA_MAX_BYTES").unwrap_or_else(|_| "1024".into())
    }

    fn client_config<T: FromClientConfig>(group: Option<&str>) -> T {
        let mut client = ClientConfig::new();
        client.set("bootstrap.servers", kafka_address());
        client.set("produce.offset.report", "true");
        client.set("message.timeout.ms", "5000");
        client.set("auto.commit.interval.ms", "1");
        if let Some(group) = group {
            client.set("group.id", group);
        }
        client.create().expect("Producer creation error")
    }

    async fn send_events(topic: String, partitions: i32, count: usize) -> DateTime<Utc> {
        let now = Utc::now();
        let timestamp = now.timestamp_millis();

        let producer: &FutureProducer = &client_config(None);
        let topic_name = topic.as_ref();

        create_topic(topic_name, partitions).await;

        (0..count)
            .map(|i| async move {
                let text = format!("{} {:03}", TEXT, i);
                let key = format!("{} {}", KEY, i);
                let record = FutureRecord::to(topic_name)
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
            })
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .await;

        now
    }

    async fn send_to_test_topic(partitions: i32, count: usize) -> (String, String, DateTime<Utc>) {
        let topic = kafka_test_topic();
        let group_id = format!("test-group-{}", random_string(10));

        let sent_at = send_events(topic.clone(), partitions, count).await;

        (topic, group_id, sent_at)
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
        let config = make_config(&topic, &group_id, log_namespace, None);

        let now = send_events(topic.clone(), 1, 10).await;

        let events = assert_source_compliance(&["protocol", "topic", "partition"], async move {
            let (tx, rx) = SourceSender::new_test_errors(error_at);
            let (trigger_shutdown, shutdown_done) =
                spawn_kafka(tx, config, acknowledgements, false, log_namespace);
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
                    event.as_log()[log_schema().message_key().unwrap().to_string()],
                    format!("{} {:03}", TEXT, i).into()
                );
                assert_eq!(
                    event.as_log()["message_key"],
                    format!("{} {}", KEY, i).into()
                );
                assert_eq!(
                    event.as_log()[log_schema().source_type_key().unwrap().to_string()],
                    "kafka".into()
                );
                assert_eq!(
                    event.as_log()[log_schema().timestamp_key().unwrap().to_string()],
                    now.trunc_subsecs(3).into()
                );
                assert_eq!(event.as_log()["topic"], topic.clone().into());
                assert!(event.as_log().contains("partition"));
                assert!(event.as_log().contains("offset"));
                let mut expected_headers = ObjectMap::new();
                expected_headers.insert(HEADER_KEY.into(), Value::from(HEADER_VALUE));
                assert_eq!(event.as_log()["headers"], Value::from(expected_headers));
            } else {
                let meta = event.as_log().metadata().value();

                assert_eq!(
                    meta.get(path!("vector", "source_type")).unwrap(),
                    &value!(KafkaSourceConfig::NAME)
                );
                assert!(meta
                    .get(path!("vector", "ingest_timestamp"))
                    .unwrap()
                    .is_timestamp());

                assert_eq!(
                    event.as_log().value(),
                    &value!(format!("{} {:03}", TEXT, i))
                );
                assert_eq!(
                    meta.get(path!("kafka", "message_key")).unwrap(),
                    &value!(format!("{} {}", KEY, i))
                );

                assert_eq!(
                    meta.get(path!("kafka", "timestamp")).unwrap(),
                    &value!(now.trunc_subsecs(3))
                );
                assert_eq!(
                    meta.get(path!("kafka", "topic")).unwrap(),
                    &value!(topic.clone())
                );
                assert!(meta.get(path!("kafka", "partition")).unwrap().is_integer(),);
                assert!(meta.get(path!("kafka", "offset")).unwrap().is_integer(),);

                let mut expected_headers = ObjectMap::new();
                expected_headers.insert(HEADER_KEY.into(), Value::from(HEADER_VALUE));
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
        let config = make_config(&topic, &group_id, LogNamespace::Legacy, None);
        (topic, group_id, config)
    }

    fn delay_pipeline(
        id: usize,
        delay: Duration,
        status: EventStatus,
    ) -> (SourceSender, impl Stream<Item = EventArray> + Unpin) {
        let (pipe, recv) = SourceSender::new_test_sender_with_buffer(100);
        let recv = recv.into_stream();
        let recv = recv.then(move |item| async move {
            let mut events = item.events;
            events.iter_logs_mut().for_each(|log| {
                log.insert(event_path!("pipeline_id"), id.to_string());
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
        out: SourceSender,
        config: KafkaSourceConfig,
        acknowledgements: bool,
        eof: bool,
        log_namespace: LogNamespace,
    ) -> (Trigger, Tripwire) {
        let (trigger_shutdown, shutdown, shutdown_done) = ShutdownSignal::new_wired();

        let decoder = DecodingConfig::new(
            config.framing.clone(),
            config.decoding.clone(),
            log_namespace,
        )
        .build()
        .unwrap();

        let (consumer, callback_rx) = create_consumer(&config, acknowledgements).unwrap();

        tokio::spawn(kafka_source(
            config,
            consumer,
            callback_rx,
            decoder,
            out,
            shutdown,
            eof,
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

    async fn create_topic(topic: &str, partitions: i32) {
        let client: AdminClient<DefaultClientContext> = client_config(None);
        let topic_results = client
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
            .expect("create_topics failed");

        for result in topic_results {
            if let Err((topic, err)) = result {
                if err != rdkafka::types::RDKafkaErrorCode::TopicAlreadyExists {
                    panic!("Creating a topic failed: {:?}", (topic, err))
                }
            }
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
        create_topic(&topic, 2).await;

        let _send_start = send_events(topic.clone(), 1, NEVENTS).await;

        let (tx, rx1) = delay_pipeline(1, Duration::from_millis(200), EventStatus::Delivered);
        let (trigger_shutdown1, shutdown_done1) =
            spawn_kafka(tx, config.clone(), true, false, LogNamespace::Legacy);
        let events1 = tokio::spawn(collect_n(rx1, NEVENTS));

        sleep(Duration::from_secs(1)).await;

        let (tx, rx2) = delay_pipeline(2, Duration::from_millis(DELAY), EventStatus::Delivered);
        let (trigger_shutdown2, shutdown_done2) =
            spawn_kafka(tx, config, true, false, LogNamespace::Legacy);
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

    #[tokio::test]
    async fn drains_acknowledgements_at_shutdown() {
        // 1. Send N events (if running against a pre-populated kafka topic, use send_count=0 and expect_count=expected number of messages; otherwise just set send_count)
        let send_count: usize = std::env::var("KAFKA_SEND_COUNT")
            .unwrap_or_else(|_| "125000".into())
            .parse()
            .expect("Number of messages to send to kafka.");
        let expect_count: usize = std::env::var("KAFKA_EXPECT_COUNT")
            .unwrap_or_else(|_| format!("{}", send_count))
            .parse()
            .expect("Number of messages to expect consumers to process.");
        let delay_ms: u64 = std::env::var("KAFKA_SHUTDOWN_DELAY")
            .unwrap_or_else(|_| "2000".into())
            .parse()
            .expect("Number of milliseconds before shutting down first consumer.");

        let (topic, group_id, _) = send_to_test_topic(1, send_count).await;

        // 2. Run the kafka source to read some of the events
        // 3. Send a shutdown signal (at some point before all events are read)
        let mut opts = HashMap::new();
        // Set options to get partition EOF notifications, and fetch data in small/configurable size chunks
        opts.insert("enable.partition.eof".into(), "true".into());
        opts.insert("fetch.message.max.bytes".into(), kafka_max_bytes());
        let events1 = {
            let config = make_config(&topic, &group_id, LogNamespace::Legacy, Some(opts.clone()));
            let (tx, rx) = SourceSender::new_test_errors(|_| false);
            let (trigger_shutdown, shutdown_done) =
                spawn_kafka(tx, config, true, false, LogNamespace::Legacy);
            let (events, _) = tokio::join!(rx.collect::<Vec<Event>>(), async move {
                sleep(Duration::from_millis(delay_ms)).await;
                drop(trigger_shutdown);
            });
            shutdown_done.await;
            events
        };

        debug!("Consumer group.id: {}", &group_id);
        debug!(
            "First consumer read {} of {} messages.",
            events1.len(),
            expect_count
        );

        // 4. Run the kafka source again to finish reading the events
        let events2 = {
            let config = make_config(&topic, &group_id, LogNamespace::Legacy, Some(opts));
            let (tx, rx) = SourceSender::new_test_errors(|_| false);
            let (trigger_shutdown, shutdown_done) =
                spawn_kafka(tx, config, true, true, LogNamespace::Legacy);
            let events = rx.collect::<Vec<Event>>().await;
            drop(trigger_shutdown);
            shutdown_done.await;
            events
        };

        debug!(
            "Second consumer read {} of {} messages.",
            events2.len(),
            expect_count
        );

        // 5. Total number of events processed should equal the number sent
        let total = events1.len() + events2.len();
        assert_ne!(
            events1.len(),
            0,
            "First batch of events should be non-zero (increase KAFKA_SHUTDOWN_DELAY?)"
        );
        assert_ne!(events2.len(), 0, "Second batch of events should be non-zero (decrease KAFKA_SHUTDOWN_DELAY or increase KAFKA_SEND_COUNT?) ");
        assert_eq!(total, expect_count);
    }

    async fn consume_with_rebalance(rebalance_strategy: String) {
        // 1. Send N events (if running against a pre-populated kafka topic, use send_count=0 and expect_count=expected number of messages; otherwise just set send_count)
        let send_count: usize = std::env::var("KAFKA_SEND_COUNT")
            .unwrap_or_else(|_| "125000".into())
            .parse()
            .expect("Number of messages to send to kafka.");
        let expect_count: usize = std::env::var("KAFKA_EXPECT_COUNT")
            .unwrap_or_else(|_| format!("{}", send_count))
            .parse()
            .expect("Number of messages to expect consumers to process.");
        let delay_ms: u64 = std::env::var("KAFKA_CONSUMER_DELAY")
            .unwrap_or_else(|_| "2000".into())
            .parse()
            .expect("Number of milliseconds before shutting down first consumer.");

        let (topic, group_id, _) = send_to_test_topic(6, send_count).await;
        debug!("Topic: {}", &topic);
        debug!("Consumer group.id: {}", &group_id);

        // 2. Run the kafka source to read some of the events
        // 3. Start 2nd & 3rd consumers using the same group.id, triggering rebalance events
        let mut kafka_options = HashMap::new();
        kafka_options.insert("enable.partition.eof".into(), "true".into());
        kafka_options.insert("fetch.message.max.bytes".into(), kafka_max_bytes());
        kafka_options.insert("partition.assignment.strategy".into(), rebalance_strategy);
        let config1 = make_config(
            &topic,
            &group_id,
            LogNamespace::Legacy,
            Some(kafka_options.clone()),
        );
        let config2 = config1.clone();
        let config3 = config1.clone();
        let config4 = config1.clone();

        let (events1, events2, events3) = tokio::join!(
            async move {
                let (tx, rx) = SourceSender::new_test_errors(|_| false);
                let (_trigger_shutdown, _shutdown_done) =
                    spawn_kafka(tx, config1, true, true, LogNamespace::Legacy);

                rx.collect::<Vec<Event>>().await
            },
            async move {
                sleep(Duration::from_millis(delay_ms)).await;
                let (tx, rx) = SourceSender::new_test_errors(|_| false);
                let (_trigger_shutdown, _shutdown_done) =
                    spawn_kafka(tx, config2, true, true, LogNamespace::Legacy);

                rx.collect::<Vec<Event>>().await
            },
            async move {
                sleep(Duration::from_millis(delay_ms * 2)).await;
                let (tx, rx) = SourceSender::new_test_errors(|_| false);
                let (_trigger_shutdown, _shutdown_done) =
                    spawn_kafka(tx, config3, true, true, LogNamespace::Legacy);

                rx.collect::<Vec<Event>>().await
            }
        );

        let unconsumed = async move {
            let (tx, rx) = SourceSender::new_test_errors(|_| false);
            let (_trigger_shutdown, _shutdown_done) =
                spawn_kafka(tx, config4, true, true, LogNamespace::Legacy);

            rx.collect::<Vec<Event>>().await
        }
        .await;

        debug!(
            "First consumer read {} of {} messages.",
            events1.len(),
            expect_count
        );

        debug!(
            "Second consumer read {} of {} messages.",
            events2.len(),
            expect_count
        );
        debug!(
            "Third consumer read {} of {} messages.",
            events3.len(),
            expect_count
        );

        // 5. Total number of events processed should equal the number sent
        let total = events1.len() + events2.len() + events3.len();
        assert_ne!(
            events1.len(),
            0,
            "First batch of events should be non-zero (increase delay?)"
        );
        assert_ne!(
            events2.len(),
            0,
            "Second batch of events should be non-zero (decrease delay or increase KAFKA_SEND_COUNT?) "
        );
        assert_ne!(
            events3.len(),
            0,
            "Third batch of events should be non-zero (decrease delay or increase KAFKA_SEND_COUNT?) "
        );
        assert_eq!(
            unconsumed.len(),
            0,
            "The first set of consumers should consume and ack all messages."
        );
        assert_eq!(total, expect_count);
    }

    #[tokio::test]
    async fn drains_acknowledgements_during_rebalance_default_assignments() {
        // the default, eager rebalance strategies generally result in more revocations
        consume_with_rebalance("range,roundrobin".into()).await;
    }
    #[tokio::test]
    async fn drains_acknowledgements_during_rebalance_sticky_assignments() {
        // Cooperative rebalance strategies generally result in fewer revokes,
        // as only reassigned partitions are revoked
        consume_with_rebalance("cooperative-sticky".into()).await;
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
