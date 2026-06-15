use bytes::Bytes;
use http::StatusCode;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use warp::{Filter, filters::BoxedFilter, path, path::FullPath, reply::Response};

use super::{ApiKeyQueryParams, DatadogAgentSource, RequestHandler};
use crate::{
    common::http::ErrorMessage,
    event::{Event, LogEvent},
    internal_events::DatadogAgentJsonParseError,
};

pub(super) fn build_warp_filter(
    handler: RequestHandler,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(path!("api" / "v2" / "llmobs" / ..))
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
                        )
                    });
                handler.clone().handle_request(events, super::LLMOBS)
            },
        )
        .boxed()
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
}

pub(crate) fn decode_llmobs_body(
    body: Bytes,
    api_key: Option<Arc<str>>,
) -> Result<Vec<Event>, ErrorMessage> {
    let envelope: Vec<LLMObsEnvelopeItem> = serde_json::from_slice(&body).map_err(|error| {
        emit!(DatadogAgentJsonParseError { error: &error });
        ErrorMessage::new(
            StatusCode::BAD_REQUEST,
            format!("Error parsing JSON: {error:?}"),
        )
    })?;

    let events = envelope
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
                if let Some(v) = span.start_ns {
                    log.insert("start_ns", v);
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
                    log.insert("tags", span.tags);
                }
                if let Some(ml_app) = span
                    .dd
                    .as_ref()
                    .and_then(|dd| dd.get("ml_app"))
                    .and_then(|v| v.as_str())
                {
                    log.insert("ml_app", ml_app.to_owned());
                }
                if let Some(v) = tracer_version.clone() {
                    log.insert("_dd.tracer_version", v);
                }
                Event::Log(log)
            })
        })
        .map(|mut event| {
            if let Some(k) = &api_key {
                event.metadata_mut().set_datadog_api_key(Arc::clone(k));
            }
            event
        })
        .collect();

    Ok(events)
}
