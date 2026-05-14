use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::Utc;
use futures::{StreamExt, future::join_all, stream::BoxStream};
use iggy::prelude::{Client, IggyConsumer, IggyError, ReceivedMessage};
use tokio::time::{MissedTickBehavior, interval, sleep};
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
    common::backoff::ExponentialBackoff,
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
    internal_events::{
        IggyBytesReceived, IggyConsumerInitError, IggyConsumerStreamEndedError, IggyEventsReceived,
        IggyOffsetCommitted, IggyOffsetPolled, IggyOffsetUpdateError, IggyReadError,
        StreamClosedError,
    },
    shutdown::ShutdownSignal,
    sources::iggy::config::IggySourceConfig,
};

/// Per-partition acknowledgement bookkeeping. The "fence" is the lowest
/// rejected offset; `pending` is the highest offset that can be safely
/// committed to the server without skipping a rejected message on restart.
#[derive(Default)]
struct PartitionState {
    pending: Option<u64>,
    /// Highest offset the server confirmed; `None` until the first store, so a
    /// pending offset 0 still counts as lag.
    committed: Option<u64>,
    max_delivered: Option<u64>,
    /// Lowest offset whose final acknowledgement was not `Delivered`. Only
    /// the minimum matters (`pending` never advances past it), so keeping
    /// every rejection would grow without bound under sustained downstream
    /// failures.
    rejected_fence: Option<u64>,
}

impl PartitionState {
    fn record_delivered(&mut self, offset: u64) {
        self.max_delivered = Some(self.max_delivered.map_or(offset, |m| m.max(offset)));
        self.recompute_pending();
    }

    /// Record a non-Delivered acknowledgement for `offset`. Returns `true`
    /// if the fence (the lowest rejected offset) moved as a result.
    fn record_rejection(&mut self, offset: u64) -> bool {
        let prev_fence = self.rejected_fence;
        self.rejected_fence = Some(prev_fence.map_or(offset, |fence| fence.min(offset)));
        let fence_changed = prev_fence != self.rejected_fence;
        self.recompute_pending();
        fence_changed
    }

    const fn fence(&self) -> Option<u64> {
        self.rejected_fence
    }

