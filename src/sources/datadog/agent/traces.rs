use std::{collections::BTreeMap, sync::Arc};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::future;
use http::StatusCode;
use ordered_float::NotNan;
use prost::Message;
use vector_core::ByteSizeOf;
use warp::{filters::BoxedFilter, path, path::FullPath, reply::Response, Filter, Rejection, Reply};

use crate::{
    event::{Event, TraceEvent, Value},
    internal_events::EventsReceived,
    sources::{
        datadog::agent::{self, handle_request, ApiKeyQueryParams, DatadogAgentSource},
        util::ErrorMessage,
    },
    SourceSender,
};

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
                    handle_request(events, acknowledgements, out.clone(), Some(agent::TRACES))
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
            // APM stats are discarded on purpose, they will be computed in the `datadog_traces` sink
            // thus we simply reply with a 200/OK response.
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
    let decoded_payload = dd_proto::TracePayload::decode(frame)?;
    if decoded_payload.tracer_payloads.is_empty() {
        debug!("Older trace payload decoded.");
        handle_dd_trace_payload_v0(decoded_payload, api_key, lang, source)
    } else {
        debug!("Newer trace payload decoded.");
        handle_dd_trace_payload_v1(decoded_payload, api_key, source)
    }
}

/// Decode Datadog newer protobuf schema
fn handle_dd_trace_payload_v1(
    decoded_payload: dd_proto::TracePayload,
    api_key: Option<Arc<str>>,
    source: &DatadogAgentSource,
) -> crate::Result<Vec<Event>> {
    let env = decoded_payload.env;
    let hostname = decoded_payload.host_name;
    let agent_version = decoded_payload.agent_version;
    let target_tps = decoded_payload.target_tps;
    let error_tps = decoded_payload.error_tps;
    let tags = convert_tags(decoded_payload.tags);

    let trace_events: Vec<TraceEvent> = decoded_payload
        .tracer_payloads
        .into_iter()
        .flat_map(convert_dd_tracer_payload)
        .collect();

    emit!(EventsReceived {
        byte_size: trace_events.size_of(),
        count: trace_events.len(),
    });

    let enriched_events = trace_events
        .into_iter()
        .map(|mut trace_event| {
            if let Some(k) = &api_key {
                trace_event
                    .metadata_mut()
                    .set_datadog_api_key(Some(Arc::clone(k)));
            }
            trace_event.insert(
                source.log_schema_source_type_key,
                Bytes::from("datadog_agent"),
            );
            trace_event.insert("payload_version", "v2".to_string());
            trace_event.insert(source.log_schema_host_key, hostname.clone());
            trace_event.insert("env", env.clone());
            trace_event.insert("agent_version", agent_version.clone());
            trace_event.insert("target_tps", target_tps);
            trace_event.insert("error_tps", error_tps);
            if let Some(Value::Object(span_tags)) = trace_event.get_mut("tags") {
                span_tags.extend(tags.clone());
            } else {
                trace_event.insert("tags", Value::from(tags.clone()));
            }
            Event::Trace(trace_event)
        })
        .collect();
    Ok(enriched_events)
}

fn convert_dd_tracer_payload(payload: dd_proto::TracerPayload) -> Vec<TraceEvent> {
    payload
        .chunks
        .into_iter()
        .map(|trace| {
            let mut trace_event = TraceEvent::default();
            trace_event.insert("priority", trace.priority as i64);
            trace_event.insert("origin", trace.origin);
            trace_event.insert("dropped", trace.dropped_trace);
            trace_event.insert("tags", Value::from(convert_tags(trace.tags)));
            trace_event.insert(
                "spans",
                trace
                    .spans
                    .into_iter()
                    .map(|s| Value::from(convert_span(s)))
                    .collect::<Vec<Value>>(),
            );
            trace_event.insert("container_id", payload.container_id.clone());
            trace_event.insert("language_name", payload.language_name.clone());
            trace_event.insert("language_version", payload.language_version.clone());
            trace_event.insert("tracer_version", payload.tracer_version.clone());
            trace_event.insert("runtime_id", payload.runtime_id.clone());
            trace_event.insert("app_version", payload.app_version.clone());
            trace_event
        })
        .collect()
}

