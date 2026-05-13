use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

use chrono::Utc;
use futures::{StreamExt, stream::BoxStream};
use iggy::prelude::{ConsumerGroupClient, Identifier, IggyClient, IggyConsumer};
use tokio::time::{MissedTickBehavior, interval, sleep};
use tokio_stream::StreamMap;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{DecoderFramedRead, decoding::StreamDecodingError},
    config::{LegacyKey, LogNamespace},
    finalizer::OrderedFinalizer,
    lookup::owned_value_path,
};

use crate::{
    SourceSender,
    codecs::Decoder,
    event::{BatchNotifier, BatchStatus, Event, EventFinalizer, EventStatus},
    internal_events::{
        IggyBytesReceived, IggyEventsReceived, IggyOffsetCommitted, IggyOffsetUpdateError,
        IggyOffsetUpdated, IggyReadError, StreamClosedError,
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
    committed: u64,
    max_delivered: Option<u64>,
    rejected: BTreeSet<u64>,
}

impl PartitionState {
    /// Record a Delivered acknowledgement for `offset`. Returns `true` if
    /// this cleared a previously fenced offset (i.e. a redelivery succeeded).
    fn record_delivered(&mut self, offset: u64) -> bool {
        let was_fenced = self.rejected.remove(&offset);
        self.max_delivered = Some(self.max_delivered.map_or(offset, |m| m.max(offset)));
        self.recompute_pending();
        was_fenced
    }

    /// Record a non-Delivered acknowledgement for `offset`. Returns `true`
    /// if the fence (the lowest rejected offset) moved as a result.
    fn record_rejection(&mut self, offset: u64) -> bool {
        let prev_fence = self.fence();
        self.rejected.insert(offset);
        let fence_changed = prev_fence != self.fence();
        self.recompute_pending();
        fence_changed
    }

    fn fence(&self) -> Option<u64> {
        self.rejected.iter().next().copied()
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
        self.pending.map_or(0, |p| p.saturating_sub(self.committed))
    }
}

/// Apply an acknowledgement to a partition's state and emit a log line
/// when the fence changes.
fn record_ack(state: &mut PartitionState, partition_id: u32, status: BatchStatus, offset: u64) {
    match status {
        BatchStatus::Delivered => {
            if state.record_delivered(offset) {
                debug!(
                    message = "Previously rejected Iggy offset was redelivered; fence updated.",
                    partition_id, offset,
                );
            }
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

/// Store each partition's pending offset on the Iggy server. On success the
/// committed offset is updated so the eager-commit threshold is measured
/// against what the server actually knows. On error the pending offset is
/// left in place for a later timer tick or the shutdown drain to retry.
async fn commit_offsets(
    consumer: &mut IggyConsumer,
    stream: &str,
    topic: &str,
    partitions: &mut HashMap<u32, PartitionState>,
) {
    for (&partition_id, state) in partitions.iter_mut() {
        let Some(offset) = state.pending.take() else {
            continue;
        };
        match consumer.store_offset(offset, Some(partition_id)).await {
            Ok(()) => {
                state.committed = offset;
                emit!(IggyOffsetCommitted {
                    stream,
                    topic,
                    partition: partition_id,
                    offset,
                });
            }
            Err(error) => {
                emit!(IggyOffsetUpdateError { error });
                state.pending = Some(offset);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_iggy_source(
    config: IggySourceConfig,
    keep_alive_client: IggyClient,
    mut consumer: IggyConsumer,
    decoder: Decoder,
    log_namespace: LogNamespace,
    acknowledgements: bool,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let stream = config.stream.as_str();
    let topic = config.topic.as_str();
    let stream_key = config.stream_key_field.path.as_ref();
    let topic_key = config.topic_key_field.path.as_ref();
    let stream_path = owned_value_path!("stream");
    let topic_path = owned_value_path!("topic");
    let partition_id_path = owned_value_path!("partition_id");
    let offset_path = owned_value_path!("offset");

    // Build identifiers eagerly so that any parse failure surfaces before
    // the source starts handling traffic. The consumer itself was already
    // built from these same strings, so this should not fail in practice.
    let (stream_id, topic_id, group_id) = match (
        Identifier::from_str_value(stream),
        Identifier::from_str_value(topic),
        Identifier::from_str_value(config.consumer_name.as_str()),
    ) {
        (Ok(s), Ok(t), Ok(g)) => (s, t, g),
        _ => {
            error!(message = "Failed to build Iggy identifiers; cannot start source.");
            return Err(());
        }
    };

    // Per-partition acknowledgement bookkeeping. Each partition gets its
    // own ordered finalizer so a stalled batch on partition A does not
    // head-of-line-block partition B's deliveries from reaching the commit
    // path. Within a partition the queue stays ordered, which the fence
    // logic in `PartitionState` depends on for safety.
    let mut partitions: HashMap<u32, PartitionState> = HashMap::new();
    let mut finalizers: HashMap<u32, OrderedFinalizer<u64>> = HashMap::new();
    let mut ack_streams: StreamMap<u32, BoxStream<'static, (BatchStatus, u64)>> = StreamMap::new();

    // The Iggy SDK polls relative to the server-stored consumer offset, so
    // the committed offset must stay close to the consumed position or
    // every poll re-fetches the same window. Commit eagerly once roughly
    // half a batch has been acknowledged, with the timer below as a
    // backstop for sparse traffic and shutdown.
    let commit_after = u64::from((config.batch_length / 2).max(1));
    let mut commit_timer = interval(Duration::from_secs(config.commit_interval_secs.max(1)));
    commit_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

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
                    commit_offsets(&mut consumer, stream, topic, &mut partitions).await;
                }
            }

            _ = commit_timer.tick() => {
                commit_offsets(&mut consumer, stream, topic, &mut partitions).await;
            }

            _ = &mut shutdown => {
                info!("Shutdown signal received. Stopping Iggy consumer.");
                break;
            }

            next = consumer.next() => {
                match next {
                    Some(Ok(received)) => {
                        let payload = &received.message.payload;
                        let partition_id = received.partition_id;
                        emit!(IggyBytesReceived {
                            byte_size: payload.len(),
                            stream,
                            topic,
                            partition: partition_id,
                        });
                        emit!(IggyOffsetUpdated {
                            stream,
                            topic,
                            partition: partition_id,
                            message_offset: received.message.header.offset,
                            current_offset: received.current_offset,
                        });

                        let (batch, receiver) = BatchNotifier::new_with_receiver();
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
                                    emit!(IggyEventsReceived {
                                        count,
                                        byte_size,
                                        stream,
                                        topic,
                                        partition: partition_id,
                                    });
                                    let now = Utc::now();
                                    let offset = received.message.header.offset;

                                    let events = events.into_iter().map(|mut event| {
                                        if let Event::Log(ref mut log) = event {
                                            log_namespace.insert_standard_vector_source_metadata(
                                                log,
                                                IggySourceConfig::NAME,
                                                now,
                                            );
                                            log_namespace.insert_source_metadata(
                                                IggySourceConfig::NAME,
                                                log,
                                                stream_key.map(LegacyKey::InsertIfEmpty),
                                                &stream_path,
                                                stream,
                                            );
                                            log_namespace.insert_source_metadata(
                                                IggySourceConfig::NAME,
                                                log,
                                                topic_key.map(LegacyKey::InsertIfEmpty),
                                                &topic_path,
                                                topic,
                                            );
                                            log_namespace.insert_source_metadata(
                                                IggySourceConfig::NAME,
                                                log,
                                                None::<LegacyKey<&str>>,
                                                &partition_id_path,
                                                partition_id as i64,
                                            );
                                            log_namespace.insert_source_metadata(
                                                IggySourceConfig::NAME,
                                                log,
                                                None::<LegacyKey<&str>>,
                                                &offset_path,
                                                offset as i64,
                                            );
                                        }
                                        if acknowledgements {
                                            event.with_batch_notifier(&batch)
                                        } else {
                                            event
                                        }
                                    });

                                    if out.send_batch(events).await.is_err() {
                                        emit!(StreamClosedError { count });
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

                        // Drop our handle so the batch is finalized once the
                        // events that were attached to it are dropped
                        // downstream. On a non-continuable decode error, wrap
                        // the batch in an `EventFinalizer` set to `Rejected`
                        // so the offset is not committed; otherwise the
                        // default `Delivered` status would let a malformed
                        // message advance the consumer.
                        if decode_failed {
                            let efin = EventFinalizer::new(batch);
                            efin.update_status(EventStatus::Rejected);
                            drop(efin);
                        } else {
                            drop(batch);
                        }

                        if channel_closed {
                            return Err(());
                        }

                        if acknowledgements {
                            let finalizer = finalizers
                                .entry(partition_id)
                                .or_insert_with(|| {
                                    let (finalizer, stream) =
                                        OrderedFinalizer::<u64>::new(None);
                                    ack_streams.insert(partition_id, stream);
                                    finalizer
                                });
                            partitions.entry(partition_id).or_default();
                            finalizer.add(received.message.header.offset, receiver);
                        }
                    }
                    Some(Err(error)) => {
                        emit!(IggyReadError { error });
                    }
                    None => {
                        warn!("Iggy consumer stream ended unexpectedly.");
                        break;
                    }
                }
            }
        }
    }

    if acknowledgements {
        // Stop accepting new pending entries, then wait (bounded by
        // `drain_timeout_secs`) for events already sent downstream to be
        // acknowledged before committing the final offsets, so a graceful
        // shutdown does not replay delivered messages on restart. Dropping
        // the finalizer map closes each per-partition sender; the streams
        // remain in `ack_streams` until their pending entries finalize,
        // then end and are removed from the map.
        drop(finalizers);
        let drain_deadline = sleep(Duration::from_secs(config.drain_timeout_secs));
        tokio::pin!(drain_deadline);
        while !ack_streams.is_empty() {
            tokio::select! {
                biased;

                Some((partition_id, (status, offset))) = ack_streams.next() => {
                    if let Some(state) = partitions.get_mut(&partition_id) {
                        record_ack(state, partition_id, status, offset);
                    }
                }

                _ = &mut drain_deadline => break,
            }
        }
        commit_offsets(&mut consumer, stream, topic, &mut partitions).await;
    }

    // Without acknowledgements, the SDK's `shutdown()` cleanly flushes its
    // own offset tracking and leaves the consumer group. With
    // acknowledgements, that flush walks `last_consumed_offsets` (updated
    // by every successful `consumer.next()`) and stores any offset higher
    // than `last_stored_offsets`, which would commit polled-but-not-yet-
    // delivered offsets to the broker and skip rejected or in-flight
    // messages on restart. Bypass the SDK flush in that case and leave
    // the consumer group directly via the keep-alive client (no-op when
    // the consumer is pinned to a single partition).
    if acknowledgements {
        if config.partition.is_none()
            && let Err(error) = keep_alive_client
                .leave_consumer_group(&stream_id, &topic_id, &group_id)
                .await
        {
            warn!(
                message = "Failed to leave Iggy consumer group on shutdown; rebalance may be delayed until the broker times out the connection.",
                %error,
            );
        }
    } else if let Err(error) = consumer.shutdown().await {
        warn!(
            message = "Failed to shut down Iggy consumer cleanly; the consumer-group rebalance may be delayed until the broker times out the connection.",
            %error,
        );
    }

    info!("Iggy source shut down.");
    Ok(())
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
    fn delivered_returns_false_when_not_fenced() {
        let mut s = PartitionState::default();
        assert!(!s.record_delivered(5));
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
    fn redelivery_clears_fence_and_advances_pending() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(5);
        assert_eq!(s.pending, Some(4));
        assert!(s.record_delivered(5));
        assert_eq!(s.pending, Some(10));
    }

    #[test]
    fn pending_advances_to_next_fence_after_partial_clear() {
        let mut s = PartitionState::default();
        s.record_delivered(10);
        s.record_rejection(5);
        s.record_rejection(7);
        assert_eq!(s.pending, Some(4));
        s.record_delivered(5);
        assert_eq!(s.pending, Some(6));
        s.record_delivered(7);
        assert_eq!(s.pending, Some(10));
    }

    #[test]
    fn lag_is_pending_minus_committed() {
        let mut s = PartitionState::default();
        assert_eq!(s.lag(), 0);
        s.record_delivered(10);
        assert_eq!(s.lag(), 10);
        s.committed = 4;
        assert_eq!(s.lag(), 6);
    }

    #[test]
    fn lag_is_zero_when_caught_up_or_unfenced_to_none() {
        let mut s = PartitionState::default();
        assert_eq!(s.lag(), 0);
        s.record_delivered(10);
        s.committed = 10;
        assert_eq!(s.lag(), 0);
        s.record_rejection(0);
        assert_eq!(s.pending, None);
        assert_eq!(s.lag(), 0);
    }
}
