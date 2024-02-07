use std::sync::Arc;

use bytes::{BufMut, Bytes, BytesMut};
use chrono::Utc;
use http::StatusCode;
use tokio_util::codec::Decoder;
use vector_lib::codecs::StreamDecodingError;
use vector_lib::internal_event::{CountByteSize, InternalEventHandle as _};
use vector_lib::lookup::path;
use vector_lib::{config::LegacyKey, EstimatedJsonEncodedSizeOf};
use warp::{filters::BoxedFilter, path as warp_path, path::FullPath, reply::Response, Filter};

use crate::{
    event::Event,
    sources::{
        datadog_agent::{
            handle_request, ApiKeyQueryParams, DatadogAgentConfig, DatadogAgentSource, LogMsg,
        },
        util::ErrorMessage,
    },
    SourceSender,
};

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

                let output = multiple_outputs.then_some(super::LOGS);
                handle_request(events, acknowledgements, out.clone(), output)
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
        debug!(
            message = "Empty payload ignored.",
            internal_log_rate_limit = true
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
                            namespace.insert_source_metadata(
                                source_name,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!("ddtags"))),
                                path!("ddtags"),
                                ddtags.clone(),
                            );

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

    source.events_received.emit(CountByteSize(
        decoded.len(),
        decoded.estimated_json_encoded_size_of(),
    ));

    Ok(decoded)
}
