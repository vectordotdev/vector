use crate::{
    event::{Event, LogEvent, Value},
    internal_events::EventsReceived,
    sources::datadog::agent::{self, handle_request, ApiKeyQueryParams, DatadogAgentSource},
    sources::util::ErrorMessage,
    vector_core::ByteSizeOf,
    SourceSender,
};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::future;
use http::StatusCode;
use prost::Message;
use std::collections::BTreeMap;
use std::sync::Arc;
use warp::{filters::BoxedFilter, path, path::FullPath, reply::Response, Filter, Rejection, Reply};

mod dd_proto {
    include!(concat!(env!("OUT_DIR"), "/dd_trace.rs"));
}

pub(crate) fn build_warp_filter(
    acknowledgements: bool,
    multiple_outputs: bool,
    out: SourceSender,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    build_trace_filter(acknowledgements, multiple_outputs, out, source)
        .or(build_stats_filter())
        .unify()
        .boxed()
}

fn build_trace_filter(
    acknowledgements: bool,
    multiple_outputs: bool,
    out: SourceSender,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(path!("api" / "v0.2" / "traces" / ..))
        .and(warp::path::full())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::header::optional::<String>("dd-api-key"))
        .and(warp::header::optional::<String>(
            "X-Datadog-Reported-Languages",
        ))
        .and(warp::query::<ApiKeyQueryParams>())
        .and(warp::body::bytes())
        .and_then(
            move |path: FullPath,
                  encoding_header: Option<String>,
                  api_token: Option<String>,
                  reported_language: Option<String>,
                  query_params: ApiKeyQueryParams,
                  body: Bytes| {
                let events = source
                    .decode(&encoding_header, body, path.as_str())
                    .and_then(|body| {
                        handle_dd_trace_payload(
                            body,
                            source.api_key_extractor.extract(
                                path.as_str(),
                                api_token,
                                query_params.dd_api_key,
                            ),
                            reported_language.as_ref(),
                            &source,
                        )
                        .map_err(|error| {
                            ErrorMessage::new(
                                StatusCode::UNPROCESSABLE_ENTITY,
                                format!("Error decoding Datadog traces: {:?}", error),
                            )
                        })
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

fn build_stats_filter() -> BoxedFilter<(Response,)> {
    warp::post()
        .and(path!("api" / "v0.2" / "stats" / ..))
        .and_then(|| {
            warn!(message = "/api/v0.2/stats route is yet not supported.");
            let response: Result<Response, Rejection> = Ok(warp::reply().into_response());
            future::ready(response)
        })
        .boxed()
}

fn handle_dd_trace_payload(
    frame: Bytes,
    api_key: Option<Arc<str>>,
    lang: Option<&String>,
    source: &DatadogAgentSource,
) -> crate::Result<Vec<Event>> {
    let trace_events;
    // Try newer version first
    let decoded_payload = dd_proto::AgentPayload::decode(frame.clone())?;
    if decoded_payload.agent_version.is_empty() {
        warn!(message = "Failed to decode trace payload, trying with older format");
        let decoded_payload = dd_proto::TracePayload::decode(frame)?;
        trace_events = handle_dd_trace_payload_v1(decoded_payload, api_key, lang, source);
    } else {
        trace_events = handle_dd_trace_payload_v2(decoded_payload, api_key, source);
    }
    emit!(&EventsReceived {
        byte_size: trace_events.size_of(),
        count: trace_events.len(),
    });
    Ok(trace_events)
}

fn handle_dd_trace_payload_v1(
    trace_payload: dd_proto::TracePayload,
    api_key: Option<Arc<str>>,
    lang: Option<&String>,
    source: &DatadogAgentSource,
) -> Vec<Event> {
    let env = trace_payload.env;
    let hostname = trace_payload.host_name;
    // Each traces is mapped to one event...
    trace_payload
        .traces
        .iter()
        .map(|dd_traces| convert_dd_trace_v1(dd_traces, env.clone(), hostname.clone(), source))
        //... and each APM event is also mapped into its own event
        .chain(trace_payload.transactions.iter().map(|s| {
            let mut log_event = LogEvent::from(convert_span(s));
            log_event.insert(
                source.log_schema_source_type_key,
                Bytes::from("datadog_agent"),
            );
            log_event.insert(source.log_schema_host_key, hostname.clone());
            log_event.insert("env", env.clone());
            log_event
        }))
        .map(|mut log_event| {
            if let Some(k) = &api_key {
                log_event
                    .metadata_mut()
                    .set_datadog_api_key(Some(Arc::clone(k)));
            }
            if let Some(lang) = lang {
                log_event.insert("language", lang.clone());
            }
            log_event.insert("payload_version", "v1".to_string());
            Event::Trace(log_event)
        })
        .collect()
}

fn convert_dd_trace_v1(
    dd_trace: &dd_proto::ApiTrace,
    env: String,
    hostname: String,
    source: &DatadogAgentSource,
) -> LogEvent {
    let mut log_event = LogEvent::default();
    log_event.insert(
        source.log_schema_source_type_key,
        Bytes::from("datadog_agent"),
    );
    log_event.insert(source.log_schema_host_key, hostname);
    log_event.insert("env", env);

    log_event.insert("trace_id", dd_trace.trace_id as i64);
    log_event.insert("start_time", Utc.timestamp_nanos(dd_trace.start_time));
    log_event.insert("end_time", Utc.timestamp_nanos(dd_trace.end_time));
    log_event.insert(
        "spans",
        dd_trace
            .spans
            .iter()
            .map(|s| Value::from(convert_span(s)))
            .collect::<Vec<Value>>(),
    );
    log_event
}

fn handle_dd_trace_payload_v2(
    agent_payload: dd_proto::AgentPayload,
    api_key: Option<Arc<str>>,
    source: &DatadogAgentSource,
) -> Vec<Event> {
    let env = agent_payload.env;
    let hostname = agent_payload.host_name;
    let agent_version = agent_payload.agent_version;
    let target_tps = agent_payload.target_tps;
    let error_tps = agent_payload.error_tps;

    let common_tags = agent_payload
        .tags
        .iter()
        .map(|(k, v)| (k.clone(), Value::from(v.clone())))
        .collect::<BTreeMap<String, Value>>();

    // Iterate over tracer payload, each payload will be an events
    // This remains TBC
    agent_payload
        .tracer_payloads
        .iter()
        .map(|tracer_payload| {
            convert_tracer_payload(
                tracer_payload,
                env.clone(),
                hostname.clone(),
                agent_version.clone(),
                target_tps,
                error_tps,
                common_tags.clone(),
                source,
            )
        })
        .map(|mut log_event| {
            if let Some(k) = &api_key {
                log_event
                    .metadata_mut()
                    .set_datadog_api_key(Some(Arc::clone(k)));
            }
            log_event.insert("payload_version", "v2".to_string());
            Event::Trace(log_event)
        })
        .collect()
}

fn convert_tracer_payload(
    payload: &dd_proto::TracerPayload,
    env: String,
    hostname: String,
    agent_version: String,
    target_tps: f64,
    error_tps: f64,
    common_tags: BTreeMap<String, Value>,
    source: &DatadogAgentSource,
) -> LogEvent {
    let mut log_event = LogEvent::default();
    log_event.insert(
        source.log_schema_source_type_key,
        Bytes::from("datadog_agent"),
    );
    log_event.insert(source.log_schema_host_key, hostname);
    log_event.insert("env", env);
    log_event.insert("agent_version", agent_version);
    log_event.insert("target_tps", target_tps);
    log_event.insert("error_tps", error_tps);

    log_event.insert("container_id", payload.container_id.clone());
    log_event.insert("language_name", payload.language_name.clone());
    log_event.insert("language_version", payload.language_version.clone());
    log_event.insert("tracer_version", payload.tracer_version.clone());
    log_event.insert("runtime_id", payload.runtime_id.clone());
    log_event.insert(
        "chunks",
        Value::from(
            payload
                .chunks
                .iter()
                .map(|c| Value::from(convert_chunk(c)))
                .collect::<Vec<Value>>(),
        ),
    );
    let mut tags = convert_tags(&payload.tags);
    tags.extend(common_tags);
    log_event.insert("tags", Value::from(tags));
    log_event.insert("tracer_env", payload.env.clone());
    log_event.insert("tracer_hostname", payload.hostname.clone());
    log_event.insert("app_version", payload.app_version.clone());
    log_event
}

fn convert_chunk(chunk: &dd_proto::TraceChunk) -> BTreeMap<String, Value> {
    let mut c = BTreeMap::<String, Value>::new();
    c.insert("priority".into(), Value::from(chunk.priority as i64));
    c.insert("origin".into(), chunk.origin.clone().into());
    c.insert(
        "spans".into(),
        Value::from(
            chunk
                .spans
                .iter()
                .map(|s| Value::from(convert_span(s)))
                .collect::<Vec<Value>>(),
        ),
    );
    c.insert("tags".into(), Value::from(convert_tags(&chunk.tags)));
    c.insert("dropped_trace".into(), Value::from(chunk.dropped_trace));
    c
}

fn convert_span(dd_span: &dd_proto::Span) -> BTreeMap<String, Value> {
    let mut span = BTreeMap::<String, Value>::new();
    span.insert("service".into(), dd_span.service.clone().into());
    span.insert("name".into(), Value::from(dd_span.name.clone()));
    span.insert("resource".into(), Value::from(dd_span.resource.clone()));
    span.insert("trace_id".into(), Value::from(dd_span.trace_id as i64));
    span.insert("span_id".into(), Value::from(dd_span.span_id as i64));
    span.insert("parent_id".into(), Value::from(dd_span.parent_id as i64));
    span.insert(
        "start".into(),
        Value::from(Utc.timestamp_nanos(dd_span.start)),
    );
    span.insert("duration".into(), Value::from(dd_span.duration as i64));
    span.insert("error".into(), Value::from(dd_span.error as i64));
    span.insert("meta".into(), Value::from(convert_tags(&dd_span.meta)));
    span.insert(
        "metrics".into(),
        Value::from(
            dd_span
                .metrics
                .iter()
                .map(|(k, v)| (k.clone(), Value::from(*v)))
                .collect::<BTreeMap<String, Value>>(),
        ),
    );
    span.insert("type".into(), Value::from(dd_span.r#type.clone()));
    span
}

fn convert_tags(original_map: &BTreeMap<String, String>) -> BTreeMap<String, Value> {
    original_map
        .iter()
        .map(|(k, v)| (k.clone(), Value::from(v.clone())))
        .collect::<BTreeMap<String, Value>>()
}
