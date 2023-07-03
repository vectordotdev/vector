use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Cursor,
    sync::{
        mpsc::{sync_channel, SyncSender},
        Arc, RwLock, Weak,
    },
    time::Duration,
};

use async_stream::stream;
use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use futures::{stream::BoxStream, Stream, StreamExt};
use lookup::{lookup_v2::OptionalValuePath, owned_value_path, path, OwnedValuePath};
use once_cell::sync::OnceCell;
use rdkafka::{
    consumer::{CommitMode, Consumer, ConsumerContext, Rebalance, StreamConsumer},
    error::KafkaError,
    message::{BorrowedMessage, Headers as _, Message},
    types::RDKafkaErrorCode,
    ClientConfig, ClientContext, Statistics,
};
use serde_with::serde_as;
use snafu::{ResultExt, Snafu};
use tokio::{
    runtime::Handle,
    sync::mpsc::{self, Receiver, Sender, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};
use tokio_util::codec::FramedRead;

use vector_common::{finalization::BatchStatusReceiver, finalizer::OrderedFinalizer};
use vector_config::configurable_component;
use vector_core::{
    config::{LegacyKey, LogNamespace},
    EstimatedJsonEncodedSizeOf,
};
use vrl::value::{kind::Collection, Kind};

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
    #[snafu(display("Could not create Kafka consumer: {}", source))]
    KafkaCreateError { source: rdkafka::error::KafkaError },
    #[snafu(display("Could not subscribe to Kafka topics: {}", source))]
    KafkaSubscribeError { source: rdkafka::error::KafkaError },
}

