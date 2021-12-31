use std::{io::Read, sync::Arc};

use bytes::Bytes;
use chrono::Utc;
use flate2::read::MultiGzDecoder;
use futures::{SinkExt, StreamExt, TryFutureExt};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::FramedRead;
use vector_core::event::BatchNotifier;
use warp::reject;

use super::{
    errors::{ParseRecords, RequestError},
    models::{EncodedFirehoseRecord, FirehoseRequest, FirehoseResponse},
    Compression,
};
use crate::{
    codecs,
    config::log_schema,
    event::{BatchStatus, Event},
    internal_events::{
        AwsKinesisFirehoseAutomaticRecordDecodeError, AwsKinesisFirehoseEventsReceived,
    },
    sources::util::StreamDecodingError,
    Pipeline,
};

/// Publishes decoded events from the FirehoseRequest to the pipeline
pub async fn firehose(
    request_id: String,
    source_arn: String,
    request: FirehoseRequest,
    compression: Compression,
    decoder: codecs::Decoder,
    acknowledgements: bool,
    mut out: Pipeline,
) -> Result<impl warp::Reply, reject::Rejection> {
    for record in request.records {
        let bytes = decode_record(&record, compression)
            .with_context(|| ParseRecords {
                request_id: request_id.clone(),
            })
            .map_err(reject::custom)?;

        let mut stream = FramedRead::new(bytes.as_ref(), decoder.clone());
        loop {
            match stream.next().await {
                Some(Ok((events, byte_size))) => {
                    emit!(&AwsKinesisFirehoseEventsReceived {
                        count: events.len(),
                        byte_size
                    });

                    let (batch, receiver) = acknowledgements
                        .then(|| {
                            let (batch, receiver) = BatchNotifier::new_with_receiver();
                            (Some(batch), Some(receiver))
                        })
                        .unwrap_or((None, None));

                    for mut event in events {
                        if let Some(batch) = &batch {
                            event.add_batch_notifier(Arc::clone(batch));
                        }
                        if let Event::Log(ref mut log) = event {
                            log.try_insert(
                                log_schema().source_type_key(),
                                Bytes::from("aws_kinesis_firehose"),
                            );
                            log.try_insert(log_schema().timestamp_key(), request.timestamp);
                            log.try_insert_flat("request_id", request_id.to_string());
                            log.try_insert_flat("source_arn", source_arn.to_string());
                        }

                        out.send(event)
                            .map_err(|error| {
                                let error = RequestError::ShuttingDown {
                                    request_id: request_id.clone(),
                                    source: error,
                                };
                                // can only fail if receiving end disconnected, so we are shutting
                                // down, probably not gracefully.
                                error!(message = "Failed to forward events, downstream is closed.");
                                error!(message = "Tried to send the following event.", %error);
                                warp::reject::custom(error)
                            })
                            .await?;
                    }

                    drop(batch);
                    if let Some(receiver) = receiver {
                        match receiver.await {
                            BatchStatus::Delivered => Ok(()),
                            BatchStatus::Rejected => {
                                Err(warp::reject::custom(RequestError::DeliveryFailed {
                                    request_id: request_id.clone(),
                                }))
                            }
                            BatchStatus::Errored => {
                                Err(warp::reject::custom(RequestError::DeliveryErrored {
                                    request_id: request_id.clone(),
                                }))
                            }
                        }?;
                    }
                }
                Some(Err(error)) => {
                    // Error is logged by `crate::codecs::Decoder`, no further
                    // handling is needed here.
                    if !error.can_continue() {
                        break;
                    }
                }
                None => break,
            }
        }
    }

    Ok(warp::reply::json(&FirehoseResponse {
        request_id: request_id.clone(),
        timestamp: Utc::now(),
        error_message: None,
    }))
}

#[derive(Debug, Snafu)]
pub enum RecordDecodeError {
    #[snafu(display("Could not base64 decode request data: {}", source))]
    Base64 { source: base64::DecodeError },
    #[snafu(display("Could not decompress request data as {}: {}", compression, source))]
    Decompression {
        source: std::io::Error,
        compression: Compression,
    },
}

/// Decodes a Firehose record.
fn decode_record(
    record: &EncodedFirehoseRecord,
    compression: Compression,
) -> Result<Bytes, RecordDecodeError> {
    let buf = base64::decode(record.data.as_bytes()).context(Base64 {})?;

    if buf.is_empty() {
        return Ok(Bytes::default());
    }

    match compression {
        Compression::None => Ok(Bytes::from(buf)),
        Compression::Gzip => decode_gzip(&buf[..]).with_context(|| Decompression {
            compression: compression.to_owned(),
        }),
        Compression::Auto => {
            match infer::get(&buf) {
                Some(filetype) => match filetype.mime_type() {
                    "application/gzip" => decode_gzip(&buf[..]).or_else(|error| {
                        emit!(&AwsKinesisFirehoseAutomaticRecordDecodeError {
                            compression: Compression::Gzip,
                            error
                        });
                        Ok(Bytes::from(buf))
                    }),
                    // only support gzip for now
                    _ => Ok(Bytes::from(buf)),
                },
                None => Ok(Bytes::from(buf)),
            }
        }
    }
}

fn decode_gzip(data: &[u8]) -> std::io::Result<Bytes> {
    let mut decoded = Vec::new();

    let mut gz = MultiGzDecoder::new(data);
    gz.read_to_end(&mut decoded)?;

    Ok(Bytes::from(decoded))
}
