use std::sync::Arc;

use bytes::{BufMut, Bytes, BytesMut};
use chrono::Utc;
use http::StatusCode;
use tokio_util::codec::Decoder;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::StreamDecodingError,
    config::LegacyKey,
    internal_event::{CountByteSize, InternalEventHandle as _},
    json_size::JsonSize,
    lookup::path,
};
use vrl::core::Value;
use warp::{Filter, filters::BoxedFilter, path as warp_path, path::FullPath, reply::Response};

use super::{ApiKeyQueryParams, DatadogAgentConfig, DatadogAgentSource, LogMsg, RequestHandler};
use crate::{
    common::{datadog::DDTAGS, http::ErrorMessage},
    event::Event,
    internal_events::DatadogAgentJsonParseError,
};

pub(super) fn build_warp_filter(
    handler: RequestHandler,
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
                handler.clone().handle_request(events, super::LOGS)
            },
        )
        .boxed()
}

pub(crate) fn decode_log_body(
    body: Bytes,
    api_key: Option<Arc<str>>,
    source: &DatadogAgentSource,
) -> Result<Vec<Event>, ErrorMessage> {
    if body.is_empty() || body.as_ref() == b"{}" {
        // The datadog agent may send an empty payload as a keep alive
        // https://github.com/DataDog/datadog-agent/blob/5a6c5dd75a2233fbf954e38ddcc1484df4c21a35/pkg/logs/client/http/destination.go#L52
        debug!(message = "Empty payload ignored.");
        return Ok(Vec::new());
    }

    let messages: Vec<LogMsg> = serde_json::from_slice(&body).map_err(|error| {
        emit!(DatadogAgentJsonParseError { error: &error });

        ErrorMessage::new(
            StatusCode::BAD_REQUEST,
            format!("Error parsing JSON: {error:?}"),
        )
    })?;

    let now = Utc::now();
    let mut decoded = Vec::new();
    let mut event_bytes_received = JsonSize::zero();

    for LogMsg {
        message,
        status,
        timestamp,
        hostname,
        service,
        ddsource,
        ddtags,
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
                            let namespace = &source.log_namespace;
                            let source_name = "datadog_agent";

                            namespace.insert_source_metadata(
                                source_name,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!("status"))),
                                path!("status"),
                                status.clone(),
                            );
                            namespace.insert_source_metadata(
                                source_name,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!("timestamp"))),
                                path!("timestamp"),
                                timestamp,
                            );
                            namespace.insert_source_metadata(
                                source_name,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!("hostname"))),
                                path!("hostname"),
                                hostname.clone(),
                            );
                            namespace.insert_source_metadata(
                                source_name,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!("service"))),
                                path!("service"),
                                service.clone(),
                            );
                            namespace.insert_source_metadata(
                                source_name,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!("ddsource"))),
                                path!("ddsource"),
                                ddsource.clone(),
                            );

                            let ddtags: Value = if source.parse_ddtags {
                                parse_ddtags(&ddtags)
                            } else {
                                ddtags.clone().into()
                            };

                            namespace.insert_source_metadata(
                                source_name,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!(DDTAGS))),
                                path!(DDTAGS),
                                ddtags,
                            );

                            // compute EstimatedJsonSizeOf before enrichment
                            event_bytes_received += log.estimated_json_encoded_size_of();

                            namespace.insert_standard_vector_source_metadata(
                                log,
                                DatadogAgentConfig::NAME,
                                now,
                            );

                            if let Some(k) = &api_key {
                                log.metadata_mut().set_datadog_api_key(Arc::clone(k));
                            }

                            let logs_schema_definition = source
                                .logs_schema_definition
                                .as_ref()
                                .unwrap_or_else(|| panic!("registered log schema required"));

                            log.metadata_mut()
                                .set_schema_definition(logs_schema_definition);
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

    source
        .events_received
        .emit(CountByteSize(decoded.len(), event_bytes_received));

    Ok(decoded)
}

// ddtags input is a string containing a list of tags which
// can include both bare tags and key-value pairs.
// the tag list members are separated by `,` and the
// tag-value pairs are separated by `:`.
//
// The output is an Array regardless of the input string.
fn parse_ddtags(ddtags_raw: &Bytes) -> Value {
    if ddtags_raw.is_empty() {
        return Vec::<Value>::new().into();
    }

    let ddtags_str = String::from_utf8_lossy(ddtags_raw);

    // There are multiple tags, which could be either bare or pairs
    let ddtags: Vec<Value> = ddtags_str
        .split(',')
        .filter(|kv| !kv.is_empty())
        .map(|kv| Value::Bytes(Bytes::from(kv.trim().to_string())))
        .collect();

    if ddtags.is_empty() && !ddtags_str.is_empty() {
        warn!(
            message = "`parse_ddtags` set to true and Agent log contains non-empty ddtags string, but no tag-value pairs were parsed."
        )
    }

    ddtags.into()
}

#[cfg(test)]
mod tests {
    use similar_asserts::assert_eq;
    use vrl::value;

    use super::*;

    #[test]
    fn ddtags_parse_empty() {
        let raw = Bytes::from(String::from(""));
        let val = parse_ddtags(&raw);

        assert_eq!(val, value!([]));
    }

    #[test]
    fn ddtags_parse_bare() {
        let raw = Bytes::from(String::from("bare"));
        let val = parse_ddtags(&raw);

        assert_eq!(val, value!(["bare"]));
    }

    #[test]
    fn ddtags_parse_kv_one() {
        let raw = Bytes::from(String::from("filename:driver.log"));
        let val = parse_ddtags(&raw);

        assert_eq!(val, value!(["filename:driver.log"]));
    }

    #[test]
    fn ddtags_parse_kv_multi() {
        let raw = Bytes::from(String::from("filename:driver.log,wizard:the_grey"));
        let val = parse_ddtags(&raw);

        assert_eq!(val, value!(["filename:driver.log", "wizard:the_grey"]));
    }

    #[test]
    fn ddtags_parse_kv_bare_combo() {
        let raw = Bytes::from(String::from("filename:driver.log,debug,wizard:the_grey"));
        let val = parse_ddtags(&raw);

        assert_eq!(
            val,
            value!(["filename:driver.log", "debug", "wizard:the_grey"])
        );
    }
}
