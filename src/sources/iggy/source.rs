use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

use chrono::Utc;
use futures::StreamExt;
use iggy::prelude::{IggyClient, IggyConsumer};
use tokio::time::{MissedTickBehavior, interval, sleep};
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
    event::{BatchNotifier, BatchStatus, Event},
    internal_events::{
        IggyBytesReceived, IggyEventsReceived, IggyOffsetCommitted, IggyOffsetUpdateError,
        IggyOffsetUpdated, IggyReadError, StreamClosedError,
    },
    shutdown::ShutdownSignal,
    sources::iggy::config::IggySourceConfig,
};

/// An entry tracked by the [`OrderedFinalizer`] until the corresponding batch
/// of events has been acknowledged downstream, at which point the partition's
/// consumer offset is advanced and later committed to the Iggy server.
#[derive(Debug)]
struct FinalizerEntry {
    partition_id: u32,
    offset: u64,
}

/// Recompute the committable offset for a partition given the highest
/// delivered offset and the set of currently-rejected offsets. The "fence"
/// is the lowest rejected offset; `pending` must never reach or exceed it
/// so that a restart does not skip a message that was never delivered.
fn recompute_pending(
    pending: &mut HashMap<u32, u64>,
    max_delivered: &HashMap<u32, u64>,
    rejected: &HashMap<u32, BTreeSet<u64>>,
    partition_id: u32,
) {
    let Some(&max) = max_delivered.get(&partition_id) else {
        return;
    };
    let fence = rejected
        .get(&partition_id)
        .and_then(|set| set.iter().next().copied());
    match fence {
        // The very first message (offset 0) was rejected; nothing can be
        // committed safely.
        Some(0) => {
            pending.remove(&partition_id);
        }
        Some(f) => {
            pending.insert(partition_id, max.min(f - 1));
        }
        None => {
            pending.insert(partition_id, max);
        }
    }
}

/// Record a non-Delivered acknowledgement, inserting the offset into the
/// rejection set and recomputing the partition's pending offset. Emits a
/// warning the first time the fence is lowered so operators can see why
/// offset progress has stalled.
fn record_rejection(
    rejected: &mut HashMap<u32, BTreeSet<u64>>,
    pending: &mut HashMap<u32, u64>,
    max_delivered: &HashMap<u32, u64>,
    partition_id: u32,
    offset: u64,
    status: BatchStatus,
) {
    let set = rejected.entry(partition_id).or_default();
    let prev_fence = set.iter().next().copied();
    set.insert(offset);
    let new_fence = set.iter().next().copied();
    if prev_fence != new_fence {
        warn!(
            message = "Message was not delivered downstream; consumer offset for this partition will not advance past it on restart.",
            partition_id,
            offset,
            ?status,
        );
    }
    recompute_pending(pending, max_delivered, rejected, partition_id);
}

/// Record a Delivered acknowledgement, removing the offset from the rejection
/// set (in case of in-process redelivery), updating the highest delivered
/// offset for the partition, and recomputing the partition's pending offset.
fn record_delivered(
    rejected: &mut HashMap<u32, BTreeSet<u64>>,
    pending: &mut HashMap<u32, u64>,
    max_delivered: &mut HashMap<u32, u64>,
    partition_id: u32,
    offset: u64,
) {
    if let Some(set) = rejected.get_mut(&partition_id) {
        let was_fenced = set.remove(&offset);
        if set.is_empty() {
            rejected.remove(&partition_id);
        }
        if was_fenced {
            debug!(
                message = "Previously rejected Iggy offset was redelivered; fence updated.",
                partition_id, offset,
            );
        }
    }
    let slot = max_delivered.entry(partition_id).or_insert(offset);
    *slot = (*slot).max(offset);
    recompute_pending(pending, max_delivered, rejected, partition_id);
}

