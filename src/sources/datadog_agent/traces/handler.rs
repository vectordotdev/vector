use std::{collections::BTreeMap, sync::Arc};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::future;
use http::StatusCode;
use ordered_float::NotNan;
use prost::Message;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    event::TraceLayout,
    internal_event::{CountByteSize, InternalEventHandle as _},
};
use vrl::event_path;
use warp::{Filter, Rejection, Reply, filters::BoxedFilter, path, path::FullPath, reply::Response};

use super::{
    super::{ApiKeyQueryParams, DatadogAgentSource, RequestHandler, TRACES, ddtrace_proto},
    idx::convert_idx_tracer_payload,
};
use crate::{
    common::{datadog::encode_u64_id_hex, http::ErrorMessage},
    event::{Event, ObjectMap, TraceEvent, Value},
    internal_events::{
        DatadogAgentInvalidIdxStringRefError, DatadogAgentUnsupportedTracePayloadError,
    },
    sources::util::http::capped_body,
};

pub(in crate::sources::datadog_agent) fn build_warp_filter(
    handler: RequestHandler,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    build_trace_filter(handler, source)
        .or(build_stats_filter())
        .unify()
        .boxed()
}

fn build_trace_filter(
    handler: RequestHandler,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(path!("api" / "v0.2" / "traces" / ..))
        .and(warp::path::full())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(warp::header::optional::<String>("dd-api-key"))
        .and(warp::query::<ApiKeyQueryParams>())
        .and(capped_body())
        .and_then({
            move |path: FullPath,
                  encoding_header: Option<String>,
                  api_token: Option<String>,
                  query_params: ApiKeyQueryParams,
                  body: Bytes| {
                let events = source
                    .decode(&encoding_header, body, path.as_str())
                    .and_then(|body| {
                        handle_dd_trace_payload(
                            body,
                            source
                                .api_key_extractor
                                .extract(path.as_str(), api_token, query_params.dd_api_key)
                                .as_ref(),
                            &source,
                        )
                        .map_err(|error| {
                            ErrorMessage::new(
                                StatusCode::UNPROCESSABLE_ENTITY,
                                format!("Error decoding Datadog traces: {error:?}"),
                            )
                        })
                    });
                handler.clone().handle_request(events, TRACES)
            }
        })
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

struct AgentTraceFields<'a> {
    env: &'a str,
    hostname: &'a str,
    agent_version: &'a str,
    target_tps: f64,
    error_tps: f64,
    tags: &'a ObjectMap,
}

fn handle_dd_trace_payload(
    frame: Bytes,
    api_key: Option<&Arc<str>>,
    source: &DatadogAgentSource,
) -> crate::Result<Vec<Event>> {
    let decoded_payload = ddtrace_proto::AgentPayload::decode(frame)?;
    if decoded_payload.tracer_payloads.is_empty() && decoded_payload.idx_tracer_payloads.is_empty()
    {
        emit!(DatadogAgentUnsupportedTracePayloadError {
            error_code: "empty_tracer_payloads",
        });
        return Ok(Vec::new());
    }

    let mut trace_events: Vec<TraceEvent> = decoded_payload
        .tracer_payloads
        .into_iter()
        .flat_map(convert_dd_tracer_payload)
        .collect();
    for payload in decoded_payload.idx_tracer_payloads {
        match convert_idx_tracer_payload(payload) {
            Ok(events) => trace_events.extend(events),
            Err(error) => {
                emit!(DatadogAgentInvalidIdxStringRefError { index: error.index });
                return Err(error.into());
            }
        }
    }

    source.events_received.emit(CountByteSize(
        trace_events.len(),
        trace_events.estimated_json_encoded_size_of(),
    ));

    let tags = convert_tags(decoded_payload.tags);
    let fields = AgentTraceFields {
        env: &decoded_payload.env,
        hostname: &decoded_payload.host_name,
        agent_version: &decoded_payload.agent_version,
        target_tps: decoded_payload.target_tps,
        error_tps: decoded_payload.error_tps,
        tags: &tags,
    };
    Ok(trace_events
        .into_iter()
        .map(|trace_event| enrich_trace_event(trace_event, api_key, source, &fields))
        .collect())
}

