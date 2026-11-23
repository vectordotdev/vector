use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use futures::{StreamExt, future::join_all, stream::BoxStream};
use iggy::prelude::{
    Checkpoint, Client, IggyClient, IggyConsumer, IggyError, PollingKind, PollingStrategy,
    ReadError, ReceivedMessage,
};
use tokio::time::{MissedTickBehavior, interval, sleep, sleep_until};
use tokio_stream::StreamMap;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{DecoderFramedRead, decoding::StreamDecodingError},
    config::{LegacyKey, LogNamespace},
    finalizer::OrderedFinalizer,
    internal_event::emit,
    lookup::{OwnedValuePath, owned_value_path},
};

use crate::{
    SourceSender,
    codecs::Decoder,
    common::{
        backoff::ExponentialBackoff,
        iggy::{is_unauthenticated, login_if_http, logs_in_over_http},
    },
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
    internal_events::{
        IggyBytesReceived, IggyConsumerInitError, IggyConsumerStreamEndedError, IggyEventsReceived,
        IggyOffsetCommitted, IggyOffsetPolled, IggyOffsetUpdateError, IggyReadError,
        StreamClosedError,
    },
    shutdown::ShutdownSignal,
    sources::iggy::config::IggySourceConfig,
};

/// How many times a permanently rejected offset is replayed before the source
/// gives up and commits past it, so a single undeliverable message cannot
/// stall a partition for the lifetime of its assignment. Transient failures
/// (`BatchStatus::Errored`) are never skipped: dropping them would break the
/// source's at-least-once guarantee.
const MAX_REDELIVERY_ATTEMPTS: u32 = 3;

/// Polled messages allowed to wait for a final acknowledgement before the
/// source stops polling. An ordered finalizer yields acknowledgements in order,
/// so one stalled acknowledgement holds every later one on that partition; an
/// unbounded read would grow that queue for as long as the stall lasts.
///
/// The cap covers every assigned partition: the SDK polls the consumer group
/// through one multiplexed stream and offers no per-partition pause, so a
/// stalled partition eventually pauses them all. It bounds memory rather
/// than pacing one partition.
const MAX_IN_FLIGHT: usize = 10_000;

/// Per-partition acknowledgement bookkeeping. The "fence" is the lowest
/// rejected offset; `pending` is the highest offset that can be safely
/// committed to the server without skipping a rejected message on restart.
/// A rejection is replayed from the broker by seeking the partition back to
/// the fence; the fence clears once that replay is delivered.
#[derive(Default)]
struct PartitionState {
    pending: Option<u64>,
    /// Highest offset the server confirmed; `None` until the first store, so a
    /// pending offset 0 still counts as lag.
    committed: Option<u64>,
    max_delivered: Option<u64>,
    /// Lowest offsets whose final acknowledgement was not `Delivered`, with
    /// `true` when the failure was transient. `pending` never advances past
    /// the minimum, but every rejection stays: replaying the fence can deliver
    /// it while a higher rejection is still undelivered, and clearing the
    /// whole fence would let a commit skip that one. Transient failures are
    /// replayed indefinitely; permanent ones are skipped after
    /// `MAX_REDELIVERY_ATTEMPTS`.
    rejected: BTreeMap<u64, bool>,
    /// Offsets the broker could not decode or decrypt and a seek moved past;
    /// a late acknowledgement for one must not re-fence the partition.
    /// Earlier offsets stay actionable: they gate the checkpoint.
    skipped: BTreeSet<u64>,
    /// Replay attempts already made for the current fence.
    retry_attempts: u32,
    /// Polled messages whose final acknowledgement has not arrived yet.
    in_flight: usize,
    /// Offsets of those messages, counted so overlapping replays settle
    /// correctly. The lowest unsettled offset caps the checkpoint, so a skip
    /// cannot commit past an earlier message that has not been acknowledged.
    in_flight_offsets: BTreeMap<u64, usize>,
    /// The checkpoint each polled offset arrived with, kept until its offset is
    /// committed. A commit of the checkpoint carries the partition incarnation
    /// and owner that delivered the message, so the SDK refuses it after a
    /// rebalance or a recreated partition instead of storing the offset under
    /// the new context.
    checkpoints: BTreeMap<u64, Checkpoint>,
}

/// What the source should do about an outstanding rejection.
enum RetryOutcome {
    /// Seek the partition back to this offset so the message is replayed.
    Seek(u64),
    /// Replay kept failing; the offset is skipped so commits can resume.
    Skipped(u64),
    /// No rejection is outstanding.
    Idle,
}

/// A seek the loop must issue before polling again: recovery of a stopped
/// partition resumes after the last polled message (`next`), while a rejected
/// offset is replayed from that offset.
struct PendingSeek {
    at: tokio::time::Instant,
    strategy: PollingStrategy,
    /// The seek recovers from a lost partition history. It starts a new SDK
    /// generation of a different history, so the offsets, fences, and
    /// finalizers tracked for the old one are stale and must be dropped.
    resets_history: bool,
}

impl PartitionState {
    fn mark_in_flight(&mut self, offset: u64, checkpoint: Checkpoint) {
        self.in_flight += 1;
        *self.in_flight_offsets.entry(offset).or_default() += 1;
        self.checkpoints.insert(offset, checkpoint);
    }

    fn settle(&mut self, offset: u64) {
        self.in_flight = self.in_flight.saturating_sub(1);
        match self.in_flight_offsets.get_mut(&offset) {
            Some(count) if *count > 1 => *count -= 1,
            Some(_) => {
                self.in_flight_offsets.remove(&offset);
            }
            None => {}
        }
    }

    const fn is_settled(&self) -> bool {
        self.in_flight == 0
    }

    /// Record a delivered acknowledgement. Returns `true` when this delivery
    /// cleared the fence, i.e. a rejected offset was replayed successfully.
    fn record_delivered(&mut self, offset: u64) -> bool {
        self.max_delivered = Some(self.max_delivered.map_or(offset, |m| m.max(offset)));
        let recovered = self.fence() == Some(offset);
        self.rejected.remove(&offset);
        if recovered {
            self.retry_attempts = 0;
        }
        self.recompute_pending();
        recovered
    }

    /// Record a non-Delivered acknowledgement for `offset`. `transient` is
    /// true for `BatchStatus::Errored`, where another replay may succeed, and
    /// false for a permanent `Rejected`. Returns `true` if the fence moved.
    fn record_rejection(&mut self, offset: u64, transient: bool) -> bool {
        if self.skipped.remove(&offset) {
            // Undecodable and already moved past: replaying fails the same
            // way forever. Lower offsets stay actionable so their failure
            // still holds the checkpoint.
            self.recompute_pending();
            return false;
        }
        let prev_fence = self.fence();
        self.rejected.insert(offset, transient);
        let fence_changed = prev_fence != self.fence();
        if fence_changed {
            // A new target restarts the replay budget.
            self.retry_attempts = 0;
        }
        self.recompute_pending();
        fence_changed
    }

