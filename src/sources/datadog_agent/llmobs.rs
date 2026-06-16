use std::sync::Arc;

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use http::StatusCode;
use serde::Deserialize;
use serde_json::Value;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    internal_event::{CountByteSize, InternalEventHandle as _},
    json_size::JsonSize,
    lookup::event_path,
};
use warp::{Filter, filters::BoxedFilter, path, path::FullPath, reply::Response};

use super::{ApiKeyQueryParams, DatadogAgentConfig, DatadogAgentSource, RequestHandler};
use crate::{
    common::http::ErrorMessage,
    config::log_schema,
    event::{Event, LogEvent},
    internal_events::DatadogAgentJsonParseError,
};

pub(super) fn build_warp_filter(
    handler: RequestHandler,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    let direct = warp::post()
        .and(path!("api" / "v2" / "llmobs" / ..))
        .and(warp::path::full())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::header::optional::<String>("dd-api-key"))
        .and(warp::query::<ApiKeyQueryParams>())
        .and(warp::body::bytes())
        .and_then({
            let handler = handler.clone();
            let source = source.clone();
            move |path: FullPath,
                  encoding_header: Option<String>,
                  api_token: Option<String>,
                  query_params: ApiKeyQueryParams,
                  body: Bytes| {
                let events = source
                    .decode(&encoding_header, body, path.as_str())
                    .and_then(|body| {
                        decode_llmobs_body(
                            body,
                            source.api_key_extractor.extract(
                                path.as_str(),
                                api_token,
                                query_params.dd_api_key,
                            ),
                            &source,
                        )
                    });
                handler.clone().handle_request(events, super::LLMOBS)
            }
        });

    let evp_proxy = warp::post()
        .and(path!("evp_proxy" / "v2" / "api" / "v2" / "llmobs" / ..))
        .and(warp::path::full())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::header::optional::<String>("dd-api-key"))
        .and(warp::query::<ApiKeyQueryParams>())
        .and(warp::body::bytes())
        .and_then(
            move |path: FullPath,
                  encoding_header: Option<String>,
                  api_token: Option<String>,
                  query_params: ApiKeyQueryParams,
                  body: Bytes| {
                let events = source
                    .decode(&encoding_header, body, path.as_str())
                    .and_then(|body| {
                        decode_llmobs_body(
                            body,
                            source.api_key_extractor.extract(
                                path.as_str(),
                                api_token,
                                query_params.dd_api_key,
                            ),
                            &source,
                        )
                    });
                handler.clone().handle_request(events, super::LLMOBS)
            },
        );

    direct.or(evp_proxy).unify().boxed()
}

#[derive(Deserialize)]
struct LLMObsEnvelopeItem {
    #[serde(rename = "event_type")]
    _event_type: Option<String>,
    spans: Vec<LLMObsSpan>,
    #[serde(rename = "_dd.tracer_version")]
    dd_tracer_version: Option<String>,
    #[serde(rename = "_dd.scope")]
    _dd_scope: Option<String>,
}

#[derive(Deserialize)]
struct LLMObsSpan {
    span_id: String,
    trace_id: String,
    parent_id: Option<String>,
    name: Option<String>,
    session_id: Option<String>,
    service: Option<String>,
    start_ns: Option<i64>,
    duration: Option<i64>,
    status: Option<String>,
    status_message: Option<String>,
    meta: Option<Value>,
    metrics: Option<Value>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(rename = "_dd")]
    dd: Option<Value>,
    span_links: Option<Value>,
    #[serde(rename = "config")]
    config: Option<Value>,
    collection_errors: Option<Value>,
}

pub(crate) fn decode_llmobs_body(
    body: Bytes,
    api_key: Option<Arc<str>>,
    source: &DatadogAgentSource,
) -> Result<Vec<Event>, ErrorMessage> {
    let envelope: Vec<LLMObsEnvelopeItem> = serde_json::from_slice(&body).map_err(|error| {
        emit!(DatadogAgentJsonParseError { error: &error });
        ErrorMessage::new(
            StatusCode::BAD_REQUEST,
            format!("Error parsing JSON: {error:?}"),
        )
    })?;

    let now = Utc::now();
    let mut event_bytes_received = JsonSize::zero();

    let events: Vec<Event> = envelope
        .into_iter()
        .flat_map(|item| {
            let tracer_version = item.dd_tracer_version.clone();
            item.spans.into_iter().map(move |span| {
                let mut log = LogEvent::default();
                log.insert("span_id", span.span_id);
                log.insert("trace_id", span.trace_id);
                if let Some(v) = span.parent_id {
                    log.insert("parent_id", v);
                }
                if let Some(v) = span.name {
                    log.insert("name", v);
                }
                if let Some(v) = span.session_id {
                    log.insert("session_id", v);
                }
                if let Some(v) = span.service {
                    log.insert("service", v);
                }
                if let Some(ns) = span.start_ns {
                    log.insert("start_ns", ns);
                    if let Some(ts_path) = log_schema().timestamp_key_target_path() {
                        log.insert(ts_path, Utc.timestamp_nanos(ns));
                    }
                }
                if let Some(v) = span.duration {
                    log.insert("duration", v);
                }
                if let Some(v) = span.status {
                    log.insert("status", v);
                }
                if let Some(v) = span.status_message {
                    log.insert("status_message", v);
                }
                if let Some(v) = span.meta {
                    log.insert("meta", v);
                }
                if let Some(v) = span.metrics {
                    log.insert("metrics", v);
                }
                if !span.tags.is_empty() {
                    log.insert("tags", span.tags.clone());
                }
                if let Some(v) = span.span_links {
                    log.insert("span_links", v);
                }
                if let Some(v) = span.config {
                    log.insert("config", v);
                }
                if let Some(v) = span.collection_errors {
                    log.insert("collection_errors", v);
                }

                // Extract ml_app: first check span._dd.ml_app, then fall back to tags array.
                let ml_app = span
                    .dd
                    .as_ref()
                    .and_then(|dd| dd.get("ml_app"))
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                    .or_else(|| {
                        span.tags
                            .iter()
                            .find_map(|tag| tag.strip_prefix("ml_app:").map(str::to_owned))
                    });
                if let Some(app) = ml_app {
                    log.insert("ml_app", app);
                }

                if let Some(v) = tracer_version.clone() {
                    log.insert(event_path!("_dd", "tracer_version"), v);
                }

                Event::Log(log)
            })
        })
        .map(|mut event| {
            if let Event::Log(ref mut log) = event {
                event_bytes_received += log.estimated_json_encoded_size_of();
                source.log_namespace.insert_standard_vector_source_metadata(
                    log,
                    DatadogAgentConfig::NAME,
                    now,
                );
                if let Some(k) = &api_key {
                    log.metadata_mut().set_datadog_api_key(Arc::clone(k));
                }
            }
            event
        })
        .collect();

    source
        .events_received
        .emit(CountByteSize(events.len(), event_bytes_received));

    Ok(events)
}
