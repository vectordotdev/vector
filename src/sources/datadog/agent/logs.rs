use std::sync::Arc;

use bytes::{BufMut, Bytes, BytesMut};
use chrono::Utc;
use codecs::StreamDecodingError;
use http::StatusCode;
use tokio_util::codec::Decoder;
use vector_core::ByteSizeOf;
use warp::{filters::BoxedFilter, path as warp_path, path::FullPath, reply::Response, Filter};

use crate::{
    event::Event,
    internal_events::EventsReceived,
    sources::{
        datadog::agent::{self, handle_request, ApiKeyQueryParams, DatadogAgentSource, LogMsg},
        util::ErrorMessage,
    },
    SourceSender,
};
use lookup::path;

pub(crate) fn build_warp_filter(
    acknowledgements: bool,
    multiple_outputs: bool,
    out: SourceSender,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(warp_path!("v1" / "input" / ..).or(warp_path!("api" / "v2" / "logs" / ..)))
        .and(warp::path::full())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::header::optional::<String>("dd-api-key"))
        .and(warp::query::<ApiKeyQueryParams>())
        .and(warp::body::bytes())
        .and_then(
            move |_,
                  path: FullPath,
                  encoding_header: Option<String>,
                  api_token: Option<String>,
                  query_params: ApiKeyQueryParams,
                  body: Bytes| {
                let events = source
                    .decode(&encoding_header, body, path.as_str())
                    .and_then(|body| {
                        decode_log_body(
                            body,
                            source.api_key_extractor.extract(
                                path.as_str(),
                                api_token,
                                query_params.dd_api_key,
                            ),
                            &source,
                        )
                    });

                if multiple_outputs {
                    handle_request(events, acknowledgements, out.clone(), Some(agent::LOGS))
                } else {
                    handle_request(events, acknowledgements, out.clone(), None)
                }
            },
        )
        .boxed()
}

pub(crate) fn decode_log_body(
    body: Bytes,
    api_key: Option<Arc<str>>,
    source: &DatadogAgentSource,
) -> Result<Vec<Event>, ErrorMessage> {
    if body.is_empty() {
        // The datadog agent may send an empty payload as a keep alive
        debug!(
            message = "Empty payload ignored.",
            internal_log_rate_secs = 30
        );
        return Ok(Vec::new());
    }

    let messages: Vec<LogMsg> = serde_json::from_slice(&body).map_err(|error| {
        ErrorMessage::new(
            StatusCode::BAD_REQUEST,
            format!("Error parsing JSON: {:?}", error),
        )
    })?;

    let now = Utc::now();
    let mut decoded = Vec::new();

    for LogMsg {
        message,
        timestamp,
        hostname,
        service,
        ddsource,
        ddtags,
        ..
    } in messages
    {
        let mut decoder = source.decoder.clone();
        let mut buffer = BytesMut::new();
        buffer.put(message);
        loop {
            match decoder.decode_eof(&mut buffer) {
                Ok(Some((events, _byte_size))) => {
                    for mut event in events {
                        if let Event::Log(ref mut log) = event {
                            log.try_insert(path!("timestamp"), timestamp);
                            log.try_insert(path!("hostname"), hostname.clone());
                            log.try_insert(path!("service"), service.clone());
                            log.try_insert(path!("ddsource"), ddsource.clone());
                            log.try_insert(path!("ddtags"), ddtags.clone());
                            log.try_insert(
                                path!(source.log_schema_source_type_key),
                                Bytes::from("datadog_agent"),
                            );
                            log.try_insert(path!(source.log_schema_timestamp_key), now);
                            if let Some(k) = &api_key {
                                log.metadata_mut().set_datadog_api_key(Arc::clone(k));
                            }

                            log.metadata_mut()
                                .set_schema_definition(&source.logs_schema_definition);
                        }

                        decoded.push(event);
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    // Error is logged by `crate::codecs::Decoder`, no further
                    // handling is needed here.
                    if !error.can_continue() {
                        break;
                    }
                }
            }
        }
    }

    emit!(EventsReceived {
        byte_size: decoded.size_of(),
        count: decoded.len(),
    });

    Ok(decoded)
}
