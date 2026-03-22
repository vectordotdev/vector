use async_nats::jetstream::consumer::PullConsumer;
use chrono::Utc;
use futures::StreamExt;
use snafu::ResultExt;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{DecoderFramedRead, decoding::StreamDecodingError},
    config::{LegacyKey, LogNamespace},
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, EventsReceived, EventsReceivedHandle,
        InternalEventHandle as _, Protocol,
    },
    lookup::owned_value_path,
};

use crate::{
    SourceSender,
    codecs::Decoder,
    common::backoff::ExponentialBackoff,
    event::Event,
    internal_events::StreamClosedError,
    shutdown::ShutdownSignal,
    sources::nats::config::{
        BuildError, ConsumerSnafu, JetStreamConfig, MessagesSnafu, NatsSourceConfig, StreamSnafu,
        SubscribeSnafu,
    },
};

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
                    event
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

pub(crate) async fn create_consumer_stream(
    connection: &async_nats::Client,
    js_config: &JetStreamConfig,
) -> Result<async_nats::jetstream::consumer::pull::Stream, BuildError> {
    let js = async_nats::jetstream::new(connection.clone());
    let stream = js.get_stream(&js_config.stream).await.context(StreamSnafu)?;
    let consumer: PullConsumer = stream
        .get_consumer(&js_config.consumer)
        .await
        .context(ConsumerSnafu)?;
    consumer
        .stream()
        .max_messages_per_batch(js_config.batch_config.batch)
        .max_bytes_per_batch(js_config.batch_config.max_bytes)
        .messages()
        .await
        .context(MessagesSnafu)
}

pub async fn run_nats_jetstream(
    config: NatsSourceConfig,
    connection: async_nats::Client,
    initial_messages: async_nats::jetstream::consumer::pull::Stream,
    decoder: Decoder,
    log_namespace: LogNamespace,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let events_received = register!(EventsReceived);
    let bytes_received = register!(BytesReceived::from(Protocol::TCP));
    let mut backoff = ExponentialBackoff::default().max_delay(std::time::Duration::from_secs(30));

    let js_config = config
        .jetstream
        .as_ref()
        .expect("jetstream config must be present");

    let mut messages = initial_messages;

    loop {
        let mut message_stream = messages.take_until(&mut shutdown);

        while let Some(result) = message_stream.next().await {
            match result {
                Ok(msg) => {
                    backoff.reset();
                    bytes_received.emit(ByteSize(msg.payload.len()));

                    let status = process_message(
                        &msg,
                        &config,
                        &decoder,
                        log_namespace,
                        &mut out,
                        &events_received,
                    )
                    .await;

                    match status {
                        ProcessingStatus::Success => {
                            if let Err(err) = msg.ack().await {
                                error!(message = "Failed to acknowledge JetStream message.", %err);
                            }
                        }
                        ProcessingStatus::ChannelClosed => return Err(()),
                        // Do not acknowledge on failure; the message will be redelivered.
                        ProcessingStatus::Failed => {}
                    }
                }
                Err(err) => {
                    warn!(message = "JetStream consumer stream error, recreating.", %err);
                    break;
                }
            }
        }

        if futures::poll!(&mut shutdown).is_ready() {
            return Ok(());
        }

        // Reconnect: rebuild the consumer stream with backoff.
        // The durable consumer on the server tracks delivery state,
        // so we pick up where we left off.
        warn!(message = "JetStream pull stream terminated. Recovering consumer...");
        loop {
            let delay = backoff.next().expect("backoff never ends");
            tokio::select! {
                _ = &mut shutdown => return Ok(()),
                _ = tokio::time::sleep(delay) => {},
            }

            match create_consumer_stream(&connection, js_config).await {
                Ok(m) => {
                    messages = m;
                    backoff.reset();
                    break;
                }
                Err(err) => {
                    warn!(message = "Failed to recreate JetStream consumer stream, retrying.", %err);
                }
            }
        }
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
