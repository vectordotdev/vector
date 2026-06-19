use async_nats::jetstream::consumer::pull::Stream as PullConsumerStream;
use bytes::Bytes;
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
use vrl::value::{ObjectMap, Value};

use crate::{
    SourceSender,
    codecs::Decoder,
    event::Event,
    internal_events::StreamClosedError,
    shutdown::ShutdownSignal,
    sources::nats::config::{BuildError, NatsSourceConfig, SubscribeSnafu},
};

/// Converts a NATS [`HeaderMap`] into a [`Value`] suitable for insertion into an event.
///
/// Each header name maps to an array of its string values, since NATS headers
/// can be multi-valued.
fn headers_to_value(headers: &async_nats::HeaderMap) -> Value {
    let mut map = ObjectMap::new();
    for (name, values) in headers.iter() {
        let values = values
            .iter()
            .map(|value| Value::from(Bytes::copy_from_slice(value.as_str().as_bytes())))
            .collect::<Vec<_>>();
        map.insert(name.to_string().into(), Value::Array(values));
    }
    Value::Object(map)
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

                        if let Some(headers_key) = config.headers_key.path.as_ref()
                            && let Some(headers) = msg.headers.as_ref()
                        {
                            log_namespace.insert_source_metadata(
                                NatsSourceConfig::NAME,
                                log,
                                Some(LegacyKey::Overwrite(headers_key)),
                                &owned_value_path!("headers"),
                                headers_to_value(headers),
                            );
                        }
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

pub async fn run_nats_jetstream(
    config: NatsSourceConfig,
    _connection: async_nats::Client,
    stream: PullConsumerStream,
    decoder: Decoder,
    log_namespace: LogNamespace,
    shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let events_received = register!(EventsReceived);
    let bytes_received = register!(BytesReceived::from(Protocol::TCP));
    let mut message_stream = stream.take_until(shutdown);

    while let Some(Ok(msg)) = message_stream.next().await {
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
                // Message processed successfully, acknowledge it.
                if let Err(err) = msg.ack().await {
                    error!(message = "Failed to acknowledge JetStream message.", %err);
                }
            }
            ProcessingStatus::Failed => {
                // Do not acknowledge on failure; the message will be redelivered.
            }
            ProcessingStatus::ChannelClosed => {
                // Downstream channel is closed, shut down the source.
                return Err(());
            }
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_to_value_maps_names_to_arrays_of_values() {
        let mut headers = async_nats::HeaderMap::new();
        headers.insert("X-My-Custom-Header", "A");
        headers.insert("X-My-Custom-Timestamp", "1771517040123456000");

        let value = headers_to_value(&headers);

        let mut expected = ObjectMap::new();
        expected.insert(
            "X-My-Custom-Header".into(),
            Value::Array(vec![Value::from("A")]),
        );
        expected.insert(
            "X-My-Custom-Timestamp".into(),
            Value::Array(vec![Value::from("1771517040123456000")]),
        );

        assert_eq!(value, Value::Object(expected));
    }

    #[test]
    fn headers_to_value_collects_multiple_values_per_name() {
        let mut headers = async_nats::HeaderMap::new();
        headers.append("X-Multi", "one");
        headers.append("X-Multi", "two");

        let value = headers_to_value(&headers);

        let object = value.as_object().expect("headers should be an object");
        let values = object
            .get("X-Multi")
            .and_then(|v| v.as_array())
            .expect("X-Multi should be an array");

        assert_eq!(values.len(), 2);
        assert!(values.contains(&Value::from("one")));
        assert!(values.contains(&Value::from("two")));
    }

    #[test]
    fn headers_to_value_empty_map_is_empty_object() {
        let headers = async_nats::HeaderMap::new();
        let value = headers_to_value(&headers);
        assert_eq!(value, Value::Object(ObjectMap::new()));
    }
}