    fn recompute_pending(&mut self) {
        let Some(max) = self.max_delivered else {
            return;
        };
        self.pending = match self.fence() {
            // The very first message (offset 0) was rejected; nothing can be
            // committed safely.
            Some(0) => None,
            Some(f) => Some(max.min(f - 1)),
            None => Some(max),
        };
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

fn record_ack(state: &mut PartitionState, partition_id: u32, status: BatchStatus, offset: u64) {
    // OrderedFinalizer guarantees per-partition FIFO ordering; acks must
    // arrive in offset order for the fence/pending monotonicity invariants
    // in PartitionState to hold.
    debug_assert!(
        offset >= state.committed.unwrap_or(0),
        "ack for offset {offset} arrived out of order (partition {partition_id}, committed={})",
        state.committed.unwrap_or(0)
    );
    match status {
        BatchStatus::Delivered => {
            state.record_delivered(offset);
        }
        status => {
            if state.record_rejection(offset) {
                warn!(
                    message = "Message was not delivered downstream; consumer offset for this partition will not advance past it on restart.",
                    partition_id,
                    offset,
                    ?status,
                );
            }
        }
    }
}

/// Store each partition's pending offset on the Iggy server. The per-
/// partition `store_offset` calls run concurrently so a slow broker
/// response on partition A does not block the `tokio::select!` for the
/// duration of every other partition's commit (which would back up new
/// polls in the SDK queue and delay shutdown observation).
///
/// On success the committed offset is updated so the eager-commit
/// threshold is measured against what the server actually knows. On
/// `IggyError::NotResolvedConsumer` the broker has told us we no
/// longer own the partition (typical after a consumer-group rebalance
/// revokes it), so the per-partition entries in `partitions`,
/// `finalizers`, and `ack_streams` are removed; without that, every
/// subsequent commit tick would emit the same error indefinitely. On
/// other transient errors the pending offset is not re-queued; a later
/// ack will set a fresh (higher) pending value and the next commit
/// tick will pick it up.
async fn commit_offsets(
    consumer: &IggyConsumer,
    stream: &str,
    topic: &str,
    partitions: &mut HashMap<u32, PartitionState>,
    finalizers: &mut HashMap<u32, OrderedFinalizer<u64>>,
    ack_streams: &mut StreamMap<u32, BoxStream<'static, (BatchStatus, u64)>>,
) {
    let pending: Vec<(u32, u64)> = partitions
        .iter_mut()
        .filter_map(|(&partition_id, state)| state.pending.take().map(|o| (partition_id, o)))
        .collect();

    if pending.is_empty() {
        return;
    }

    let results = join_all(
        pending
            .into_iter()
            .map(|(partition_id, offset)| async move {
                let result = consumer.store_offset(offset, Some(partition_id)).await;
                (partition_id, offset, result)
            }),
    )
    .await;

    for (partition_id, offset, result) in results {
        match result {
            Ok(()) => {
                if let Some(state) = partitions.get_mut(&partition_id) {
                    state.committed = Some(offset);
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
                emit(IggyOffsetUpdateError { error });
            }
        }
    }
}

/// Configuration needed to enrich each log event with source metadata.
/// Built once in `run_iggy_source` and reborrowed for every received
/// message so the inner processing helper has a single-parameter view of
/// the logging context.
struct MessageMetadata<'a> {
    log_namespace: LogNamespace,
    stream: &'a str,
    topic: &'a str,
    stream_key: Option<&'a OwnedValuePath>,
    topic_key: Option<&'a OwnedValuePath>,
    stream_path: &'a OwnedValuePath,
    topic_path: &'a OwnedValuePath,
    partition_id_path: &'a OwnedValuePath,
    offset_path: &'a OwnedValuePath,
}

/// Mutable references to the per-partition acknowledgement bookkeeping.
/// Updated together when a new message is registered for finalization, so
/// bundling them keeps the call sites focused on the high-level flow.
struct AckTracker<'a> {
    partitions: &'a mut HashMap<u32, PartitionState>,
    finalizers: &'a mut HashMap<u32, OrderedFinalizer<u64>>,
    ack_streams: &'a mut StreamMap<u32, BoxStream<'static, (BatchStatus, u64)>>,
}

impl AckTracker<'_> {
    /// Register an in-flight message for acknowledgement tracking. A new
    /// per-partition finalizer and ack stream are created lazily the first
    /// time a partition is seen.
    fn register(&mut self, partition_id: u32, offset: u64, receiver: BatchStatusReceiver) {
        let finalizer = self.finalizers.entry(partition_id).or_insert_with(|| {
            let (finalizer, stream) = OrderedFinalizer::<u64>::new(None);
            self.ack_streams.insert(partition_id, stream);
            finalizer
        });
        self.partitions.entry(partition_id).or_default();
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
                            None::<LegacyKey<&OwnedValuePath>>,
                            metadata.partition_id_path,
                            i64::from(partition_id),
                        );
                        metadata.log_namespace.insert_source_metadata(
                            IggySourceConfig::NAME,
                            log,
                            None::<LegacyKey<&OwnedValuePath>>,
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

    // Drop the BatchNotifier handle so the batch is finalized once all events
    // attached to it are settled downstream. On a decode failure we drop
    // normally (no retroactive rejection) so events from earlier frames that
    // were already sent downstream keep whatever status they earn naturally.
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
            tracker.register(partition_id, offset, status_receiver);
        }
        // Without acknowledgements `CommitPolicy::OnRead` stored the end of the
        // polled batch when it was read, so the failed record is already behind
        // the checkpoint; storing its offset would move it back over records
        // that were delivered.
        return Ok(());
    }

    if acknowledgements {
        tracker.register(partition_id, offset, status_receiver);
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
        stream_path: &stream_path,
        topic_path: &topic_path,
        partition_id_path: &partition_id_path,
        offset_path: &offset_path,
    };

    // Per-partition acknowledgement bookkeeping. Each partition gets its
    // own ordered finalizer so a stalled batch on partition A does not
    // head-of-line-block partition B's deliveries from reaching the commit
    // path. Within a partition the queue stays ordered, which the fence
    // logic in `PartitionState` depends on for safety.
    let mut partitions: HashMap<u32, PartitionState> = HashMap::new();
    let mut finalizers: HashMap<u32, OrderedFinalizer<u64>> = HashMap::new();
    let mut ack_streams: StreamMap<u32, BoxStream<'static, (BatchStatus, u64)>> = StreamMap::new();
    let mut generations: HashMap<u32, u64> = HashMap::new();

    // The Iggy SDK polls relative to the server-stored consumer offset, so
    // the committed offset must stay close to the consumed position or
    // every poll re-fetches the same window. Commit eagerly once roughly
    // half a batch has been acknowledged, with the timer below as a
    // backstop for sparse traffic and shutdown.
    let commit_after = u64::from((config.batch_length / 2).max(1));
    let mut commit_timer = interval(Duration::from_secs(config.commit_interval_secs.max(1)));
    commit_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

    // Set when the downstream `SourceSender` closes mid-stream so the
    // post-loop cleanup (drain, final commit, leave-group, disconnect)
    // still runs before the source returns an error. Returning directly
    // from the select arm would leave the TCP session open until the
    // broker times it out and delay any consumer-group rebalance.
    let mut downstream_closed = false;
    // A consumer EOF before shutdown stops the source; report it as a failure
    // so the topology does not treat the stopped component as completed.
    let mut consumer_ended = false;

    // Exponential backoff for repeated read failures. Without it, a broker
    // outage produces an IggyReadError log line and counter increment every
    // 500 ms for the duration of the outage. Starts at 500 ms and doubles
    // up to 30 s; reset to the base delay after any successful poll.
    let mut read_backoff = ExponentialBackoff::from_millis(2)
        .factor(250)
        .max_delay(Duration::from_secs(30));

    loop {
        tokio::select! {
            biased;

            // Handle acknowledgements before polling for new messages so
            // the finalizer queues cannot grow unbounded under load. The
            // `is_empty` guard prevents this branch from firing before any
            // partition finalizer has been created.
            Some((partition_id, (status, offset))) = ack_streams.next(),
                if !ack_streams.is_empty() =>
            {
                let should_commit = match partitions.get_mut(&partition_id) {
                    Some(state) => {
                        record_ack(state, partition_id, status, offset);
                        matches!(status, BatchStatus::Delivered) && state.lag() >= commit_after
                    }
                    None => false,
                };
                if should_commit {
                    commit_offsets(
                        &consumer,
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
                    stream,
                    topic,
                    &mut partitions,
                    &mut finalizers,
                    &mut ack_streams,
                )
                .await;
            }

            _ = &mut shutdown => {
                info!("Shutdown signal received. Stopping Iggy consumer.");
                break;
            }

            next = consumer.next() => {
                match next {
                    Some(Ok(received)) => {
                        read_backoff.reset();

                        // A new assignment starts a fresh partition generation.
                        // Fences and pending acks from the previous one must not
                        // survive it, or a stale rejection blocks every commit.
                        if let Some(generation) =
                            received.checkpoint().stamp().map(|stamp| stamp.generation)
                        {
                            let partition_id = received.partition_id;
                            if generations.insert(partition_id, generation) != Some(generation) {
                                partitions.remove(&partition_id);
                                finalizers.remove(&partition_id);
                                ack_streams.remove(&partition_id);
                            }
                        }

                        let mut tracker = AckTracker {
                            partitions: &mut partitions,
                            finalizers: &mut finalizers,
                            ack_streams: &mut ack_streams,
                        };
                        if process_received_message(
                            received,
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
                        emit(IggyReadError { error: error.error });
                        // Back off before the next poll. Without this sleep,
                        // SDK error variants that do not internally sleep
                        // (e.g. transient non-connectivity errors) cause a
                        // tight spin that pegs CPU and floods the log. The
                        // delay doubles each iteration up to the configured
                        // max so a sustained outage does not flood either.
                        // The sleep races against shutdown so a graceful
                        // stop does not wait out the full backoff delay.
                        let delay = read_backoff.next().unwrap_or(Duration::from_secs(30));
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
        // Stop accepting new pending entries, then wait (bounded by
        // `drain_timeout_secs`) for events already sent downstream to be
        // acknowledged before committing the final offsets, so a graceful
        // shutdown does not replay delivered messages on restart. Clearing
        // the finalizer map closes each per-partition sender; the streams
        // remain in `ack_streams` until their pending entries finalize,
        // then end and are removed from the map. Clear (rather than drop)
        // so the final `commit_offsets` below can still borrow `finalizers`
        // to prune entries the broker rejects as no-longer-assigned.
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
    use super::PartitionState;

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
        s.record_rejection(5);
        assert_eq!(s.pending, Some(4));
    }

    #[test]
    fn rejection_at_zero_blocks_all_commits() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(0);
        assert_eq!(s.pending, None);
    }

    #[test]
    fn lowest_rejection_wins_as_fence() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(7);
        s.record_rejection(3);
        s.record_rejection(8);
        assert_eq!(s.pending, Some(2));
    }

    #[test]
    fn rejection_returns_true_only_on_fence_change() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        assert!(s.record_rejection(5));
        assert!(!s.record_rejection(7));
        assert!(s.record_rejection(3));
    }

    #[test]
    fn rejection_before_any_delivery_leaves_pending_none() {
        let mut s = PartitionState::default();
        s.record_rejection(5);
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
        s.record_rejection(0);
        assert_eq!(s.pending, None);
        assert_eq!(s.lag(), 0);
    }
}
