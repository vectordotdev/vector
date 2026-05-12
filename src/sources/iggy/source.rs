use std::{collections::HashMap, time::Duration};

use chrono::Utc;
use futures::StreamExt;
use iggy::prelude::{IggyClient, IggyConsumer};
use tokio::time::{MissedTickBehavior, interval};
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
    for (partition_id, offset) in pending.drain() {
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
            Err(error) => emit!(IggyOffsetUpdateError { error }),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_iggy_source(
    config: IggySourceConfig,
    _client: IggyClient,
    mut consumer: IggyConsumer,
    decoder: Decoder,
    log_namespace: LogNamespace,
    acknowledgements: bool,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
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
    let commit_after = u64::from((config.batch_length / 2).max(1));
    let mut commit_timer = interval(Duration::from_secs(config.commit_interval_secs.max(1)));
    commit_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            biased;

            // Handle acknowledgements before polling for new messages so that
            // the finalizer queue cannot grow unbounded under high load.
            ack = ack_stream.next() => {
                if let Some((BatchStatus::Delivered, entry)) = ack {
                    let pending = {
                        let offset = pending_offsets.entry(entry.partition_id).or_default();
                        *offset = (*offset).max(entry.offset);
                        *offset
                    };
                    let committed = committed_offsets
                        .get(&entry.partition_id)
                        .copied()
                        .unwrap_or(0);
                    if pending.saturating_sub(committed) >= commit_after {
                        commit_offsets(&mut consumer, config.stream.as_str(), config.topic.as_str(), &mut pending_offsets, &mut committed_offsets)
                            .await;
                    }
                }
            }

            _ = commit_timer.tick() => {
                commit_offsets(&mut consumer, config.stream.as_str(), config.topic.as_str(), &mut pending_offsets, &mut committed_offsets).await;
            }

            _ = &mut shutdown => {
                info!("Shutdown signal received. Stopping Iggy consumer.");
                commit_offsets(&mut consumer, config.stream.as_str(), config.topic.as_str(), &mut pending_offsets, &mut committed_offsets).await;
                break;
            }

            next = consumer.next() => {
                match next {
                    Some(Ok(received)) => {
                        let payload = &received.message.payload;
                        let partition_id = received.partition_id;
                        emit!(IggyBytesReceived {
                            byte_size: payload.len(),
                            stream: config.stream.as_str(),
                            topic: config.topic.as_str(),
                            partition: partition_id,
                        });
                        emit!(IggyOffsetUpdated {
                            stream: config.stream.as_str(),
                            topic: config.topic.as_str(),
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
                                        stream: config.stream.as_str(),
                                        topic: config.topic.as_str(),
                                        partition: partition_id,
                                    });
                                    let now = Utc::now();
                                    let offset = received.current_offset;

                                    let events = events.into_iter().map(|mut event| {
                                        if let Event::Log(ref mut log) = event {
                                            log_namespace.insert_standard_vector_source_metadata(
                                                log,
                                                IggySourceConfig::NAME,
                                                now,
                                            );
                                            let stream_key = config
                                                .stream_key_field
                                                .path
                                                .as_ref()
                                                .map(LegacyKey::InsertIfEmpty);
                                            let topic_key = config
                                                .topic_key_field
                                                .path
                                                .as_ref()
                                                .map(LegacyKey::InsertIfEmpty);
                                            log_namespace.insert_source_metadata(
                                                IggySourceConfig::NAME,
                                                log,
                                                stream_key,
                                                &owned_value_path!("stream"),
                                                config.stream.as_str(),
                                            );
                                            log_namespace.insert_source_metadata(
                                                IggySourceConfig::NAME,
                                                log,
                                                topic_key,
                                                &owned_value_path!("topic"),
                                                config.topic.as_str(),
                                            );
                                            log_namespace.insert_source_metadata(
                                                IggySourceConfig::NAME,
                                                log,
                                                None::<LegacyKey<&str>>,
                                                &owned_value_path!("partition_id"),
                                                partition_id as i64,
                                            );
                                            log_namespace.insert_source_metadata(
                                                IggySourceConfig::NAME,
                                                log,
                                                None::<LegacyKey<&str>>,
                                                &owned_value_path!("offset"),
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
                        break;
                    }
                }
            }
        }
    }

    info!("Iggy source shut down.");
    Ok(())
}
