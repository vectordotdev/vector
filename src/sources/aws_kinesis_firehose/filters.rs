use std::{convert::Infallible, io};

use bytes::{Buf, Bytes};
use chrono::Utc;
use flate2::read::MultiGzDecoder;
use snafu::ResultExt;
use vector_lib::config::LogNamespace;
use vector_lib::internal_event::{BytesReceived, Protocol};
use warp::{http::StatusCode, Filter};

use super::{
    errors::{ParseSnafu, RequestError},
    handlers,
    models::{FirehoseRequest, FirehoseResponse},
    Compression,
};
use crate::{
    codecs,
    internal_events::{AwsKinesisFirehoseRequestError, AwsKinesisFirehoseRequestReceived},
    SourceSender,
};

/// Handles routing of incoming HTTP requests from AWS Kinesis Firehose
pub fn firehose(
    access_keys: Vec<String>,
    store_access_key: bool,
    record_compression: Compression,
    decoder: codecs::Decoder,
    acknowledgements: bool,
    out: SourceSender,
    log_namespace: LogNamespace,
) -> impl Filter<Extract = (impl warp::Reply,), Error = Infallible> + Clone {
    let bytes_received = register!(BytesReceived::from(Protocol::HTTP));
    let context = handlers::Context {
        compression: record_compression,
        store_access_key,
        decoder,
        acknowledgements,
        bytes_received,
        out,
        log_namespace,
    };
    warp::post()
        .and(emit_received())
        .and(authenticate(access_keys))
        .and(warp::header("X-Amz-Firehose-Request-Id"))
        .and(warp::header("X-Amz-Firehose-Source-Arn"))
        .and(
            warp::header("X-Amz-Firehose-Protocol-Version")
                .and_then(|version: String| async move {
                    match version.as_str() {
                        "1.0" => Ok(()),
                        _ => Err(warp::reject::custom(
                            RequestError::UnsupportedProtocolVersion { version },
                        )),
                    }
                })
                .untuple_one(),
        )
        .and(parse_body())
        .and(warp::any().map(move || context.clone()))
        .and_then(handlers::firehose)
        .recover(handle_firehose_rejection)
}

/// Decode (if needed) and parse request body
///
/// Firehose can be configured to gzip compress messages so we handle this here
fn parse_body() -> impl Filter<Extract = (FirehoseRequest,), Error = warp::reject::Rejection> + Clone
{
    warp::any()
        .and(warp::header::optional::<String>("Content-Encoding"))
        .and(warp::header("X-Amz-Firehose-Request-Id"))
        .and(warp::body::bytes())
        .and_then(
            |encoding: Option<String>, request_id: String, body: Bytes| async move {
                match encoding {
                    Some(s) if s == "gzip" => {
                        Ok(Box::new(MultiGzDecoder::new(body.reader())) as Box<dyn io::Read>)
                    }
                    Some(s) => Err(warp::reject::Rejection::from(
                        RequestError::UnsupportedEncoding {
                            encoding: s,
                            request_id: request_id.clone(),
                        },
                    )),
                    None => Ok(Box::new(body.reader()) as Box<dyn io::Read>),
                }
                .and_then(|r| {
                    serde_json::from_reader(r)
                        .context(ParseSnafu {
                            request_id: request_id.clone(),
                        })
                        .map_err(warp::reject::custom)
                })
            },
        )
}

fn emit_received() -> impl Filter<Extract = (), Error = warp::reject::Rejection> + Clone {
    warp::any()
        .and(warp::header::optional("X-Amz-Firehose-Request-Id"))
        .and(warp::header::optional("X-Amz-Firehose-Source-Arn"))
        .map(|request_id: Option<String>, source_arn: Option<String>| {
            emit!(AwsKinesisFirehoseRequestReceived {
                request_id: request_id.as_deref(),
                source_arn: source_arn.as_deref(),
            });
        })
        .untuple_one()
}

/// If there is a configured access key, validate that the request key matches it
fn authenticate(
    configured_access_keys: Vec<String>,
) -> impl Filter<Extract = (), Error = warp::Rejection> + Clone {
    warp::any()
        .and(warp::header("X-Amz-Firehose-Request-Id"))
        .and(warp::header::optional("X-Amz-Firehose-Access-Key"))
        .and_then(move |request_id: String, access_key: Option<String>| {
            let configured_access_keys = configured_access_keys.clone();

            async move {
                match (access_key, configured_access_keys.is_empty()) {
                    // No configured access keys
                    (_, true) => Ok(()),
                    // Passed access key is present in configured access keys
                    (Some(access_key), false) if configured_access_keys.contains(&access_key) => {
                        Ok(())
                    }
                    // No configured access keys, but passed with the request
                    (Some(_), false) => Err(warp::reject::custom(RequestError::AccessKeyInvalid {
                        request_id,
                    })),
                    // Access keys are configured, but missing from the request
                    (None, false) => Err(warp::reject::custom(RequestError::AccessKeyMissing {
                        request_id,
                    })),
                }
            }
        })
        .untuple_one()
}

/// Maps RequestError and warp errors to AWS Kinesis Firehose response structure
async fn handle_firehose_rejection(err: warp::Rejection) -> Result<impl warp::Reply, Infallible> {
    let request_id: Option<&str>;
    let message: String;
    let code: StatusCode;

    if let Some(e) = err.find::<RequestError>() {
        message = e.to_string();
        code = e.status();
        request_id = e.request_id();
    } else if let Some(e) = err.find::<warp::reject::MissingHeader>() {
        code = StatusCode::BAD_REQUEST;
        message = format!("Required header missing: {}", e.name());
        request_id = None;
    } else {
        code = StatusCode::INTERNAL_SERVER_ERROR;
        message = format!("{:?}", err);
        request_id = None;
    }

    emit!(AwsKinesisFirehoseRequestError::new(
        code,
        message.as_str(),
        request_id
    ));

    let json = warp::reply::json(&FirehoseResponse {
        request_id: request_id.unwrap_or_default().to_string(),
        timestamp: Utc::now(),
        error_message: Some(message),
    });

    Ok(warp::reply::with_status(json, code))
}
