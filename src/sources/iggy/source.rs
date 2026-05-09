use chrono::Utc;
use futures::StreamExt;
use iggy::prelude::{IggyClient, IggyConsumer};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{DecoderFramedRead, decoding::StreamDecodingError},
    config::{LegacyKey, LogNamespace},
    lookup::owned_value_path,
};

use crate::{
    SourceSender,
    codecs::Decoder,
    event::Event,
    internal_events::{
        IggyBytesReceived, IggyEventsReceived, IggyOffsetUpdated, IggyReadError, StreamClosedError,
    },
    shutdown::ShutdownSignal,
    sources::iggy::config::IggySourceConfig,
};

pub async fn run_iggy_source(
    config: IggySourceConfig,
    _client: IggyClient,
    mut consumer: IggyConsumer,
    decoder: Decoder,
    log_namespace: LogNamespace,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    loop {
        tokio::select! {
            biased;

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
                                        event
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

                        if channel_closed {
                            return Err(());
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