fn enrich_trace_event(
    mut trace_event: TraceEvent,
    api_key: Option<&Arc<str>>,
    source: &DatadogAgentSource,
    fields: &AgentTraceFields<'_>,
) -> Event {
    if let Some(key) = api_key {
        trace_event
            .metadata_mut()
            .set_datadog_api_key(Arc::clone(key));
    }
    trace_event.insert(
        &source.log_schema_source_type_key,
        Value::from_static_str("datadog_agent"),
    );
    trace_event.insert(event_path!("payload_version"), "v2".to_string());
    trace_event.insert(&source.log_schema_host_key, fields.hostname.to_owned());
    trace_event.insert(event_path!("env"), fields.env.to_owned());
    trace_event.insert(
        event_path!("agent_version"),
        fields.agent_version.to_owned(),
    );
    trace_event.insert(
        event_path!("target_tps"),
        Value::Float(NotNan::new(fields.target_tps).expect("target_tps cannot be Nan")),
    );
    trace_event.insert(
        event_path!("error_tps"),
        Value::Float(NotNan::new(fields.error_tps).expect("error_tps cannot be Nan")),
    );
    if let Some(Value::Object(span_tags)) = trace_event.get_mut(event_path!("tags")) {
        span_tags.extend(fields.tags.clone());
    } else {
        trace_event.insert(event_path!("tags"), Value::from(fields.tags.clone()));
    }
    Event::Trace(trace_event)
}

pub(super) fn new_trace_event() -> TraceEvent {
    let mut trace_event = TraceEvent::default();
    trace_event
        .metadata_mut()
        .set_trace_layout(TraceLayout::Datadog);
    trace_event
}

pub(super) fn convert_dd_tracer_payload(payload: ddtrace_proto::TracerPayload) -> Vec<TraceEvent> {
    let tags = convert_tags(payload.tags);
    payload
        .chunks
        .into_iter()
        .map(|trace| {
            let mut trace_event = new_trace_event();
            trace_event.insert(event_path!("priority"), i64::from(trace.priority));
            trace_event.insert(event_path!("origin"), trace.origin);
            trace_event.insert(event_path!("dropped"), trace.dropped_trace);
            let mut trace_tags = convert_tags(trace.tags);
            trace_tags.extend(tags.clone());
            trace_event.insert(event_path!("tags"), Value::from(trace_tags));

            trace_event.insert(
                event_path!("spans"),
                trace
                    .spans
                    .into_iter()
                    .map(|span| Value::from(convert_span(span)))
                    .collect::<Vec<Value>>(),
            );

            trace_event.insert(event_path!("container_id"), payload.container_id.clone());
            trace_event.insert(event_path!("language_name"), payload.language_name.clone());
            trace_event.insert(
                event_path!("language_version"),
                payload.language_version.clone(),
            );
            trace_event.insert(
                event_path!("tracer_version"),
                payload.tracer_version.clone(),
            );
            trace_event.insert(event_path!("runtime_id"), payload.runtime_id.clone());
            trace_event.insert(event_path!("app_version"), payload.app_version.clone());
            trace_event
        })
        .collect()
}

