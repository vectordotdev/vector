use super::errors::{ParseRecords, RequestError};
use super::models::{EncodedFirehoseRecord, FirehoseRequest, FirehoseResponse};
use crate::{config::log_schema, event::Event, Pipeline};
use bytes::Bytes;
use chrono::Utc;
use flate2::read::GzDecoder;
use futures::{SinkExt, StreamExt, TryFutureExt};
use snafu::ResultExt;
use std::io::Read;
use warp::reject;

/// Publishes decoded events from the FirehoseRequest to the pipeline
pub async fn firehose(
    request_id: String,
    source_arn: String,
    request: FirehoseRequest,
    mut out: Pipeline,
) -> Result<impl warp::Reply, reject::Rejection> {
    let events = parse_records(request, request_id.as_str(), source_arn.as_str())
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
    request: FirehoseRequest,
    request_id: &str,
    source_arn: &str,
) -> std::io::Result<Vec<Event>> {
    request
        .records
        .iter()
        .map(|record| {
            decode_record(record).map(|record| {
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

/// Decodes a Firehose record from its base64 gzip format
fn decode_record(record: &EncodedFirehoseRecord) -> std::io::Result<Bytes> {
    let mut cursor = std::io::Cursor::new(record.data.as_bytes());
    let base64decoder = base64::read::DecoderReader::new(&mut cursor, base64::STANDARD);

    let mut gz = GzDecoder::new(base64decoder);
    let mut buffer = Vec::new();
    gz.read_to_end(&mut buffer)?;

    Ok(Bytes::from(buffer))
}
