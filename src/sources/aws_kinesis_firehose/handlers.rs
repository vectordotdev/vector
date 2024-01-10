use std::io::Read;

use base64::prelude::{Engine as _, BASE64_STANDARD};
use bytes::Bytes;
use chrono::Utc;
use flate2::read::MultiGzDecoder;
use futures::StreamExt;
use snafu::{ResultExt, Snafu};
use tokio_util::codec::FramedRead;
use vector_lib::codecs::StreamDecodingError;
use vector_lib::lookup::{metadata_path, path, PathPrefix};
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    event::BatchNotifier,
    EstimatedJsonEncodedSizeOf,
};
use vector_lib::{
    finalization::AddBatchNotifier,
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Registered,
    },
};
use vrl::compiler::SecretTarget;
use warp::reject;

use super::{
    errors::{ParseRecordsSnafu, RequestError},
    models::{EncodedFirehoseRecord, FirehoseRequest, FirehoseResponse},
    Compression,
};
use crate::{
    codecs::Decoder,
    config::log_schema,
    event::{BatchStatus, Event},
    internal_events::{
        AwsKinesisFirehoseAutomaticRecordDecodeError, EventsReceived, StreamClosedError,
    },
    sources::aws_kinesis_firehose::AwsKinesisFirehoseConfig,
    SourceSender,
};

#[derive(Clone)]
pub(super) struct Context {
    pub(super) compression: Compression,
    pub(super) store_access_key: bool,
    pub(super) decoder: Decoder,
    pub(super) acknowledgements: bool,
    pub(super) bytes_received: Registered<BytesReceived>,
    pub(super) out: SourceSender,
    pub(super) log_namespace: LogNamespace,
}

/// Publishes decoded events from the FirehoseRequest to the pipeline
pub(super) async fn firehose(
    request_id: String,
    source_arn: String,
    request: FirehoseRequest,
    mut context: Context,
) -> Result<impl warp::Reply, reject::Rejection> {
    let log_namespace = context.log_namespace;
    let events_received = register!(EventsReceived);

    for record in request.records {
        let bytes = decode_record(&record, context.compression)
            .with_context(|_| ParseRecordsSnafu {
                request_id: request_id.clone(),
            })
            .map_err(reject::custom)?;
        context.bytes_received.emit(ByteSize(bytes.len()));

        let mut stream = FramedRead::new(bytes.as_ref(), context.decoder.clone());
        loop {
            match stream.next().await {
                Some(Ok((mut events, _byte_size))) => {
                    events_received.emit(CountByteSize(
                        events.len(),
                        events.estimated_json_encoded_size_of(),
                    ));

                    let (batch, receiver) = context
                        .acknowledgements
                        .then(|| {
                            let (batch, receiver) = BatchNotifier::new_with_receiver();
                            (Some(batch), Some(receiver))
                        })
                        .unwrap_or((None, None));

                    let now = Utc::now();
                    for event in &mut events {
                        if let Some(batch) = &batch {
                            event.add_batch_notifier(batch.clone());
                        }
                        if let Event::Log(ref mut log) = event {
                            log_namespace.insert_vector_metadata(
                                log,
                                log_schema().source_type_key(),
                                path!("source_type"),
                                Bytes::from_static(AwsKinesisFirehoseConfig::NAME.as_bytes()),
                            );
                            // This handles the transition from the original timestamp logic. Originally the
                            // `timestamp_key` was always populated by the `request.timestamp` time.
                            match log_namespace {
                                LogNamespace::Vector => {
                                    log.insert(metadata_path!("vector", "ingest_timestamp"), now);
                                    log.insert(
                                        metadata_path!(AwsKinesisFirehoseConfig::NAME, "timestamp"),
                                        request.timestamp,
                                    );
                                }
                                LogNamespace::Legacy => {
                                    if let Some(timestamp_key) = log_schema().timestamp_key() {
                                        log.try_insert(
                                            (PathPrefix::Event, timestamp_key),
                                            request.timestamp,
                                        );
                                    }
                                }
                            };

                            log_namespace.insert_source_metadata(
                                AwsKinesisFirehoseConfig::NAME,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!("request_id"))),
                                path!("request_id"),
                                request_id.to_owned(),
                            );
                            log_namespace.insert_source_metadata(
                                AwsKinesisFirehoseConfig::NAME,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!("source_arn"))),
                                path!("source_arn"),
                                source_arn.to_owned(),
                            );

                            if context.store_access_key {
                                if let Some(access_key) = &request.access_key {
                                    log.metadata_mut().secrets_mut().insert_secret(
                                        "aws_kinesis_firehose_access_key",
                                        access_key,
                                    );
                                }
                            }
                        }
                    }

                    let count = events.len();
                    if let Err(error) = context.out.send_batch(events).await {
                        emit!(StreamClosedError { count });
                        let error = RequestError::ShuttingDown {
                            request_id: request_id.clone(),
                            source: error,
                        };
                        warp::reject::custom(error);
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
    let buf = BASE64_STANDARD
        .decode(record.data.as_bytes())
        .context(Base64Snafu {})?;

    if buf.is_empty() {
        return Ok(Bytes::default());
    }

    match compression {
        Compression::None => Ok(Bytes::from(buf)),
        Compression::Gzip => decode_gzip(&buf[..]).with_context(|_| DecompressionSnafu {
            compression: compression.to_owned(),
        }),
        Compression::Auto => {
            match infer::get(&buf) {
                Some(filetype) => match filetype.mime_type() {
                    "application/gzip" => decode_gzip(&buf[..]).or_else(|error| {
                        emit!(AwsKinesisFirehoseAutomaticRecordDecodeError {
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