#[allow(
    clippy::cast_possible_wrap,
    reason = "Preserve the existing numeric conversion semantics of this source. https://github.com/vectordotdev/vector/issues/23659"
)]
fn convert_span(dd_span: ddtrace_proto::Span) -> ObjectMap {
    let mut span = ObjectMap::new();
    span.insert("service".into(), Value::from(dd_span.service));
    span.insert("name".into(), Value::from(dd_span.name));

    span.insert("resource".into(), Value::from(dd_span.resource));

    // TODO trace_id, span_id and parent_id are being forced into an i64 but
    // the incoming payload is u64. This is a bug and needs to be fixed per:
    // https://github.com/vectordotdev/vector/issues/14687
    span.insert("trace_id".into(), Value::from(dd_span.trace_id as i64));
    span.insert("span_id".into(), Value::from(dd_span.span_id as i64));
    span.insert("parent_id".into(), Value::from(dd_span.parent_id as i64));
    span.insert(
        "start".into(),
        Value::from(Utc.timestamp_nanos(dd_span.start)),
    );
    span.insert("duration".into(), Value::from(dd_span.duration));
    span.insert("error".into(), Value::from(i64::from(dd_span.error)));
    span.insert("meta".into(), Value::from(convert_tags(dd_span.meta)));
    span.insert(
        "metrics".into(),
        Value::from(
            dd_span
                .metrics
                .into_iter()
                .map(|(key, value)| {
                    (
                        key.into(),
                        NotNan::new(value).map_or(Value::Null, Value::Float),
                    )
                })
                .collect::<ObjectMap>(),
        ),
    );
    span.insert("type".into(), Value::from(dd_span.r#type));
    span.insert(
        "meta_struct".into(),
        Value::from(
            dd_span
                .meta_struct
                .into_iter()
                .map(|(key, value)| (key.into(), Value::from(bytes::Bytes::from(value))))
                .collect::<ObjectMap>(),
        ),
    );
    span.insert(
        "span_links".into(),
        Value::Array(
            dd_span
                .span_links
                .into_iter()
                .map(|link| Value::from(convert_span_link(link)))
                .collect(),
        ),
    );
    span.insert(
        "span_events".into(),
        Value::Array(
            dd_span
                .span_events
                .into_iter()
                .map(|event| Value::from(convert_span_event(event)))
                .collect(),
        ),
    );

    span
}

fn convert_span_link(link: ddtrace_proto::SpanLink) -> ObjectMap {
    ObjectMap::from([
        (
            "trace_id".into(),
            Value::from(encode_u64_id_hex(link.trace_id)),
        ),
        (
            "trace_id_high".into(),
            Value::from(encode_u64_id_hex(link.trace_id_high)),
        ),
        (
            "span_id".into(),
            Value::from(encode_u64_id_hex(link.span_id)),
        ),
        (
            "attributes".into(),
            Value::from(convert_tags(link.attributes)),
        ),
        ("tracestate".into(), Value::from(link.tracestate)),
        ("flags".into(), Value::from(i64::from(link.flags))),
    ])
}

#[allow(
    clippy::cast_possible_wrap,
    reason = "Preserve the existing numeric conversion semantics of this source. https://github.com/vectordotdev/vector/issues/23659"
)]
fn convert_span_event(event: ddtrace_proto::SpanEvent) -> ObjectMap {
    ObjectMap::from([
        (
            "time_unix_nano".into(),
            Value::from(Utc.timestamp_nanos(event.time_unix_nano as i64)),
        ),
        ("name".into(), Value::from(event.name)),
        (
            "attributes".into(),
            Value::from(
                event
                    .attributes
                    .into_iter()
                    .map(|(key, value)| (key.into(), convert_attribute_any_value(value)))
                    .collect::<ObjectMap>(),
            ),
        ),
    ])
}

fn convert_attribute_any_value(value: ddtrace_proto::AttributeAnyValue) -> Value {
    use ddtrace_proto::attribute_any_value::AttributeAnyValueType;

    match AttributeAnyValueType::try_from(value.r#type)
        .unwrap_or(AttributeAnyValueType::StringValue)
    {
        AttributeAnyValueType::StringValue => Value::from(value.string_value),
        AttributeAnyValueType::BoolValue => Value::from(value.bool_value),
        AttributeAnyValueType::IntValue => Value::from(value.int_value),
        AttributeAnyValueType::DoubleValue => {
            NotNan::new(value.double_value).map_or(Value::Null, Value::Float)
        }
        AttributeAnyValueType::ArrayValue => Value::Array(
            value
                .array_value
                .map(|array| {
                    array
                        .values
                        .into_iter()
                        .map(convert_attribute_array_value)
                        .collect()
                })
                .unwrap_or_default(),
        ),
    }
}

fn convert_attribute_array_value(value: ddtrace_proto::AttributeArrayValue) -> Value {
    use ddtrace_proto::attribute_array_value::AttributeArrayValueType;

    match AttributeArrayValueType::try_from(value.r#type)
        .unwrap_or(AttributeArrayValueType::StringValue)
    {
        AttributeArrayValueType::StringValue => Value::from(value.string_value),
        AttributeArrayValueType::BoolValue => Value::from(value.bool_value),
        AttributeArrayValueType::IntValue => Value::from(value.int_value),
        AttributeArrayValueType::DoubleValue => {
            NotNan::new(value.double_value).map_or(Value::Null, Value::Float)
        }
    }
}

fn convert_tags(original_map: BTreeMap<String, String>) -> ObjectMap {
    original_map
        .into_iter()
        .map(|(key, value)| (key.into(), Value::from(value)))
        .collect::<ObjectMap>()
}