    /// Decide what to do about the outstanding rejection, counting an attempt
    /// against a permanent rejection's budget.
    fn retry(&mut self) -> RetryOutcome {
        let Some((fence, transient)) = self.fence_entry() else {
            return RetryOutcome::Idle;
        };
        self.retry_attempts = self.retry_attempts.saturating_add(1);
        if !transient && self.retry_attempts > MAX_REDELIVERY_ATTEMPTS {
            // Commit past the poison offset so later deliveries can be
            // checkpointed; the skipped event was already dropped downstream.
            self.rejected.remove(&fence);
            self.max_delivered = Some(self.max_delivered.map_or(fence, |m| m.max(fence)));
            self.retry_attempts = 0;
            self.recompute_pending();
            RetryOutcome::Skipped(fence)
        } else {
            RetryOutcome::Seek(fence)
        }
    }

    /// Mark the record the broker cannot decode as skipped so its late
    /// acknowledgement cannot re-fence the partition, and drop fences the
    /// seek passed. `in_flight_offsets` still caps the checkpoint below any
    /// earlier message that has not settled.
    fn skip_through(&mut self, offset: u64) {
        // A marker only suppresses its own offset's acknowledgement, and only
        // a message in flight can still acknowledge, so parked markers would
        // otherwise accumulate for the lifetime of the assignment.
        self.skipped
            .retain(|skipped| self.in_flight_offsets.contains_key(skipped));
        self.skipped.insert(offset);
        self.rejected.retain(|&rejected, _| rejected > offset);
        self.max_delivered = Some(self.max_delivered.map_or(offset, |m| m.max(offset)));
        self.retry_attempts = 0;
        self.recompute_pending();
    }

    fn fence_entry(&self) -> Option<(u64, bool)> {
        self.rejected
            .first_key_value()
            .map(|(&offset, &transient)| (offset, transient))
    }

    fn fence(&self) -> Option<u64> {
        self.rejected.keys().next().copied()
    }

    fn recompute_pending(&mut self) {
        let Some(max) = self.max_delivered else {
            return;
        };
        // A skip or poison-message skip can raise `max_delivered` above
        // messages that were polled but have not settled. Capping at the
        // lowest unsettled offset keeps a restart from resuming past them.
        let max = match self.in_flight_offsets.first_key_value() {
            Some((&0, _)) => {
                self.pending = None;
                return;
            }
            Some((&lowest, _)) => max.min(lowest - 1),
            None => max,
        };
        self.pending = match self.fence() {
            // The very first message (offset 0) was rejected; nothing can be
            // committed safely.
            Some(0) => None,
            Some(f) => Some(max.min(f - 1)),
            None => Some(max),
        };
    }

    /// Take the safe commit offset together with the checkpoint of that
    /// offset, if a delivered message supplied one. Skips raise `pending` past
    /// the last delivered message, so a skip-only commit has no checkpoint.
    fn take_pending(&mut self) -> Option<(u64, Option<Checkpoint>)> {
        let pending = self.pending.take()?;
        Some((pending, self.checkpoints.get(&pending).copied()))
    }

    fn committed_through(&mut self, offset: u64) {
        self.committed = Some(offset);
        self.checkpoints.retain(|&polled, _| polled > offset);
    }

    /// How far the highest safely-committable offset is ahead of what the
    /// server already knows. Drives the eager-commit threshold.
    fn lag(&self) -> u64 {
        self.pending.map_or(0, |p| match self.committed {
            Some(committed) => p.saturating_sub(committed),
            None => p.saturating_add(1),
        })
    }
}

/// Whether a generation's first message is the one a seek this source issued
/// reads first. A seek resumes at its target, so the message at that offset
/// opens the generation it started; a generation that opens anywhere else
/// belongs to a new assignment, whose stale fence and finalizers must be
/// dropped. A continuation carries no offset to compare against, so it keeps
/// the state it was issued to protect.
fn seek_started(seek: Option<PollingStrategy>, offset: u64) -> bool {
    seek.is_some_and(|seek| match seek.kind {
        PollingKind::Offset => seek.value == offset,
        _ => true,
    })
}

/// Whether the acknowledgement backlog reached the cap that pauses polling.
fn at_capacity(partitions: &HashMap<u32, PartitionState>) -> bool {
    partitions
        .values()
        .map(|state| state.in_flight)
        .sum::<usize>()
        >= MAX_IN_FLIGHT
}

/// The partition that a read error left stopped, if the SDK will not read it
/// again without a seek.
fn stopped_partition(error: &ReadError) -> Option<u32> {
    let partition_id = error.partition_id?;
    (error.is_lasting_refusal() || matches!(error.error, IggyError::HistoryUnavailable))
        .then_some(partition_id)
}

/// Forget the acknowledgement state tied to a partition: a reassignment or a
/// lost history invalidates all of it. The generation map keeps the current
/// generation so the next message is not treated as another change.
fn reset_partition(
    partition_id: u32,
    partitions: &mut HashMap<u32, PartitionState>,
    finalizers: &mut HashMap<u32, OrderedFinalizer<u64>>,
    ack_streams: &mut StreamMap<u32, BoxStream<'static, (BatchStatus, u64)>>,
    last_settled_commits: &mut HashMap<u32, Instant>,
) {
    partitions.remove(&partition_id);
    finalizers.remove(&partition_id);
    ack_streams.remove(&partition_id);
    last_settled_commits.remove(&partition_id);
}

/// Queue a seek. A replay or skip never replaces a pending history reset: the
/// stale offsets it would seek to belong to the history that was lost.
fn queue_seek(
    pending: &mut HashMap<u32, PendingSeek>,
    partition_id: u32,
    strategy: PollingStrategy,
    at: tokio::time::Instant,
    resets_history: bool,
) {
    if !resets_history
        && pending
            .get(&partition_id)
            .is_some_and(|seek| seek.resets_history)
    {
        return;
    }
    pending.insert(
        partition_id,
        PendingSeek {
            at,
            strategy,
            resets_history,
        },
    );
}