/// Metrics configuration.
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
#[typetag::serde(name = "kafka")]
impl SourceConfig for KafkaSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);

        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);
        let consumer = create_consumer(self, acknowledgements)?;
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace).build();

        Ok(Box::pin(kafka_source(
            self.clone(),
            consumer,
            decoder,
            cx.shutdown,
            cx.out,
            acknowledgements,
            #[cfg(test)]
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

        vec![SourceOutput::new_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

async fn kafka_source(
    config: KafkaSourceConfig,
    consumer: StreamConsumer<KafkaSourceContext>,
    decoder: Decoder,
    shutdown: ShutdownSignal,
    out: SourceSender,
    acknowledgements: bool,
    #[cfg(test)] eof: bool,
    log_namespace: LogNamespace,
) -> Result<(), ()> {
    let consumer = Arc::new(consumer);

    consumer
        .context()
        .consumer
        .set(Arc::downgrade(&consumer))
        .expect("Error setting up consumer context.");

    let mut ack_task = None;
    if acknowledgements {
        let consumer = Arc::clone(&consumer);
        let (callback_sender, callback_rx) = mpsc::unbounded_channel();

        consumer
            .context()
            .callbacks
            .set(callback_sender)
            .expect("Error setting up consumer callback channel.");

        ack_task = Some(handle_acks(
            consumer,
            callback_rx,
            config.session_timeout_ms,
        ));
    }

    let msg_consumer = Arc::clone(&consumer);
    let span = info_span!("kafka_source");
    let msg_task = tokio::task::spawn_blocking(move || {
        let _enter = span.enter();
        handle_messages(
            config,
            msg_consumer,
            decoder,
            shutdown,
            out,
            log_namespace,
            #[cfg(test)]
            eof,
        );
    });

    if let Some(ack_task) = ack_task {
        _ = tokio::join!(msg_task, ack_task);
    } else {
        _ = tokio::join!(msg_task);
    }

    consumer.context().commit_consumer_state();

    Ok(())
}

fn handle_acks(
    consumer: Arc<StreamConsumer<KafkaSourceContext>>,
    mut callbacks: UnboundedReceiver<KafkaCallback>,
    max_drain_ms: Duration,
) -> JoinHandle<()> {
    let mut drain_signal: Option<SyncSender<()>> = None;

    fn handle_ack(
        consumer: &Arc<StreamConsumer<KafkaSourceContext>>,
        status: BatchStatus,
        entry: FinalizerEntry,
    ) {
        if status == BatchStatus::Delivered {
            if let Err(error) = consumer.store_offset(&entry.topic, entry.partition, entry.offset) {
                emit!(KafkaOffsetUpdateError { error });
            }
        }
    }

    async fn revoke_timeout(t: Duration) {
        tokio::time::sleep(t).await;
    }

    // Wrap acks for each partition in this enum, so that we can let the receiver
    // know when it has seen the last one for the current assignment of this partition
    enum ForwardedAck {
        Entry(BatchStatus, FinalizerEntry),
        Drained(TopicPartition),
    }

    struct KafkaPartitionState {
        /// The sender for ack forwarding tasks to use
        ack_tx: Option<Sender<ForwardedAck>>,

        /// Tasks forwarding acknowledgement entries for each partition to a main channel.
        /// There will be one task per assigned partition, and this allows us, during rebalances,
        /// to precisely manage when acks for any revoked partitions are complete
        ack_forwarders: tokio::task::JoinSet<TopicPartition>,

        /// Abort handles for each forwarding task, indexed by the (topic, partition) pair. This
        /// allows for precise task cancellation when a partition is revoked but acks can't be processed
        /// before a timeout; only pending acks for revoked partitions will be cancelled/dropped.
        abort_handles: HashMap<TopicPartition, tokio::task::AbortHandle>,

        /// The Set of partitions expected to drain during a shutdown or rebalance that revokes partitions
        expect_drain: HashSet<TopicPartition>,

        /// The set of partitions we have observed and stored the final acknowledgement for. Ack streams
        /// can complete before we get a rebalance callback, so "observed complete" (based on seeing the end of the stream)
        /// and "expect to complete" (based on seeing a rebalance callback with revoked partition info) are tracked separately.
        observed_drain: HashSet<TopicPartition>,
    }
    impl KafkaPartitionState {
        fn new() -> (Self, Receiver<ForwardedAck>) {
            let (ack_tx, all_acks) = mpsc::channel(16); // arbitrary size 16
            let state = KafkaPartitionState {
                ack_tx: Some(ack_tx),
                ack_forwarders: tokio::task::JoinSet::new(),
                abort_handles: HashMap::new(),
                expect_drain: HashSet::new(),
                observed_drain: HashSet::new(),
            };
            (state, all_acks)
        }
        pub fn assign_partition(&mut self, tp: TopicPartition, acks: AckStream) {
            if let Some(ref ack_tx) = self.ack_tx {
                self.abort_handles.insert(
                    tp.clone(),
                    self.ack_forwarders
                        .spawn(forward_acks(tp, acks, ack_tx.clone())),
                );
            }
        }
        pub fn revoke_partition(&mut self, tp: TopicPartition) {
            self.expect_drain.insert(tp);
        }

        pub fn forwarder_complete(&mut self, tp: &TopicPartition) {
            self.abort_handles.remove(tp);
        }
        pub fn has_forwarders(&self) -> bool {
            !self.ack_forwarders.is_empty()
        }
        pub fn abort_pending_forwarders(&mut self) {
            for tp in self.expect_drain.drain() {
                // If the handle isn't here anymore (None case) it just means the task already completed
                if let Some(handle) = self.abort_handles.remove(&tp) {
                    handle.abort();
                }
            }
        }
        pub fn observed_last_ack(&mut self, tp: TopicPartition) {
            self.observed_drain.insert(tp);
        }

        pub fn is_drain_complete(&self) -> bool {
            self.observed_drain == self.expect_drain
        }

        pub fn clear(&mut self) {
            self.expect_drain.clear();
            self.observed_drain.clear();
        }

        pub fn close(&mut self) {
            let _ = self.ack_tx.take();
        }
    }

    async fn forward_acks(
        tp: TopicPartition,
        mut acks: AckStream,
        forward_to: mpsc::Sender<ForwardedAck>,
    ) -> TopicPartition {
        while let Some((status, entry)) = acks.next().await {
            if let Err(e) = forward_to.send(ForwardedAck::Entry(status, entry)).await {
                warn!("Error sending to main ack task: {}", e);
            }
        }
        let _ = forward_to.send(ForwardedAck::Drained(tp.clone())).await;
        tp
    }

    tokio::spawn(async move {
        /*
        Ok how does this work and where are the nuances in here?
        We have:
        - the consumer task: a task talking to Kafka, reading messages and getting rebalance notifications, etc.
            - Finalizer entries are added to partition-specific channels from this task
            - During rebalances, send notifications about assigned/revoked partitions
            - When a partition is revoked, or the client is shutting down, close the
              finalizer channels so they can be drained by the ack task. Wait on a channel coordinating with
              the acknowledgement task to avoid proceeding with rebalance before pending offsets are written
          Nuance: the kafka task runs blocking code in the rebalance callbacks, so must live
          on a separate thread from acknowledgement handling, otherwise everything deadlocks

        - the acknowledgement task:
            - Nuance: rebalancing may revoke a subset of our partitions, depending on the strategy-- to handle this correctly
              we use a finalizer stream per partition
            - When a partition is assigned, spawn a task dedicated to reading the finalizer channel. Tasks and channels
              perform significantly better than a tokio StreamMap when dealing with more than a few partitions.
            - As finalizers become ready, forward them to the main ack task to be stored.
            - When a finalizer channel closes, the final message in the forwarding channel
              is a marker indicating the partition is drained. Nuance: acks for the partition
              are considered drained when this marker is _processed by the main ack task_, not
              when the forwarding task ends
              Additional nuance: finalizer channels can end before we even get a rebalance notification!
            - During a rebalance, we track expected and observed drain markers, as well as a timeout.
              As soon as the expected partition streams are drained, or the timeout is reached, signal back
              to the consumer task to proceed with rebalancing.
              In the case of a timeout, drop any remaining acks on revoked partitions.
            - Rebalance bookkeeping is done by the ForwardedAck enum and KafkaPartitionState struct.

        */

        let mut drain_deadline = tokio::spawn(revoke_timeout(max_drain_ms));
        let (mut partition_state, mut all_acks) = KafkaPartitionState::new();

        loop {
            tokio::select! {
                Some(callback) = callbacks.recv() => match callback {
                    KafkaCallback::PartitionsAssigned(mut assigned_streams) => {
                        for (tp, acks) in assigned_streams.drain(0..) {
                            partition_state.assign_partition(tp, acks);
                        }
                    },
                    KafkaCallback::PartitionsRevoked(mut revoked_partitions, drain) => {
                        drain_deadline = tokio::spawn(revoke_timeout(max_drain_ms));

                        for tp in revoked_partitions.drain(0..) {
                            partition_state.revoke_partition(tp);
                        }

                        if partition_state.is_drain_complete() {
                            partition_state.clear();
                            drop(drain);
                        } else if drain_signal.replace(drain).is_some() {
                            unreachable!("Concurrent rebalance callbacks should not be possible.");
                        }
                    },
                    KafkaCallback::ShuttingDown(drain) => {
                        // Shutting down is just like a full assignment revoke, but we also close the ack senders and callback
                        // channels, since we don't expect additional assignments or rebalances
                        if let Ok(tpl) = consumer.assignment() {
                            tpl.elements()
                              .iter()
                              .for_each(|el| {
                                partition_state.revoke_partition((el.topic().into(), el.partition()));
                            });
                        }

                        drain_deadline = tokio::spawn(revoke_timeout(max_drain_ms));
                        if partition_state.is_drain_complete() {
                            partition_state.clear();
                            drop(drain);
                        } else if drain_signal.replace(drain).is_some() {
                            unreachable!("Shutdown callback happened somehow during an ongoing rebalance...?")
                        }

                        // No new partitions will be assigned, so drop our handle to the ack sender and close the callback channel
                        partition_state.close();
                        callbacks.close();
                    },
                },

                // As partition-specific sender tasks complete (revoked partitions
                // during a rebalance, shutdown, or eof during tests), handle the results here
                Some(Ok(finished_partition)) = partition_state.ack_forwarders.join_next(), if partition_state.has_forwarders() => {
                    partition_state.forwarder_complete(&finished_partition);
                },

                _ = &mut drain_deadline, if drain_signal.is_some() => {
                    debug!("Acknowledgement drain deadline reached. Dropping any pending ack streams for revoked partitions.");
                    partition_state.abort_pending_forwarders();
                    partition_state.clear();

                    if let Err(e) = drain_signal.take().unwrap().send(()) {
                        warn!("Error sending to drain signal: {}.", e);
                    }
                },

                Some(entry) = all_acks.recv() => match entry {
                    ForwardedAck::Drained(tp) => {
                        partition_state.observed_last_ack(tp);

                        if drain_signal.is_some() {
                            _ = drain_signal.as_ref().map(|sig| _ = sig.send(()) );
                        }

                        if partition_state.is_drain_complete() {
                            partition_state.clear();
                            drain_signal.take();
                        }
                    },
                    ForwardedAck::Entry(delivery_status, entry) => {
                        handle_ack(&consumer, delivery_status, entry);
                    },
                },

                // acks and callbacks are all done
                else => {
                    break
                }
            }
        }
    })
}

fn handle_messages(
    config: KafkaSourceConfig,
    consumer: Arc<StreamConsumer<KafkaSourceContext>>,
    decoder: Decoder,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
    log_namespace: LogNamespace,
    #[cfg(test)] eof: bool,
) {
    #[cfg(test)]
    let mut eof_partitions = std::collections::HashSet::new();

    Handle::current().block_on(async move {
        let mut stream = consumer.stream();
        loop {
            tokio::select! {
                _ = &mut shutdown => {
                    consumer.context().shutdown();
                    break
                },

                message = stream.next() => match message {
                    None => unreachable!("MessageStream never returns Ready(None)"),
                    Some(Err(error)) => match error {
                        #[cfg(test)]
                        rdkafka::error::KafkaError::PartitionEOF(partition) if eof => {
                            // NB this is not production ready EOF detection! Hence cfg(test) on this branch
                            // Used only in tests when we can be certain only one topic is being consumed,
                            // and new messages are not written after EOF is seen
                            // Also: RdKafka only tells us the partition, so
                            // we are assuming single-topic consumers when using this.
                            // Also also: due to rebalances, we might get notified about an EOF partition
                            // more than once, so we use a Set to exit once we've seen EOF on all currently-assigned partitions
                            eof_partitions.insert(partition);
                            if let Ok(assignment) = consumer.assignment() {
                                // All currently assigned partitions have reached EOF
                                if assignment.elements().iter().all(|tp| eof_partitions.contains(&tp.partition())) {
                                    consumer.context().shutdown();
                                    break
                                }
                            }
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

                        parse_message(msg, decoder.clone(), config.keys(), &mut out, &consumer, log_namespace).await;
                    }
                },
            }
        }
    });
}

async fn parse_message(
    msg: BorrowedMessage<'_>,
    decoder: Decoder,
    keys: Keys<'_>,
    out: &mut SourceSender,
    consumer: &Arc<StreamConsumer<KafkaSourceContext>>,
    log_namespace: LogNamespace,
) {
    let context = consumer.context();

    if let Some((count, mut stream)) = parse_stream(&msg, decoder, keys, log_namespace) {
        if context.acknowledgements {
            let (batch, receiver) = BatchNotifier::new_with_receiver();
            let mut stream = stream.map(|event| event.with_batch_notifier(&batch));
            match out.send_event_stream(&mut stream).await {
                Err(_) => {
                    emit!(StreamClosedError { count });
                }
                Ok(_) => {
                    // Drop stream to avoid borrowing `msg`: "[...] borrow might be used
                    // here, when `stream` is dropped and runs the destructor [...]".
                    drop(stream);
                    context.add_finalizer_entry(msg.into(), receiver);
                }
            }
        } else {
            match out.send_event_stream(&mut stream).await {
                Err(_) => {
                    emit!(StreamClosedError { count });
                }
                Ok(_) => {
                    if let Err(error) =
                        consumer.store_offset(msg.topic(), msg.partition(), msg.offset())
                    {
                        emit!(KafkaOffsetUpdateError { error });
                    }
                }
            }
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

#[derive(Clone, Debug)]
struct Keys<'a> {
    timestamp: Option<OwnedValuePath>,
    key_field: &'a Option<OwnedValuePath>,
    topic: &'a Option<OwnedValuePath>,
    partition: &'a Option<OwnedValuePath>,
    offset: &'a Option<OwnedValuePath>,
    headers: &'a Option<OwnedValuePath>,
}

impl<'a> Keys<'a> {
    fn from(schema: &'a LogSchema, config: &'a KafkaSourceConfig) -> Self {
        Self {
            timestamp: schema.timestamp_key().cloned(),
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
) -> crate::Result<StreamConsumer<KafkaSourceContext>> {
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

    let consumer = client_config
        .create_with_context::<_, StreamConsumer<_>>(KafkaSourceContext::new(
            config.metrics.topic_lag_metric,
            acknowledgements,
        ))
        .context(KafkaCreateSnafu)?;
    let topics: Vec<&str> = config.topics.iter().map(|s| s.as_str()).collect();
    consumer.subscribe(&topics).context(KafkaSubscribeSnafu)?;

    Ok(consumer)
}

type TopicPartition = (String, i32);
type AckStream = BoxStream<'static, (BatchStatus, FinalizerEntry)>;

enum KafkaCallback {
    PartitionsAssigned(Vec<(TopicPartition, AckStream)>),
    PartitionsRevoked(Vec<TopicPartition>, SyncSender<()>),
    ShuttingDown(SyncSender<()>),
}

#[derive(Default)]
struct KafkaSourceContext {
    acknowledgements: bool,
    stats: kafka::KafkaStatisticsContext,

    /// A callback channel used to coordinate between the main consumer task and the acknowledgement task
    callbacks: OnceCell<UnboundedSender<KafkaCallback>>,

    /// Use a finalizer stream for each partition being consumed, so that when a partition
    /// is revoked during a consumer rebalance, acknowledgements can be prioritized (and
    /// dropped after a time limit) without affecting acks for retained partitions.
    finalizers: RwLock<HashMap<TopicPartition, OrderedFinalizer<FinalizerEntry>>>,

    /// A weak reference to the consumer, so that we can commit offsets during a rebalance operation
    consumer: OnceCell<Weak<StreamConsumer<KafkaSourceContext>>>,
}

impl KafkaSourceContext {
    fn new(expose_lag_metrics: bool, acknowledgements: bool) -> Self {
        Self {
            stats: kafka::KafkaStatisticsContext { expose_lag_metrics },
            acknowledgements,
            ..Default::default()
        }
    }

    pub fn add_finalizer_entry(&self, mut entry: FinalizerEntry, recv: BatchStatusReceiver) {
        if let Ok(fin) = self.finalizers.read() {
            let key = (entry.topic, entry.partition);
            if let Some(entries) = fin.get(&key) {
                entry.topic = key.0; // Slightly awkward, but avoids cloning the topic string for every entry added
                entries.add(entry, recv);
            }
        }
    }

    pub fn shutdown(&self) {
        if let Ok(mut fin) = self.finalizers.write() {
            fin.clear();
        }

        if let Some(tx) = self.callbacks.get() {
            let (send, rendezvous) = sync_channel(0);
            if tx.send(KafkaCallback::ShuttingDown(send)).is_ok() {
                while rendezvous.recv().is_ok() {
                    self.commit_consumer_state();
                }
            }
        }
    }

    fn add_finalizerset(&self, key: TopicPartition) -> Option<(TopicPartition, AckStream)> {
        if let Ok(fin) = self.finalizers.read() {
            if fin.contains_key(&key) {
                trace!("Finalizer entry already exists for {}:{}.", key.0, key.1);
                return None;
            }
        }

        let (finalizer, ack_stream) = OrderedFinalizer::<FinalizerEntry>::new(None);
        if let Ok(mut fin) = self.finalizers.write() {
            fin.insert(key.clone(), finalizer);
            Some((key, ack_stream))
        } else {
            // error getting the RwLock, i.e. the lock is poisoned (!!)
            None
        }
    }

    fn rm_finalizerset(&self, key: &TopicPartition) {
        if let Ok(mut fin) = self.finalizers.write() {
            fin.remove(key);
        }
    }

    fn commit_consumer_state(&self) {
        if let Some(w) = self.consumer.get() {
            if let Some(consumer) = w.upgrade() {
                match consumer.commit_consumer_state(CommitMode::Sync) {
                    Ok(_) | Err(KafkaError::ConsumerCommit(RDKafkaErrorCode::NoOffset)) => {
                        /* Success, or nothing to do \0/ */
                    }
                    Err(error) => emit!(KafkaOffsetUpdateError { error }),
                }
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
        if let Some(tx) = self.callbacks.get() {
            match rebalance {
                Rebalance::Assign(tpl) => {
                    // Partitions are being assigned to this consumer!
                    // 1. Create a finalizer set for the new partitions
                    // 2. `self` keeps a reference to the sender - this is the entry point for pending acks on this partition
                    // 3. Hand a reference to the receiver stream to the acknowledgement task via callback channel
                    let ack_streams: Vec<(TopicPartition, AckStream)> = tpl
                        .elements()
                        .iter()
                        .filter_map(|el| self.add_finalizerset((el.topic().into(), el.partition())))
                        .collect();

                    trace!("Partition(s) assigned: {}", ack_streams.len());
                    if !ack_streams.is_empty() {
                        let _ = tx.send(KafkaCallback::PartitionsAssigned(ack_streams));
                    }
                }
                Rebalance::Revoke(tpl) => {
                    // Partitions are being revoked from this consumer!
                    // 1. Close the sending side for new acknowledgement entries.
                    // 2. Notify the acknowledgement task, and provide a rendezvous channel; wait for that channel to close
                    //    to indicate when acks for revoked partitions are drained.
                    // 3. Commit consumer offsets and return, allowing the rebalance to complete
                    let revoked: Vec<TopicPartition> = tpl
                        .elements()
                        .iter()
                        .map(|el| {
                            let key = (el.topic().into(), el.partition());
                            self.rm_finalizerset(&key);
                            key
                        })
                        .collect();

                    trace!("Partition(s) revoked: {}", revoked.len());
                    if !revoked.is_empty() {
                        let (send, rendezvous) = sync_channel(0);
                        // The ack task will signal on this channel when it has drained a revoked partition
                        // and will close the channel when it has drained all revoked partitions,
                        // or when it times out, to prevent the consumer being kicked from the group.
                        // This send will return Err if the ack task has already exited; in that case we
                        // proceed without waiting
                        if tx
                            .send(KafkaCallback::PartitionsRevoked(revoked, send))
                            .is_ok()
                        {
                            while rendezvous.recv().is_ok() {
                                self.commit_consumer_state();
                            }
                        }

                        self.commit_consumer_state();
                    }
                }
                Rebalance::Error(message) => {
                    error!("Error during rebalance: {}.", message);
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use lookup::OwnedTargetPath;
    use vector_core::schema::Definition;

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
            librdkafka_options: librdkafka_options,
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
    use vrl::value;

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
            .unwrap_or_else(|_| format!("test-topic-{}", random_string(10)).into())
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

        let writes = (0..count)
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
                producer.send(record, Timeout::Never).await
            })
            .collect::<Vec<_>>();

        for res in writes {
            if let Err(error) = res.await {
                panic!("Cannot send event to Kafka: {:?}", error);
            }
        }

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
                    event.as_log()[log_schema().timestamp_key().unwrap().to_string()],
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
        let config = make_config(&topic, &group_id, LogNamespace::Legacy, None);
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
        eof: bool,
        log_namespace: LogNamespace,
    ) -> (Trigger, Tripwire) {
        let (trigger_shutdown, shutdown, shutdown_done) = ShutdownSignal::new_wired();
        let consumer = create_consumer(&config, acknowledgements).unwrap();

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
            spawn_kafka(tx, config, true, true, LogNamespace::Legacy);
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
            .unwrap_or_else(|_| "100".into())
            .parse()
            .expect("Number of messages to send to kafka.");
        let expect_count: usize = std::env::var("KAFKA_EXPECT_COUNT")
            .unwrap_or_else(|_| format!("{}", send_count).into())
            .parse()
            .expect("Number of messages to expect consumers to process.");
        let delay_ms: u64 = std::env::var("KAFKA_SHUTDOWN_DELAY")
            .unwrap_or_else(|_| "3000".into())
            .parse()
            .expect("Number of milliseconds before shutting down first consumer.");

        let (topic, group_id, _) = send_to_test_topic(1, send_count).await;

        // 2. Run the kafka source to read some of the events
        // 3. Send a shutdown signal (at some point before all events are read)
        let mut opts = HashMap::new();
        // Set options to get partition EOF notifications, and fetch data in small/configurable size chunks
        opts.insert("enable.partition.eof".into(), "true".into());
        opts.insert("fetch.message.max.bytes".into(), kafka_max_bytes().into());
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
            "First batch of events is non-zero (increase KAFKA_SHUTDOWN_DELAY?)"
        );
        assert_ne!(events2.len(), 0, "Second batch of events is non-zero (decrease KAFKA_SHUTDOWN_DELAY or increase KAFKA_SEND_COUNT?) ");
        assert_eq!(total, expect_count);
    }

    async fn consume_with_rebalance(rebalance_strategy: String) {
        // 1. Send N events (if running against a pre-populated kafka topic, use send_count=0 and expect_count=expected number of messages; otherwise just set send_count)
        let send_count: usize = std::env::var("KAFKA_SEND_COUNT")
            .unwrap_or_else(|_| "100".into())
            .parse()
            .expect("Number of messages to send to kafka.");
        let expect_count: usize = std::env::var("KAFKA_EXPECT_COUNT")
            .unwrap_or_else(|_| format!("{}", send_count).into())
            .parse()
            .expect("Number of messages to expect consumers to process.");
        let delay_ms: u64 = std::env::var("KAFKA_CONSUMER_DELAY")
            .unwrap_or_else(|_| "3000".into())
            .parse()
            .expect("Number of milliseconds before shutting down first consumer.");

        let (topic, group_id, _) = send_to_test_topic(2, send_count).await;
        println!("Topic: {}", &topic);
        println!("Consumer group.id: {}", &group_id);

        // 2. Run the kafka source to read some of the events
        // 3. Start 2nd & 3rd consumers using the same group.id, triggering rebalance events
        let mut kafka_options = HashMap::new();
        kafka_options.insert("enable.partition.eof".into(), "true".into());
        kafka_options.insert("fetch.message.max.bytes".into(), kafka_max_bytes().into());
        kafka_options.insert("partition.assignment.strategy".into(), rebalance_strategy);
        let config1 = make_config(
            &topic,
            &group_id,
            LogNamespace::Legacy,
            Some(kafka_options.clone()),
        );
        let config2 = make_config(
            &topic,
            &group_id,
            LogNamespace::Legacy,
            Some(kafka_options.clone()),
        );
        let config3 = make_config(
            &topic,
            &group_id,
            LogNamespace::Legacy,
            Some(kafka_options.clone()),
        );
        let config4 = make_config(&topic, &group_id, LogNamespace::Legacy, Some(kafka_options));

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
            "Second batch of events is non-zero (decrease delay or increase KAFKA_SEND_COUNT?) "
        );
        assert_ne!(
            events3.len(),
            0,
            "Third batch of events is non-zero (decrease delay or increase KAFKA_SEND_COUNT?) "
        );
        assert_eq!(
            unconsumed.len(),
            0,
            "The first set of consumer should consume and ack all messages."
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