/// Store the latest acknowledged offset for each partition on the Iggy server.
///
/// On success the committed offset is recorded so that the eager-commit
/// threshold in the acknowledgement path is measured against what the server
/// actually knows about.
async fn commit_offsets(
    consumer: &mut IggyConsumer,
    stream: &str,
    topic: &str,
    pending: &mut HashMap<u32, u64>,
    committed: &mut HashMap<u32, u64>,
) {
    for (partition_id, offset) in std::mem::take(pending) {
        match consumer.store_offset(offset, Some(partition_id)).await {
            Ok(()) => {
                committed.insert(partition_id, offset);
                emit!(IggyOffsetCommitted {
                    stream,
                    topic,
                    partition: partition_id,
                    offset,
                });
            }
            Err(error) => {
                emit!(IggyOffsetUpdateError { error });
                // Keep the offset pending so a later timer tick or the
                // shutdown drain retries it instead of losing the record.
                let slot = pending.entry(partition_id).or_insert(offset);
                *slot = (*slot).max(offset);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_iggy_source(
    config: IggySourceConfig,
    _keep_alive_client: IggyClient,
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
    let (finalizer, mut ack_stream) = OrderedFinalizer::<FinalizerEntry>::new(None);

    // The highest acknowledged offset per partition that has not yet been
    // committed to the server, and the offset most recently committed for each
    // partition. The Iggy SDK polls relative to the server-stored consumer
    // offset, so the committed offset must stay close to the consumed position
    // or every poll re-fetches the same window; we therefore commit eagerly
    // once roughly half a batch has been acknowledged, with the timer below as
    // a backstop for sparse traffic and shutdown.
    let mut pending_offsets: HashMap<u32, u64> = HashMap::new();
    let mut committed_offsets: HashMap<u32, u64> = HashMap::new();
    // The highest offset per partition that has been observed as Delivered
    // downstream, regardless of whether a lower offset is currently fenced.
    // Tracking this separately from `pending_offsets` allows `pending_offsets`
    // to jump forward correctly when a redelivery clears the fence.
    let mut max_delivered_offsets: HashMap<u32, u64> = HashMap::new();
    // The set of offsets per partition that were not delivered downstream and
    // have not yet been redelivered. The smallest entry is the fence;
    // `pending_offsets` must never advance to or past it so that a restart
    // does not skip a message that was rejected or errored.
    let mut rejected_offsets: HashMap<u32, BTreeSet<u64>> = HashMap::new();
    let commit_after = u64::from((config.batch_length / 2).max(1));
    let mut commit_timer = interval(Duration::from_secs(config.commit_interval_secs.max(1)));
    commit_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            biased;

            // Handle acknowledgements before polling for new messages so that
            // the finalizer queue cannot grow unbounded under high load.
            ack = ack_stream.next() => {
                match ack {
                    Some((BatchStatus::Delivered, entry)) => {
                        record_delivered(
                            &mut rejected_offsets,
                            &mut pending_offsets,
                            &mut max_delivered_offsets,
                            entry.partition_id,
                            entry.offset,
                        );
                        let pending = pending_offsets
                            .get(&entry.partition_id)
                            .copied()
                            .unwrap_or(0);
                        let committed = committed_offsets
                            .get(&entry.partition_id)
                            .copied()
                            .unwrap_or(0);
                        if pending.saturating_sub(committed) >= commit_after {
                            commit_offsets(&mut consumer, stream, topic, &mut pending_offsets, &mut committed_offsets)
                                .await;
                        }
                    }
                    Some((status, entry)) => {
                        record_rejection(
                            &mut rejected_offsets,
                            &mut pending_offsets,
                            &max_delivered_offsets,
                            entry.partition_id,
                            entry.offset,
                            status,
                        );
                    }
                    None => {
                        error!("Iggy acknowledgement stream closed unexpectedly.");
                        break;
                    }
                }
            }

            _ = commit_timer.tick() => {
                commit_offsets(&mut consumer, stream, topic, &mut pending_offsets, &mut committed_offsets).await;
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
                                        break;
                                    }
                                }
                            }
                        }

                        // Drop our handle so that the batch is finalized once the
                        // events that were attached to it are dropped downstream.
                        drop(batch);

                        if channel_closed {
                            return Err(());
                        }

                        if acknowledgements {
                            finalizer.add(
                                FinalizerEntry {
                                    partition_id,
                                    offset: received.message.header.offset,
                                },
                                receiver,
                            );
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
        // `drain_timeout_secs`) for the events already sent downstream to be
        // acknowledged before committing the final offsets, so a graceful
        // shutdown does not replay delivered messages on restart.
        drop(finalizer);
        let drain_deadline = sleep(Duration::from_secs(config.drain_timeout_secs));
        tokio::pin!(drain_deadline);
        loop {
            tokio::select! {
                biased;

                ack = ack_stream.next() => match ack {
                    Some((BatchStatus::Delivered, entry)) => {
                        record_delivered(
                            &mut rejected_offsets,
                            &mut pending_offsets,
                            &mut max_delivered_offsets,
                            entry.partition_id,
                            entry.offset,
                        );
                    }
                    Some((status, entry)) => {
                        record_rejection(
                            &mut rejected_offsets,
                            &mut pending_offsets,
                            &max_delivered_offsets,
                            entry.partition_id,
                            entry.offset,
                            status,
                        );
                    }
                    None => break,
                },

                _ = &mut drain_deadline => break,
            }
        }
        commit_offsets(
            &mut consumer,
            stream,
            topic,
            &mut pending_offsets,
            &mut committed_offsets,
        )
        .await;
    }

    info!("Iggy source shut down.");
    Ok(())
}