fn record_ack(state: &mut PartitionState, partition_id: u32, status: BatchStatus, offset: u64) {
    // Acks settle in registration order, not offset order: a replay registers
    // a lower offset after later ones queued, so an acknowledgement can arrive
    // below `committed`. That is harmless: delivered offsets only raise the
    // maximum, and a skipped record's own acknowledgement must not re-fence.
    // Every acknowledgement settles exactly one registered message.
    state.settle(offset);
    match status {
        BatchStatus::Delivered => {
            if state.record_delivered(offset) {
                info!(
                    message = "Replayed message was delivered downstream; the consumer offset for this partition can advance again.",
                    partition_id, offset,
                );
            }
        }
        status => {
            // `Errored` is a transient delivery failure, so replaying can
            // still succeed and the offset must stay uncommitted until it
            // does. `Rejected` is permanent; a bounded replay budget keeps a
            // poison event from pinning the partition forever.
            let transient = matches!(status, BatchStatus::Errored);
            if state.record_rejection(offset, transient) {
                warn!(
                    message = "Message was not delivered downstream; replaying it from the broker before the consumer offset for this partition advances past it.",
                    partition_id,
                    offset,
                    ?status,
                );
            }
        }
    }
}

/// Store each partition's pending offset on the Iggy server. Stores run
/// concurrently so a slow partition cannot stall the commit tick. A delivered
/// message's checkpoint commits under the partition history that delivered it,
/// so the SDK drops a store that a rebalance or a recreated partition made
/// stale; a skipped offset has no checkpoint and is stored raw. A
/// `NotResolvedConsumer` or `ConsumerGroupPartitionNotOwned` error prunes the
/// partition's state after a rebalance, since its checkpoint can never be
/// stored again; other errors restore the pending offset so a later tick or
/// the shutdown commit retries it.
#[allow(
    clippy::too_many_arguments,
    reason = "One call keeps the commit, ack, and auth state together"
)]
async fn commit_offsets(
    consumer: &IggyConsumer,
    client: &IggyClient,
    url: &str,
    stream: &str,
    topic: &str,
    partitions: &mut HashMap<u32, PartitionState>,
    finalizers: &mut HashMap<u32, OrderedFinalizer<u64>>,
    ack_streams: &mut StreamMap<u32, BoxStream<'static, (BatchStatus, u64)>>,
) {
    let pending: Vec<(u32, u64, Option<Checkpoint>)> = partitions
        .iter_mut()
        .filter_map(|(&partition_id, state)| {
            state
                .take_pending()
                .map(|(offset, checkpoint)| (partition_id, offset, checkpoint))
        })
        .collect();

    if pending.is_empty() {
        return;
    }

    let results = join_all(pending.into_iter().map(
        |(partition_id, offset, checkpoint)| async move {
            let result = match checkpoint {
                Some(checkpoint) => consumer.commit(checkpoint).await,
                // Nothing was delivered at this offset: it is past a record the
                // broker could not decode or a permanently rejected event, so only
                // its offset is available to store.
                None => consumer.store_offset(offset, Some(partition_id)).await,
            };
            (partition_id, offset, result)
        },
    ))
    .await;

    for (partition_id, offset, result) in results {
        match result {
            Ok(()) => {
                if let Some(state) = partitions.get_mut(&partition_id) {
                    state.committed_through(offset);
                }
                emit(IggyOffsetCommitted {
                    stream,
                    topic,
                    partition: partition_id,
                    offset,
                });
            }
            Err(
                IggyError::NotResolvedConsumer(_) | IggyError::ConsumerGroupPartitionNotOwned(..),
            ) => {
                partitions.remove(&partition_id);
                finalizers.remove(&partition_id);
                ack_streams.remove(&partition_id);
                debug!(
                    message = "Partition no longer assigned to this consumer; dropping per-partition acknowledgement state.",
                    stream, topic, partition_id,
                );
            }
            Err(error) => {
                if let Some(state) = partitions.get_mut(&partition_id) {
                    state.pending = Some(offset);
                }
                let unauthenticated = is_unauthenticated(&error);
                emit(IggyOffsetUpdateError { error });
                if unauthenticated
                    && logs_in_over_http(url)
                    && let Err(error) = login_if_http(client, url).await
                {
                    emit(IggyOffsetUpdateError { error });
                }
            }
        }
    }
}

struct MessageMetadata<'a> {
    log_namespace: LogNamespace,
    stream: &'a str,
    topic: &'a str,
    stream_key: Option<&'a OwnedValuePath>,
    topic_key: Option<&'a OwnedValuePath>,
    partition_id_key: Option<&'a OwnedValuePath>,
    offset_key: Option<&'a OwnedValuePath>,
    stream_path: &'a OwnedValuePath,
    topic_path: &'a OwnedValuePath,
    partition_id_path: &'a OwnedValuePath,
    offset_path: &'a OwnedValuePath,
}

struct AckTracker<'a> {
    partitions: &'a mut HashMap<u32, PartitionState>,
    finalizers: &'a mut HashMap<u32, OrderedFinalizer<u64>>,
    ack_streams: &'a mut StreamMap<u32, BoxStream<'static, (BatchStatus, u64)>>,
}

impl AckTracker<'_> {
    fn register(
        &mut self,
        partition_id: u32,
        offset: u64,
        checkpoint: Checkpoint,
        receiver: BatchStatusReceiver,
    ) {
        let finalizer = self.finalizers.entry(partition_id).or_insert_with(|| {
            let (finalizer, stream) = OrderedFinalizer::<u64>::new(None);
            self.ack_streams.insert(partition_id, stream);
            finalizer
        });
        self.partitions
            .entry(partition_id)
            .or_default()
            .mark_in_flight(offset, checkpoint);
        finalizer.add(offset, receiver);
    }
}

