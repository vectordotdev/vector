use std::fmt;

use async_nats::jetstream::{
    consumer::pull::Stream as PullConsumerStream,
    message::{AckKind, Acker},
};
use chrono::Utc;
use futures::StreamExt;
use snafu::ResultExt;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{DecoderFramedRead, decoding::StreamDecodingError},
    config::{LegacyKey, LogNamespace},
    finalization::BatchStatus,
    finalizer::UnorderedFinalizer,
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, EventsReceived, EventsReceivedHandle,
        InternalEventHandle as _, Protocol,
    },
    lookup::owned_value_path,
};

use crate::{
    SourceSender,
    codecs::Decoder,
    event::{BatchNotifier, Event},
    internal_events::StreamClosedError,
    shutdown::ShutdownSignal,
    sources::nats::config::{BuildError, NatsSourceConfig, SubscribeSnafu},
};

struct FinalizerEntry(Acker);

impl fmt::Debug for FinalizerEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("FinalizerEntry").finish()
    }
}

/// The outcome of processing a single NATS message.
pub enum ProcessingStatus {
    /// The message payload was fully decoded and sent downstream.
    Success,
    /// A non-recoverable error occurred while decoding the payload.
    Failed,
    /// The downstream channel is closed, and the source should shut down.
    ChannelClosed,
}

/// Processes a single NATS message, sending decoded events downstream.
///
/// This function contains the common logic for both Core and JetStream NATS.
pub async fn process_message(
    msg: &async_nats::Message,
    config: &NatsSourceConfig,
    decoder: &Decoder,
    log_namespace: LogNamespace,
    out: &mut SourceSender,
    events_received: &EventsReceivedHandle,
    batch: &Option<BatchNotifier>,
) -> ProcessingStatus {
    let mut framed = DecoderFramedRead::new(msg.payload.as_ref(), decoder.clone());
    let mut success = true;

    while let Some(next) = framed.next().await {
        match next {
            Ok((events, _byte_size)) => {
                let count = events.len();
                if count == 0 {
                    continue;
                }

                let byte_size = events.estimated_json_encoded_size_of();
                events_received.emit(CountByteSize(count, byte_size));
                let now = Utc::now();
                let events = events.into_iter().map(|mut event| {
                    if let Event::Log(ref mut log) = event {
                        log_namespace.insert_standard_vector_source_metadata(
                            log,
                            NatsSourceConfig::NAME,
                            now,
                        );
                        let legacy_subject_key_field = config
                            .subject_key_field
                            .path
                            .as_ref()
                            .map(LegacyKey::InsertIfEmpty);
                        log_namespace.insert_source_metadata(
                            NatsSourceConfig::NAME,
                            log,
                            legacy_subject_key_field,
                            &owned_value_path!("subject"),
                            msg.subject.as_str(),
                        );
                    }
                    event.with_batch_notifier_option(batch)
                });

                if out.send_batch(events).await.is_err() {
                    emit!(StreamClosedError { count });
                    return ProcessingStatus::ChannelClosed;
                }
            }
            Err(error) => {
                success = false;
                // Error is logged by `vector_lib::codecs::Decoder`, no further
                // handling is needed here.
                if !error.can_continue() {
                    break;
                }
            }
        }
    }

    if success {
        ProcessingStatus::Success
    } else {
        ProcessingStatus::Failed
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_nats_jetstream(
    config: NatsSourceConfig,
    _connection: async_nats::Client,
    stream: PullConsumerStream,
    decoder: Decoder,
    log_namespace: LogNamespace,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
    acknowledgements: bool,
) -> Result<(), ()> {
    let (finalizer, mut ack_stream) =
        UnorderedFinalizer::<FinalizerEntry>::maybe_new(acknowledgements, Some(shutdown.clone()));
    let events_received = register!(EventsReceived);
    let bytes_received = register!(BytesReceived::from(Protocol::TCP));
    let mut message_stream = stream.take_until(shutdown);

    loop {
        tokio::select! {
            entry = ack_stream.next() => {
                if let Some((status, entry)) = entry {
                    handle_ack(status, entry).await;
                }
            },
            message = message_stream.next() => match message {
                Some(Ok(msg)) => {
                    bytes_received.emit(ByteSize(msg.payload.len()));
                    let (batch, receiver) =
                        BatchNotifier::maybe_new_with_receiver(acknowledgements);

                    let status = process_message(
                        &msg,
                        &config,
                        &decoder,
                        log_namespace,
                        &mut out,
                        &events_received,
                        &batch,
                    )
                    .await;

                    match status {
                        ProcessingStatus::Success => match receiver {
                            Some(receiver) => {
                                let (_, acker) = msg.split();
                                finalizer
                                    .as_ref()
                                    .expect("Finalizer must exist when acknowledgements are enabled")
                                    .add(FinalizerEntry(acker), receiver);
                            }
                            None => {
                                if let Err(error) = msg.ack().await {
                                    error!(message = "Failed to acknowledge JetStream message.", %error);
                                }
                            }
                        },
                        ProcessingStatus::Failed => {
                            // Do not acknowledge on failure; the message will be redelivered.
                        }
                        ProcessingStatus::ChannelClosed => {
                            // Downstream channel is closed, shut down the source.
                            return Err(());
                        }
                    }
                }
                Some(Err(error)) => {
                    error!(message = "Failed to read JetStream message.", %error);
                }
                None => break,
            },
        }
    }
    Ok(())
}

async fn handle_ack(status: BatchStatus, entry: FinalizerEntry) {
    let result = match status {
        BatchStatus::Delivered => entry.0.ack().await,
        BatchStatus::Errored | BatchStatus::Rejected => entry.0.ack_with(AckKind::Nak(None)).await,
    };

    if let Err(error) = result {
        error!(message = "Failed to acknowledge JetStream message.", %error);
    }
}

pub async fn run_nats_core(
    config: NatsSourceConfig,
    _connection: async_nats::Client,
    mut subscriber: async_nats::Subscriber,
    decoder: Decoder,
    log_namespace: LogNamespace,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let events_received = register!(EventsReceived);
    let bytes_received = register!(BytesReceived::from(Protocol::TCP));

    loop {
        tokio::select! {
            biased;

             _ = &mut shutdown => {
                info!("Shutdown signal received. Draining NATS subscription...");
                if let Err(err) = subscriber.drain().await {
                    error!(message = "Failed to drain NATS subscription.", %err);
                }
            },

            maybe_msg = subscriber.next() => {
                match maybe_msg {
                    Some(msg) => {
                        bytes_received.emit(ByteSize(msg.payload.len()));
                        let status = process_message(
                            &msg,
                            &config,
                            &decoder,
                            log_namespace,
                            &mut out,
                            &events_received,
                            &None,
                        )
                        .await;

                        if let ProcessingStatus::ChannelClosed = status {
                            return Err(());
                        }
                    },
                    None => {
                        // The stream has ended. This happens naturally after a successful
                        // drain or if the connection is lost.
                        break;
                    }
                }
            }
        }
    }

    info!("NATS source drained and shut down gracefully.");
    Ok(())
}

pub async fn create_subscription(
    config: &NatsSourceConfig,
) -> Result<(async_nats::Client, async_nats::Subscriber), BuildError> {
    let nc = config.connect().await?;

    let subscription = match &config.queue {
        None => nc.subscribe(config.subject.clone()).await,
        Some(queue) => {
            nc.queue_subscribe(config.subject.clone(), queue.clone())
                .await
        }
    };

    let subscription = subscription.context(SubscribeSnafu)?;

    Ok((nc, subscription))
}