// Decode Datadog older protobuf schema
fn handle_dd_trace_payload_v0(
    decoded_payload: dd_proto::TracePayload,
    api_key: Option<Arc<str>>,
    lang: Option<&String>,
    source: &DatadogAgentSource,
) -> crate::Result<Vec<Event>> {
    let env = decoded_payload.env;
    let hostname = decoded_payload.host_name;

    let trace_events: Vec<TraceEvent> =
    // Each traces is mapped to one event...
    decoded_payload
        .traces
        .into_iter()
        .map(|dd_trace| {
            let mut trace_event = TraceEvent::default();
            trace_event.insert("trace_id", dd_trace.trace_id as i64);
            trace_event.insert("start_time", Utc.timestamp_nanos(dd_trace.start_time));
            trace_event.insert("end_time", Utc.timestamp_nanos(dd_trace.end_time));
            trace_event.insert(
                "spans",
                dd_trace
                    .spans
                    .into_iter()
                    .map(|s| Value::from(convert_span(s)))
                    .collect::<Vec<Value>>(),
            );
            trace_event
        })
        //... and each APM event is also mapped into its own event
        .chain(decoded_payload.transactions.into_iter().map(|s| {
            let mut trace_event = TraceEvent::default();
            trace_event.insert("spans", vec![Value::from(convert_span(s))]);
            trace_event.insert("dropped", true);
            trace_event
        })).collect();

    emit!(EventsReceived {
        byte_size: trace_events.size_of(),
        count: trace_events.len(),
    });

    let enriched_events = trace_events
        .into_iter()
        .map(|mut trace_event| {
            if let Some(k) = &api_key {
                trace_event
                    .metadata_mut()
                    .set_datadog_api_key(Some(Arc::clone(k)));
            }
            if let Some(lang) = lang {
                trace_event.insert("language_name", lang.clone());
            }
            trace_event.insert(
                source.log_schema_source_type_key,
                Bytes::from("datadog_agent"),
            );
            trace_event.insert("payload_version", "v1".to_string());
            trace_event.insert(source.log_schema_host_key, hostname.clone());
            trace_event.insert("env", env.clone());
            Event::Trace(trace_event)
        })
        .collect();

    Ok(enriched_events)
}

fn convert_span(dd_span: dd_proto::Span) -> BTreeMap<String, Value> {
    let mut span = BTreeMap::<String, Value>::new();
    span.insert("service".into(), Value::from(dd_span.service));
    span.insert("name".into(), Value::from(dd_span.name));
    span.insert("resource".into(), Value::from(dd_span.resource));
    span.insert("trace_id".into(), Value::from(dd_span.trace_id as i64));
    span.insert("span_id".into(), Value::from(dd_span.span_id as i64));
    span.insert("parent_id".into(), Value::from(dd_span.parent_id as i64));
    span.insert(
        "start".into(),
        Value::from(Utc.timestamp_nanos(dd_span.start)),
    );
    span.insert("duration".into(), Value::from(dd_span.duration as i64));
    span.insert("error".into(), Value::from(dd_span.error as i64));
    span.insert("meta".into(), Value::from(convert_tags(dd_span.meta)));
    span.insert(
        "metrics".into(),
        Value::from(
            dd_span
                .metrics
                .into_iter()
                .map(|(k, v)| {
                    (
                        k,
                        NotNan::new(v as f64)
                            .map(Value::Float)
                            .unwrap_or(Value::Null),
                    )
                })
                .collect::<BTreeMap<String, Value>>(),
        ),
    );
    span.insert("type".into(), Value::from(dd_span.r#type));
    span.insert(
        "meta_struct".into(),
        Value::from(
            dd_span
                .meta_struct
                .into_iter()
                .map(|(k, v)| (k, Value::from(bytes::Bytes::from(v))))
                .collect::<BTreeMap<String, Value>>(),
        ),
    );
    span
}

fn convert_tags(original_map: BTreeMap<String, String>) -> BTreeMap<String, Value> {
    original_map
        .into_iter()
        .map(|(k, v)| (k, Value::from(v)))
        .collect::<BTreeMap<String, Value>>()
}