/// Forward a polled message's events, registering it for acknowledgement when
/// enabled. `Err(())` means the downstream sender closed.
///
/// A non-continuable decode error leaves earlier frames of the same payload
/// with the status they earn downstream and does not touch the fence.
#[allow(clippy::too_many_lines, reason = "One frame-decode and ack path")]
async fn process_received_message(
    received: ReceivedMessage,
    checkpoint: Checkpoint,
    decoder: &Decoder,
    metadata: &MessageMetadata<'_>,
    acknowledgements: bool,
    tracker: &mut AckTracker<'_>,
    out: &mut SourceSender,
) -> Result<(), ()> {
    let payload = &received.message.payload;
    let partition_id = received.partition_id;
    let offset = received.message.header.offset;
    emit(IggyBytesReceived {
        byte_size: payload.len(),
        stream: metadata.stream,
        topic: metadata.topic,
        partition: partition_id,
    });
    emit(IggyOffsetPolled {
        stream: metadata.stream,
        topic: metadata.topic,
        partition: partition_id,
        message_offset: offset,
        current_offset: received.current_offset,
    });

    let (batch, status_receiver) = BatchNotifier::new_with_receiver();
    let mut framed = DecoderFramedRead::new(payload.as_ref(), decoder.clone());
    let mut channel_closed = false;
    let mut decode_failed = false;

    while let Some(next) = framed.next().await {
        match next {
            Ok((events, _byte_size)) => {
                let count = events.len();
                if count == 0 {
                    continue;
                }
                let byte_size = events.estimated_json_encoded_size_of();
                emit(IggyEventsReceived {
                    count,
                    byte_size,
                    stream: metadata.stream,
                    topic: metadata.topic,
                    partition: partition_id,
                });
                let now = Utc::now();

                let events = events.into_iter().map(|mut event| {
                    if let Event::Log(ref mut log) = event {
                        metadata
                            .log_namespace
                            .insert_standard_vector_source_metadata(
                                log,
                                IggySourceConfig::NAME,
                                now,
                            );
                        metadata.log_namespace.insert_source_metadata(
                            IggySourceConfig::NAME,
                            log,
                            metadata.stream_key.map(LegacyKey::InsertIfEmpty),
                            metadata.stream_path,
                            metadata.stream,
                        );
                        metadata.log_namespace.insert_source_metadata(
                            IggySourceConfig::NAME,
                            log,
                            metadata.topic_key.map(LegacyKey::InsertIfEmpty),
                            metadata.topic_path,
                            metadata.topic,
                        );
                        metadata.log_namespace.insert_source_metadata(
                            IggySourceConfig::NAME,
                            log,
                            metadata.partition_id_key.map(LegacyKey::InsertIfEmpty),
                            metadata.partition_id_path,
                            i64::from(partition_id),
                        );
                        metadata.log_namespace.insert_source_metadata(
                            IggySourceConfig::NAME,
                            log,
                            metadata.offset_key.map(LegacyKey::InsertIfEmpty),
                            metadata.offset_path,
                            i64::try_from(offset).unwrap_or(i64::MAX),
                        );
                    }
                    if acknowledgements {
                        event.with_batch_notifier(&batch)
                    } else {
                        event
                    }
                });

                if out.send_batch(events).await.is_err() {
                    emit(StreamClosedError { count });
                    channel_closed = true;
                    break;
                }
            }
            Err(error) => {
                if !error.can_continue() {
                    decode_failed = true;
                    break;
                }
            }
        }
    }

    // Dropping the notifier finalizes the batch once its events settle.
    // A decode failure drops it normally, so earlier frames keep the status
    // they earn downstream.
    drop(batch);

    if channel_closed {
        return Err(());
    }

    if decode_failed {
        warn!(
            message = "Iggy message payload could not be decoded; skipping offset to avoid permanently wedging the partition.",
            stream = metadata.stream,
            topic = metadata.topic,
            partition_id,
            offset,
        );
        if acknowledgements {
            // Earlier frames may still be in flight, so register through the
            // normal ack path and let the offset advance only after they
            // settle. An empty batch resolves as Delivered immediately. A
            // downstream rejection holds the fence until this offset is
            // replayed, matching the non-decode path.
            tracker.register(partition_id, offset, checkpoint, status_receiver);
        }
        // Without acknowledgements `CommitPolicy::OnRead` stored the end of the
        // polled batch when it was read, so the failed record is already behind
        // the checkpoint; storing its offset would move it back over records
        // that were delivered.
        return Ok(());
    }

    if acknowledgements {
        tracker.register(partition_id, offset, checkpoint, status_receiver);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[allow(
    clippy::too_many_lines,
    reason = "One poll loop keeps the ack, commit, and seek state together"
)]
pub(crate) async fn run_iggy_source(
    config: IggySourceConfig,
    decoder: Decoder,
    log_namespace: LogNamespace,
    acknowledgements: bool,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    // Initializing here rather than in `build` keeps a broker that is not up
    // yet, or a topic a sink in the same topology still has to create, from
    // failing topology construction outright.
    let mut init_backoff = ExponentialBackoff::from_millis(2)
        .factor(250)
        .max_delay(Duration::from_secs(30));
    let (keep_alive_client, mut consumer) = loop {
        match config.connect_and_init(acknowledgements).await {
            Ok(initialized) => break initialized,
            Err(error) => {
                emit(IggyConsumerInitError);
                warn!(
                    message = "Failed to initialize the Iggy consumer; retrying.",
                    %error,
                );
                let delay = init_backoff.next().unwrap_or(Duration::from_secs(30));
                tokio::select! {
                    _ = &mut shutdown => {
                        info!("Shutdown signal received while initializing the Iggy consumer.");
                        return Ok(());
                    }
                    () = sleep(delay) => {}
                }
            }
        }
    };
    let keep_alive_client = Arc::new(keep_alive_client);
    let stream = config.stream.as_str();
    let topic = config.topic.as_str();
    let stream_path = owned_value_path!("stream");
    let topic_path = owned_value_path!("topic");
    let partition_id_path = owned_value_path!("partition_id");
    let offset_path = owned_value_path!("offset");
    let metadata = MessageMetadata {
        log_namespace,
        stream,
        topic,
        stream_key: config.stream_key_field.path.as_ref(),
        topic_key: config.topic_key_field.path.as_ref(),
        partition_id_key: config.partition_id_key_field.path.as_ref(),
        offset_key: config.offset_key_field.path.as_ref(),
        stream_path: &stream_path,
        topic_path: &topic_path,
        partition_id_path: &partition_id_path,
        offset_path: &offset_path,
    };

    // Each partition gets its own ordered finalizer so a stalled batch on
    // one partition does not block another's commits. Ordering within a
    // partition is what the fence logic depends on.
    let mut partitions: HashMap<u32, PartitionState> = HashMap::new();
    let mut finalizers: HashMap<u32, OrderedFinalizer<u64>> = HashMap::new();
    let mut ack_streams: StreamMap<u32, BoxStream<'static, (BatchStatus, u64)>> = StreamMap::new();
    let mut generations: HashMap<u32, u64> = HashMap::new();
    // A seek starts a new SDK generation, so a replay seek looks like a
    // reassignment. The strategy of each seek issued here is kept so its
    // generation can be told apart from an assignment's.
    let mut own_seeks: HashMap<u32, PollingStrategy> = HashMap::new();

    // The SDK polls from the server-stored consumer offset, so it must track
    // the consumed position or every poll refetches the same window. Commit
    // once half a batch is acknowledged; also commit a sparse partition once
    // all its polled messages settle, paced per partition so a paced commit on
    // another partition cannot suppress it. The timer is a shutdown backstop.
    let commit_after = u64::from((config.batch_length / 2).max(1));
    let settled_commit_gap = Duration::from_millis(config.poll_interval_ms.max(1));
    let mut last_settled_commits: HashMap<u32, Instant> = HashMap::new();
    let mut commit_timer = interval(Duration::from_secs(config.commit_interval_secs.max(1)));
    commit_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

    // Set when the downstream sender closes so post-loop cleanup still runs.
    // Returning from the select arm would leave the TCP session open until
    // the broker times it out and delay rebalance.
    let mut downstream_closed = false;
    // A consumer EOF before shutdown stops the source; report it as a failure
    // so the topology does not treat the stopped component as completed.
    let mut consumer_ended = false;

    // Back off on repeated read failures so a broker outage does not log and
    // count an error every 500 ms. Doubles from 500 ms to 30 s and resets
    // after a successful poll.
    let mut read_backoff = ExponentialBackoff::from_millis(2)
        .factor(250)
        .max_delay(Duration::from_secs(30));

    // Seek requests are queued here because the select arm holds a borrow on
    // the consumer. Deadlines are per partition: a failed seek must not drop
    // another partition's queued recovery, and the SDK stops polling a
    // partition it refused, so no read result would schedule a retry.
    let mut pending_seeks: HashMap<u32, PendingSeek> = HashMap::new();
    let mut seek_backoff = ExponentialBackoff::from_millis(2)
        .factor(250)
        .max_delay(Duration::from_secs(30));

    loop {
        let now = tokio::time::Instant::now();
        let due_seek = pending_seeks
            .iter()
            .find(|(_, seek)| seek.at <= now)
            .map(|(partition_id, seek)| (*partition_id, seek.strategy, seek.resets_history));
        if let Some((partition_id, strategy, resets_history)) = due_seek {
            let seek = tokio::select! {
                result = consumer.seek(partition_id, strategy) => result,
                _ = &mut shutdown => {
                    info!("Shutdown signal received. Stopping Iggy consumer.");
                    break;
                }
            };
            match seek {
                Ok(()) => {
                    pending_seeks.remove(&partition_id);
                    seek_backoff.reset();
                    if resets_history {
                        // The recovered partition belongs to a new history:
                        // offsets, fences, and finalizers of the old one
                        // cannot be committed and must not block it.
                        reset_partition(
                            partition_id,
                            &mut partitions,
                            &mut finalizers,
                            &mut ack_streams,
                            &mut last_settled_commits,
                        );
                    } else {
                        own_seeks.insert(partition_id, strategy);
                    }
                }
                Err(error) => {
                    let unauthenticated = is_unauthenticated(&error);
                    emit(IggyReadError { error });
                    if unauthenticated && logs_in_over_http(&config.url) {
                        match login_if_http(&keep_alive_client, &config.url).await {
                            Ok(()) => {
                                warn!(
                                    message = "Iggy access token was rejected; re-authenticated with the configured credentials."
                                );
                                read_backoff.reset();
                            }
                            Err(error) => emit(IggyReadError { error }),
                        }
                    }
                    let delay = seek_backoff.next().unwrap_or(Duration::from_secs(30));
                    queue_seek(
                        &mut pending_seeks,
                        partition_id,
                        strategy,
                        tokio::time::Instant::now() + delay,
                        resets_history,
                    );
                }
            }
        }

        tokio::select! {
            biased;

            // Drain acknowledgements before polling so the finalizer queues
            // stay bounded; the guard skips this until a finalizer exists.
            Some((partition_id, (status, offset))) = ack_streams.next(),
                if !ack_streams.is_empty() =>
            {
                let mut retry = RetryOutcome::Idle;
                let should_commit = match partitions.get_mut(&partition_id) {
                    Some(state) => {
                        record_ack(state, partition_id, status, offset);
                        if !matches!(status, BatchStatus::Delivered) {
                            // Only the fenced message spends the replay budget;
                            // a rejection queued behind the fence waits for its
                            // turn instead of exhausting the lower offset's
                            // attempts before it was ever replayed.
                            if state.fence() == Some(offset) {
                                retry = state.retry();
                            }
                            None
                        } else if state.lag() >= commit_after {
                            Some(false)
                        } else {
                            let paced = last_settled_commits
                                .get(&partition_id)
                                .is_none_or(|t| t.elapsed() >= settled_commit_gap);
                            (state.lag() > 0 && state.is_settled() && paced).then_some(true)
                        }
                    }
                    None => None,
                };
                match retry {
                    RetryOutcome::Seek(retry_offset) => {
                        // Replay the rejected message so a transient
                        // downstream failure does not fence the partition
                        // until the process restarts.
                        queue_seek(
                            &mut pending_seeks,
                            partition_id,
                            PollingStrategy::offset(retry_offset),
                            tokio::time::Instant::now(),
                            false,
                        );
                    }
                    RetryOutcome::Skipped(skipped) => {
                        error!(
                            message = "Replayed message was rejected again; skipping the offset so the partition can make progress. Events after it are delivered but the skipped one is lost.",
                            partition_id,
                            offset = skipped,
                            attempts = MAX_REDELIVERY_ATTEMPTS,
                        );
                        // A rejection above the skipped offset has no
                        // acknowledgement left to trigger its replay, so start
                        // it here or it would fence the partition forever.
                        if let Some(next) = partitions
                            .get(&partition_id)
                            .and_then(PartitionState::fence)
                        {
                            queue_seek(
                                &mut pending_seeks,
                                partition_id,
                                PollingStrategy::offset(next),
                                tokio::time::Instant::now(),
                                false,
                            );
                        }
                    }
                    RetryOutcome::Idle => {}
                }
                if let Some(settled) = should_commit {
                    if settled {
                        last_settled_commits.insert(partition_id, Instant::now());
                    }
                    commit_offsets(
                        &consumer,
                        &keep_alive_client,
                        &config.url,
                        stream,
                        topic,
                        &mut partitions,
                        &mut finalizers,
                        &mut ack_streams,
                    )
                    .await;
                }
            }

            _ = commit_timer.tick() => {
                commit_offsets(
                    &consumer,
                    &keep_alive_client,
                    &config.url,
                    stream,
                    topic,
                    &mut partitions,
                    &mut finalizers,
                    &mut ack_streams,
                )
                .await;
            }

            // A stopped partition yields no read result, so the retry needs its
            // own wake-up or it would wait behind `consumer.next()` forever.
            () = sleep_until(
                pending_seeks.values().map(|seek| seek.at).min().unwrap_or_else(tokio::time::Instant::now),
            ),
                if !pending_seeks.is_empty() => {}

            _ = &mut shutdown => {
                info!("Shutdown signal received. Stopping Iggy consumer.");
                break;
            }

            next = consumer.next(), if !at_capacity(&partitions) => {
                match next {
                    Some(Ok(received)) => {
                        read_backoff.reset();

                        // A new assignment starts a fresh partition generation.
                        // Fences and pending acks from the previous one must not
                        // survive it, or a stale rejection blocks every commit.
                        // A seek issued for a replay also starts a generation;
                        // that one must keep the fence and its replay budget or
                        // the budget resets on every attempt and never expires.
                        if let Some(generation) =
                            received.checkpoint().stamp().map(|stamp| stamp.generation)
                        {
                            let partition_id = received.partition_id;
                            let offset = received.message.header.offset;
                            let previous = generations.insert(partition_id, generation);
                            if previous != Some(generation)
                                && !seek_started(own_seeks.remove(&partition_id), offset)
                            {
                                reset_partition(
                                    partition_id,
                                    &mut partitions,
                                    &mut finalizers,
                                    &mut ack_streams,
                                    &mut last_settled_commits,
                                );
                            }
                        }

                        let checkpoint = received.checkpoint();
                        let mut tracker = AckTracker {
                            partitions: &mut partitions,
                            finalizers: &mut finalizers,
                            ack_streams: &mut ack_streams,
                        };
                        if process_received_message(
                            received,
                            checkpoint,
                            &decoder,
                            &metadata,
                            acknowledgements,
                            &mut tracker,
                            &mut out,
                        )
                        .await
                        .is_err()
                        {
                            downstream_closed = true;
                            break;
                        }
                    }
                    Some(Err(error)) => {
                        // A lasting refusal or a lost history stops the
                        // partition: the SDK excludes it from polling and
                        // keeps the stream open, so only a seek reads it again.
                        let stopped = stopped_partition(&error);
                        let unauthenticated = is_unauthenticated(&error.error);
                        let resets_history =
                            matches!(error.error, IggyError::HistoryUnavailable);
                        let skip = error.skip;
                        let skipped_partition = error.partition_id;
                        let failed_offset = error.offset;
                        emit(IggyReadError { error: error.error });
                        if let Some(partition_id) = stopped {
                            warn!(
                                message = "Partition stopped after repeated read failures; seeking it to resume polling.",
                                partition_id,
                            );
                            // Any successful seek resumes the partition, and a
                            // queued replay reads from the fence `next()` would
                            // skip. Only a history reset supersedes it.
                            if resets_history || !pending_seeks.contains_key(&partition_id) {
                                queue_seek(
                                    &mut pending_seeks,
                                    partition_id,
                                    PollingStrategy::next(),
                                    tokio::time::Instant::now(),
                                    resets_history,
                                );
                            }
                        } else if let (Some(partition_id), Some(strategy)) =
                            (skipped_partition, skip)
                        {
                            // A record the broker cannot decode or decrypt is
                            // reported with the continuation after it: the SDK
                            // re-reads that record until a seek skips it.
                            warn!(
                                message = "Iggy record could not be decoded; seeking past it so the partition can make progress.",
                                partition_id,
                                offset = ?failed_offset,
                            );
                            let state = partitions.entry(partition_id).or_default();
                            let failed = failed_offset.or_else(|| {
                                (strategy.kind == PollingKind::Offset)
                                    .then_some(strategy.value)
                                    .and_then(|next| next.checked_sub(1))
                            });
                            match state.fence_entry() {
                                // Replay a lower rejection first so its event
                                // is not skipped with the unreadable record;
                                // the read error repeats until the fence clears.
                                Some((fence, _))
                                    if failed.is_none_or(|failed| fence < failed) =>
                                {
                                    queue_seek(
                                        &mut pending_seeks,
                                        partition_id,
                                        PollingStrategy::offset(fence),
                                        tokio::time::Instant::now(),
                                        false,
                                    );
                                }
                                _ => {
                                    if let Some(failed) = failed {
                                        state.skip_through(failed);
                                    }
                                    queue_seek(
                                        &mut pending_seeks,
                                        partition_id,
                                        strategy,
                                        tokio::time::Instant::now(),
                                        false,
                                    );
                                }
                            }
                        }
                        // An expired HTTP access token is renewed from the
                        // connection string; the SDK never refreshes it.
                        let mut delay = read_backoff.next().unwrap_or(Duration::from_secs(30));
                        if unauthenticated && logs_in_over_http(&config.url) {
                            match login_if_http(&keep_alive_client, &config.url).await {
                                Ok(()) => {
                                    warn!(message = "Iggy access token was rejected; re-authenticated with the configured credentials.");
                                    read_backoff.reset();
                                    delay = Duration::ZERO;
                                }
                                Err(error) => emit(IggyReadError { error }),
                            }
                        }
                        // Some SDK errors do not sleep internally, so without
                        // this delay the loop spins and floods the log. The
                        // sleep races against shutdown to keep stops prompt.
                        tokio::select! {
                            _ = &mut shutdown => {
                                info!("Shutdown signal received. Stopping Iggy consumer.");
                                break;
                            }
                            () = sleep(delay) => {}
                        }
                    }
                    None => {
                        emit(IggyConsumerStreamEndedError);
                        consumer_ended = true;
                        break;
                    }
                }
            }
        }
    }

    // Close the downstream channel so sinks see EOF and flush partial batches
    // while their acknowledgements can still be observed; a sink whose timeout
    // outlasts `drain_timeout_secs` would otherwise deliver after the final
    // commit and replay on restart.
    drop(out);

    if acknowledgements {
        // Wait up to `drain_timeout_secs` for in-flight events to settle
        // before the final commit, so delivered messages are not replayed on
        // restart. Clearing the finalizers closes each per-partition sender;
        // their streams end once pending entries finalize. Clear rather than
        // drop so the final commit still sees the partitions.
        finalizers.clear();
        let drain_deadline = sleep(Duration::from_secs(config.drain_timeout_secs));
        tokio::pin!(drain_deadline);
        while !ack_streams.is_empty() {
            tokio::select! {
                biased;

                next = ack_streams.next() => match next {
                    Some((partition_id, (status, offset))) => {
                        if let Some(state) = partitions.get_mut(&partition_id) {
                            record_ack(state, partition_id, status, offset);
                        }
                    }
                    // Breaking here avoids waiting out the drain deadline;
                    // a `Some(..)` pattern would disable this arm instead.
                    None => break,
                },

                () = &mut drain_deadline => {
                    if !ack_streams.is_empty() {
                        warn!(
                            message = "Drain deadline reached with in-flight acknowledgements still pending; committing best-effort and exiting.",
                            drain_timeout_secs = config.drain_timeout_secs,
                            pending_partitions = ack_streams.len(),
                        );
                    }
                    break;
                }
            }
        }
        commit_offsets(
            &consumer,
            &keep_alive_client,
            &config.url,
            stream,
            topic,
            &mut partitions,
            &mut finalizers,
            &mut ack_streams,
        )
        .await;
    }

    if let Err(error) = consumer.shutdown().await {
        warn!(
            message = "Failed to shut down Iggy consumer cleanly; the consumer-group rebalance may be delayed until the broker times out the connection.",
            %error,
        );
    }

    if let Err(error) = keep_alive_client.disconnect().await {
        warn!(
            message = "Failed to disconnect Iggy client on source shutdown.",
            %error,
        );
    }

    info!("Iggy source shut down.");
    if downstream_closed || consumer_ended {
        Err(())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use iggy::prelude::{
        Checkpoint, IggyError, PartitionContext, PollingKind, PollingStrategy, ReadError,
    };

    use std::collections::HashMap;

    use super::{
        MAX_IN_FLIGHT, MAX_REDELIVERY_ATTEMPTS, PartitionState, RetryOutcome, at_capacity,
        queue_seek, record_ack, seek_started, stopped_partition,
    };
    use crate::event::BatchStatus;

    fn checkpoint(offset: u64) -> Checkpoint {
        Checkpoint::new(0, offset, PartitionContext::default())
    }

    /// The fence logic only needs a checkpoint to exist for a polled offset;
    /// its partition history is what the commit path carries.
    impl PartitionState {
        fn mark_offset_in_flight(&mut self, offset: u64) {
            self.mark_in_flight(offset, checkpoint(offset));
        }
    }

    #[test]
    fn delivered_sets_pending_to_max() {
        let mut s = PartitionState::default();
        assert_eq!(s.pending, None);
        s.record_delivered(5);
        assert_eq!(s.pending, Some(5));
        s.record_delivered(7);
        assert_eq!(s.pending, Some(7));
    }

    #[test]
    fn delivered_does_not_lower_max() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_delivered(3);
        assert_eq!(s.max_delivered, Some(10));
        assert_eq!(s.pending, Some(10));
    }

    #[test]
    fn rejection_fences_pending() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(5, false);
        assert_eq!(s.pending, Some(4));
    }

    #[test]
    fn rejection_at_zero_blocks_all_commits() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(0, false);
        assert_eq!(s.pending, None);
    }

    #[test]
    fn lowest_rejection_wins_as_fence() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(7, false);
        s.record_rejection(3, false);
        s.record_rejection(8, false);
        assert_eq!(s.pending, Some(2));
    }

    #[test]
    fn rejection_returns_true_only_on_fence_change() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        assert!(s.record_rejection(5, false));
        assert!(!s.record_rejection(7, false));
        assert!(s.record_rejection(3, false));
    }

    #[test]
    fn delivering_the_fenced_offset_resumes_commits() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(5, false);
        assert_eq!(s.pending, Some(4));

        // The replay of offset 5 is delivered.
        assert!(s.record_delivered(5));
        assert_eq!(s.pending, Some(10));
    }

    #[test]
    fn replaying_the_fence_keeps_higher_rejections() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(5, false);
        s.record_rejection(6, false);
        assert_eq!(s.pending, Some(4));

        // Replaying 5 delivers it while 6 is still undelivered, so a commit
        // must not jump to the highest delivered offset.
        assert!(s.record_delivered(5));
        assert_eq!(s.pending, Some(5));

        assert!(s.record_delivered(6));
        assert_eq!(s.pending, Some(10));
    }

    #[test]
    fn skipping_a_rejection_leaves_the_next_fence_in_place() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(5, false);
        s.record_rejection(6, false);

        for _ in 0..MAX_REDELIVERY_ATTEMPTS {
            assert!(matches!(s.retry(), RetryOutcome::Seek(5)));
        }
        assert!(matches!(s.retry(), RetryOutcome::Skipped(5)));
        assert_eq!(s.fence(), Some(6));
        assert_eq!(s.pending, Some(5));
        assert!(matches!(s.retry(), RetryOutcome::Seek(6)));
    }

    #[test]
    fn a_seek_explains_only_the_generation_it_starts() {
        // No seek explains a changed generation: it is a new assignment.
        assert!(!seek_started(None, 7));
        // A replay resumes at its target.
        assert!(seek_started(Some(PollingStrategy::offset(7)), 7));
        // A generation that opens past the target belongs to the group: the
        // record was replayed elsewhere or retention removed it, and either
        // way the fence can no longer be cleared by a delivery.
        assert!(!seek_started(Some(PollingStrategy::offset(7)), 9));
        // A continuation carries no offset to compare, so it keeps the state
        // it was issued to protect.
        assert!(seek_started(Some(PollingStrategy::next()), 9));
    }

    #[test]
    fn capacity_counts_outstanding_acknowledgements() {
        let mut partitions = HashMap::new();
        assert!(!at_capacity(&partitions));

        let mut state = PartitionState::default();
        for offset in 0..MAX_IN_FLIGHT as u64 {
            state.mark_offset_in_flight(offset);
        }
        partitions.insert(0, state);
        assert!(at_capacity(&partitions));
    }

    #[test]
    fn commit_carries_the_checkpoint_that_delivered_the_offset() {
        let mut s = PartitionState::default();
        s.mark_in_flight(5, checkpoint(5));
        record_ack(&mut s, 0, BatchStatus::Delivered, 5);
        assert_eq!(s.take_pending(), Some((5, Some(checkpoint(5)))));
    }

    #[test]
    fn skipped_offset_commits_without_a_checkpoint() {
        let mut s = PartitionState::default();
        s.mark_in_flight(3, checkpoint(3));
        record_ack(&mut s, 0, BatchStatus::Delivered, 3);
        s.skip_through(7);
        assert_eq!(s.take_pending(), Some((7, None)));
    }

    #[test]
    fn committing_drops_checkpoints_it_covers() {
        let mut s = PartitionState::default();
        s.mark_in_flight(1, checkpoint(1));
        s.mark_in_flight(2, checkpoint(2));
        s.committed_through(1);
        assert!(!s.checkpoints.contains_key(&1));
        assert!(s.checkpoints.contains_key(&2));
    }

    #[test]
    fn delivering_past_the_fence_does_not_clear_it() {
        let mut s = PartitionState::default();
        s.record_rejection(5, false);
        assert!(!s.record_delivered(7));
        assert_eq!(s.pending, Some(4));
    }

    #[test]
    fn replay_budget_exhausts_into_a_skip() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(5, false);

        for _ in 0..MAX_REDELIVERY_ATTEMPTS {
            assert!(matches!(s.retry(), RetryOutcome::Seek(5)));
        }
        // The next attempt gives up so later deliveries can be committed.
        assert!(matches!(s.retry(), RetryOutcome::Skipped(5)));
        assert_eq!(s.pending, Some(10));
        assert!(matches!(s.retry(), RetryOutcome::Idle));
    }

    #[test]
    fn a_lower_rejection_restarts_the_replay_budget() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(5, false);
        assert!(matches!(s.retry(), RetryOutcome::Seek(5)));
        assert!(matches!(s.retry(), RetryOutcome::Seek(5)));

        s.record_rejection(2, false);
        assert!(matches!(s.retry(), RetryOutcome::Seek(2)));
    }

    #[test]
    fn rejection_before_any_delivery_leaves_pending_none() {
        let mut s = PartitionState::default();
        s.record_rejection(5, false);
        assert_eq!(s.pending, None);
    }

    #[test]
    fn lag_is_pending_minus_committed() {
        let mut s = PartitionState::default();
        assert_eq!(s.lag(), 0);
        s.record_delivered(10);
        // Nothing is stored yet, so all eleven offsets are ahead.
        assert_eq!(s.lag(), 11);
        s.committed = Some(4);
        assert_eq!(s.lag(), 6);
    }

    #[test]
    fn lag_counts_the_first_offset_before_any_commit() {
        let mut s = PartitionState::default();
        s.record_delivered(0);
        assert_eq!(s.lag(), 1);
    }

    #[test]
    fn lag_is_zero_when_caught_up_or_unfenced_to_none() {
        let mut s = PartitionState::default();
        assert_eq!(s.lag(), 0);
        s.record_delivered(10);
        s.committed = Some(10);
        assert_eq!(s.lag(), 0);
        s.record_rejection(0, false);
        assert_eq!(s.pending, None);
        assert_eq!(s.lag(), 0);
    }

    #[test]
    fn settle_tracks_in_flight_messages() {
        let mut s = PartitionState::default();
        assert!(s.is_settled());
        s.mark_offset_in_flight(1);
        s.mark_offset_in_flight(2);
        assert!(!s.is_settled());
        record_ack(&mut s, 0, BatchStatus::Delivered, 1);
        assert!(!s.is_settled());
        record_ack(&mut s, 0, BatchStatus::Delivered, 2);
        assert!(s.is_settled());
        // A fully settled sparse batch has committable lag even though it
        // never reaches the eager-commit threshold.
        assert_eq!(s.lag(), 3);
    }

    #[test]
    fn rejection_also_settles_and_fences_sparse_batch() {
        let mut s = PartitionState::default();
        s.mark_offset_in_flight(5);
        record_ack(&mut s, 0, BatchStatus::Delivered, 5);
        assert_eq!(s.pending, Some(5));
        s.mark_offset_in_flight(7);
        // The rejected message settles too when its acknowledgement arrives,
        // but the fence holds the pending offset below it.
        record_ack(&mut s, 0, BatchStatus::Rejected, 7);
        assert!(s.is_settled());
        assert_eq!(s.pending, Some(5));
    }

    #[test]
    fn lasting_refusal_names_the_stopped_partition() {
        let stopped = ReadError {
            partition_id: Some(3),
            error: IggyError::Unauthorized,
            skip: None,
            offset: None,
            record: None,
        };
        assert_eq!(stopped_partition(&stopped), Some(3));

        let transient = ReadError {
            partition_id: Some(3),
            error: IggyError::NotConnected,
            skip: None,
            offset: None,
            record: None,
        };
        assert_eq!(stopped_partition(&transient), None);
    }

    #[test]
    fn transient_rejection_is_never_skipped() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(5, true);
        assert_eq!(s.pending, Some(4));

        // `Errored` means the sink could not deliver the batch but a replay
        // may still succeed, so the offset stays fenced however many times it
        // fails.
        for _ in 0..(MAX_REDELIVERY_ATTEMPTS * 4) {
            assert!(matches!(s.retry(), RetryOutcome::Seek(5)));
        }
        assert_eq!(s.fence(), Some(5));
        assert_eq!(s.pending, Some(4));
    }

    #[test]
    fn skip_clears_fences_it_passes() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(5, false);
        s.record_rejection(8, false);
        assert_eq!(s.fence(), Some(5));

        // The record at offset 5 cannot be decoded; seeking past it drops the
        // fence there but keeps the later rejection.
        s.skip_through(5);
        assert_eq!(s.fence(), Some(8));
        assert_eq!(s.pending, Some(7));

        // A late acknowledgement for the skipped record must not re-fence the
        // partition: the record can never be delivered.
        assert!(!s.record_rejection(5, false));
        assert_eq!(s.fence(), Some(8));

        s.skip_through(8);
        assert_eq!(s.fence(), None);
        assert_eq!(s.pending, Some(10));
    }

    #[test]
    fn skipped_markers_do_not_outlive_their_acknowledgements() {
        let mut s = PartitionState::default();
        // Undecodable records never register an acknowledgement, so their
        // markers cannot stay for the lifetime of the assignment.
        s.skip_through(4);
        s.skip_through(5);
        s.skip_through(6);
        assert_eq!(s.skipped.iter().copied().collect::<Vec<_>>(), vec![6]);

        // A skipped record with a message still in flight keeps its marker so
        // the late acknowledgement cannot re-fence the partition.
        s.mark_offset_in_flight(9);
        s.skip_through(9);
        s.skip_through(10);
        assert_eq!(s.skipped.iter().copied().collect::<Vec<_>>(), vec![9, 10]);
        assert!(!s.record_rejection(9, false));
        assert_eq!(s.fence(), None);
    }

    #[test]
    fn skip_does_not_commit_past_an_earlier_unsettled_offset() {
        let mut s = PartitionState::default();
        // Offset 5 was polled and forwarded but its acknowledgement has not
        // arrived when the broker reports offset 6 as undecodable.
        s.mark_offset_in_flight(5);
        s.record_delivered(4);
        s.skip_through(6);

        // Advancing to the skipped offset would checkpoint past 5 and lose it
        // on restart, so the commit stays behind the unsettled message.
        assert_eq!(s.pending, Some(4));

        record_ack(&mut s, 0, BatchStatus::Delivered, 5);
        assert_eq!(s.pending, Some(6));
    }

    #[test]
    fn late_rejection_below_a_skipped_offset_still_fences() {
        let mut s = PartitionState::default();
        s.mark_offset_in_flight(5);
        s.record_delivered(4);
        s.skip_through(6);

        // The record after 5 was undecodable, but 5 itself is readable: its
        // failure must hold the checkpoint and be replayed rather than being
        // skipped with the unreadable record.
        record_ack(&mut s, 0, BatchStatus::Rejected, 5);
        assert_eq!(s.fence(), Some(5));
        assert_eq!(s.pending, Some(4));
    }

    #[test]
    fn replay_ack_below_the_committed_offset_is_accepted() {
        // A replay seek can register a lower offset after a later one was
        // delivered and committed, so its acknowledgement arrives below
        // `committed`.
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.committed = Some(10);
        s.mark_offset_in_flight(5);

        record_ack(&mut s, 0, BatchStatus::Delivered, 5);

        assert!(s.is_settled());
        assert_eq!(s.pending, Some(10));
    }

    #[test]
    fn replay_does_not_replace_a_pending_history_reset() {
        let mut seeks = HashMap::new();
        let at = tokio::time::Instant::now();
        queue_seek(&mut seeks, 7, PollingStrategy::next(), at, true);

        // A replay of the lost history must not replace the recovery seek.
        queue_seek(&mut seeks, 7, PollingStrategy::offset(3), at, false);

        let seek = seeks.get(&7).expect("seek is queued");
        assert!(seek.resets_history);
        assert_eq!(seek.strategy.kind, PollingKind::Next);
    }
}
