use super::errors::{ParseRecords, RequestError};
use super::models::{EncodedFirehoseRecord, FirehoseRequest, FirehoseResponse};
use super::Compression;
use crate::{
    config::log_schema,
    event::Event,
    internal_events::{
        AwsKinesisFirehoseAutomaticRecordDecodeError, AwsKinesisFirehoseEventReceived,
    },
    Pipeline,
};
use bytes::Bytes;
use chrono::Utc;
use flate2::read::MultiGzDecoder;
use futures::{SinkExt, StreamExt, TryFutureExt};
use snafu::{ResultExt, Snafu};
use std::io::Read;
use warp::reject;

/// Publishes decoded events from the FirehoseRequest to the pipeline
pub async fn firehose(
    request_id: String,
    source_arn: String,
    request: FirehoseRequest,
    record_compression: Compression,
    mut out: Pipeline,
) -> Result<impl warp::Reply, reject::Rejection> {
    let events = parse_records(
        record_compression,
        request,
        request_id.as_str(),
        source_arn.as_str(),
    )
    .with_context(|| ParseRecords {
        request_id: request_id.clone(),
    })
    .map_err(reject::custom)?;
    let mut stream = futures::stream::iter(events).map(Ok);

    let request_id = request_id.clone();
    out.send_all(&mut stream)
        .map_err(|error| {
            let error = RequestError::ShuttingDown {
                request_id: request_id.clone(),
                source: error,
            };
            // can only fail if receiving end disconnected, so we are shutting down,
            // probably not gracefully.
            error!(message = "Failed to forward events, downstream is closed.");
            error!(message = "Tried to send the following event.", %error);
            warp::reject::custom(error)
        })
        .map_ok(|_| {
            warp::reply::json(&FirehoseResponse {
                request_id: request_id.clone(),
                timestamp: Utc::now(),
                error_message: None,
            })
        })
        .await
}

/// Parses out events from the FirehoseRequest
fn parse_records(
    compression: Compression,
    request: FirehoseRequest,
    request_id: &str,
    source_arn: &str,
) -> Result<Vec<Event>, RecordDecodeError> {
    request
        .records
        .iter()
        .map(|record| {
            decode_record(record, compression).map(|record| {
                emit!(&AwsKinesisFirehoseEventReceived {
                    byte_size: record.len()
                });

                let mut event = Event::new_empty_log();
                let log = event.as_mut_log();

                log.insert(log_schema().message_key(), record);
                log.insert(log_schema().timestamp_key(), request.timestamp);
                log.insert("request_id", request_id.to_string());
                log.insert("source_arn", source_arn.to_string());

                event
            })
        })
        .collect()
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
