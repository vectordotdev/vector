use std::{num::NonZeroU32, sync::Arc};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use http::StatusCode;
use prost::Message;
use serde::{Deserialize, Serialize};
use warp::{filters::BoxedFilter, path, path::FullPath, reply::Response, Filter};

use vector_common::internal_event::{CountByteSize, InternalEventHandle as _, Registered};
use vector_core::{
    event::{DatadogMetricOriginMetadata, EventMetadata},
    metrics::AgentDDSketch,
    EstimatedJsonEncodedSizeOf,
};

use crate::{
    common::datadog::{DatadogMetricType, DatadogSeriesMetric},
    config::log_schema,
    event::{
        metric::{Metric, MetricValue},
        Event, MetricKind, MetricTags,
    },
    internal_events::EventsReceived,
    schema,
    sources::{
        datadog_agent::{
            ddmetric_proto::{metric_payload, MetricPayload, SketchPayload},
            handle_request, ApiKeyQueryParams, DatadogAgentSource,
        },
        util::{extract_tag_key_and_value, ErrorMessage},
    },
    SourceSender,
};

#[derive(Deserialize, Serialize)]
pub(crate) struct DatadogSeriesRequest {
    pub(crate) series: Vec<DatadogSeriesMetric>,
}

pub(crate) fn build_warp_filter(
    acknowledgements: bool,
    multiple_outputs: bool,
    out: SourceSender,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    let output = multiple_outputs.then_some(super::METRICS);
    let sketches_service = sketches_service(acknowledgements, output, out.clone(), source.clone());
    let series_v1_service =
        series_v1_service(acknowledgements, output, out.clone(), source.clone());
    let series_v2_service = series_v2_service(acknowledgements, output, out, source);
    sketches_service
        .or(series_v1_service)
        .unify()
        .or(series_v2_service)
        .unify()
        .boxed()
}

fn sketches_service(
    acknowledgements: bool,
    output: Option<&'static str>,
    out: SourceSender,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(path!("api" / "beta" / "sketches" / ..))
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
                        decode_datadog_sketches(
                            body,
                            source.api_key_extractor.extract(
                                path.as_str(),
                                api_token,
                                query_params.dd_api_key,
                            ),
                            &source.events_received,
                        )
                    });
                handle_request(events, acknowledgements, out.clone(), output)
            },
        )
        .boxed()
}

fn series_v1_service(
    acknowledgements: bool,
    output: Option<&'static str>,
    out: SourceSender,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(path!("api" / "v1" / "series" / ..))
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
                        decode_datadog_series_v1(
                            body,
                            source.api_key_extractor.extract(
                                path.as_str(),
                                api_token,
                                query_params.dd_api_key,
                            ),
                            // Currently metrics do not have schemas defined, so for now we just pass a
                            // default one.
                            &Arc::new(schema::Definition::default_legacy_namespace()),
                            &source.events_received,
                        )
                    });
                handle_request(events, acknowledgements, out.clone(), output)
            },
        )
        .boxed()
}

fn series_v2_service(
    acknowledgements: bool,
    output: Option<&'static str>,
    out: SourceSender,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(path!("api" / "v2" / "series" / ..))
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
                        decode_datadog_series_v2(
                            body,
                            source.api_key_extractor.extract(
                                path.as_str(),
                                api_token,
                                query_params.dd_api_key,
                            ),
                            &source.events_received,
                        )
                    });
                handle_request(events, acknowledgements, out.clone(), output)
            },
        )
        .boxed()
}

fn decode_datadog_sketches(
    body: Bytes,
    api_key: Option<Arc<str>>,
    events_received: &Registered<EventsReceived>,
) -> Result<Vec<Event>, ErrorMessage> {
    if body.is_empty() {
        // The datadog agent may send an empty payload as a keep alive
        debug!(
            message = "Empty payload ignored.",
            internal_log_rate_limit = true
        );
        return Ok(Vec::new());
    }

    let metrics = decode_ddsketch(body, &api_key).map_err(|error| {
        ErrorMessage::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Error decoding Datadog sketch: {:?}", error),
        )
    })?;

    events_received.emit(CountByteSize(
        metrics.len(),
        metrics.estimated_json_encoded_size_of(),
    ));

    Ok(metrics)
}

fn decode_datadog_series_v2(
    body: Bytes,
    api_key: Option<Arc<str>>,
    events_received: &Registered<EventsReceived>,
) -> Result<Vec<Event>, ErrorMessage> {
    if body.is_empty() {
        // The datadog agent may send an empty payload as a keep alive
        debug!(
            message = "Empty payload ignored.",
            internal_log_rate_limit = true
        );
        return Ok(Vec::new());
    }

    let metrics = decode_ddseries_v2(body, &api_key).map_err(|error| {
        ErrorMessage::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Error decoding Datadog sketch: {:?}", error),
        )
    })?;

    events_received.emit(CountByteSize(
        metrics.len(),
        metrics.estimated_json_encoded_size_of(),
    ));

    Ok(metrics)
}

fn _decode_with_unknown_origin(frame: &Bytes) {
    use protofish::decode::UnknownValue;
    use protofish::prelude::*;

    fn _print_type(value: &Value) {
        match value {
            Value::Double(_) => println!("double"),
            Value::Float(_) => println!("float"),
            Value::Int32(_) => println!("int32"),
            Value::Int64(_) => println!("int64"),
            Value::UInt32(_) => println!("uint32"),
            Value::UInt64(_) => println!("uint64"),
            Value::SInt32(_) => println!("sint32"),
            Value::SInt64(_) => println!("sint64"),
            Value::Fixed32(_) => println!("fixed32"),
            Value::Fixed64(_) => println!("fixed64"),
            Value::SFixed32(_) => println!("sfixed32"),
            Value::SFixed64(_) => println!("sfixed64"),
            Value::Bool(_) => println!("bool"),
            Value::String(_) => println!("string"),
            Value::Bytes(_) => println!("bytes"),
            Value::Packed(_) => println!("packed"),
            Value::Message(_) => println!("message"),
            Value::Enum(_) => println!("enum"),
            Value::Incomplete(_, _) => println!("incomplete"),
            Value::Unknown(_) => println!("unknown"),
        }
    }

    let contents = std::fs::read_to_string("proto/dd_metric.proto")
        .expect("Should have been able to read the file");
    let context = Context::parse(&[contents]).unwrap();
    let metric_payload = context
        .get_message("datadog.agentpayload.MetricPayload")
        .unwrap();

    let metric_payload_value = metric_payload.decode(&frame, &context);

    // TODO the below works hackishly but it assumes a fixed order of the protobuf fields, which
    // is incorrect as they can be in any order.

    for field_value in &metric_payload_value.fields {
        let Value::Message(ref series) = field_value.value else {
            panic!("incorrect protobuf");
        };

        println!("n series fields: {}", series.fields.len());

        assert!(series.fields.len() >= 5);

        let mut idx = 0;

        // TODO unclear behavior if the repeated Value::Message has more than one entry. may need
        // to adapt this to loop on resources and points as well.

        // resources and metric name
        let _metric_name_value = if let Value::Message(ref _resources) = series.fields[0].value {
            idx += 2;
            &series.fields[1].value
        } else {
            idx += 1;
            &series.fields[0].value
        };

        // has tags and points
        if let Value::String(_first_tag) = &series.fields[idx].value {
            println!("got tag: {_first_tag}");
            idx += 1;
            loop {
                if let Value::String(_a_tag) = &series.fields[idx].value {
                    println!("got tag: {_a_tag}");
                    idx += 1;
                } else {
                    break;
                }
            }
        }

        // points
        if let Value::Message(_points) = &series.fields[idx].value {
            println!("got points");
            idx += 1;
        }

        // type
        if let Value::Enum(_type) = &series.fields[idx].value {
            println!("got type");
            idx += 1;
        };

        // print_type(&series.fields[idx].value);

        // unit or source_type_name
        if let Value::String(_unit_or_source_type_name) = &series.fields[idx].value {
            println!("got unit or source type name: {_unit_or_source_type_name}");
            idx += 1;
        };

        // source_type_name
        if let Value::String(_source_type_name) = &series.fields[idx].value {
            println!("got source type name: {_source_type_name}");
            idx += 1;
        };

        // interval
        if let Value::Int64(_interval) = &series.fields[idx].value {
            println!("got interval {_interval}");
            idx += 1;
        };
        _print_type(&series.fields[idx].value);

        // points
        if let Value::Message(metadata_value) = &series.fields[idx].value {
            idx += 1;
            println!("got message");
            println!("n message fields: {}", metadata_value.fields.len());
            for field in &metadata_value.fields {
                _print_type(&field.value);
                if let Value::Double(val) = &field.value {
                    println!("got double value {}", val);
                }
                if let Value::Int64(val) = &field.value {
                    println!("got int64 value {}", val);
                }
                if let Value::Enum(val) = &field.value {
                    println!("got enum value {}", val.value);
                }
            }
        }

        _print_type(&series.fields[idx].value);

        // metadata
        if let Value::Unknown(unknown_value) = &series.fields[idx].value {
            println!("got unknown value");
            idx += 1;
            match unknown_value {
                UnknownValue::VariableLength(bytes) => {
                    println!("got bytes for metadata: {:?}", bytes);
                }
                UnknownValue::Invalid(var, bytes) => {
                    println!("got invalid for metadata: {:?} {:?}", var, bytes);
                }
                UnknownValue::Varint(var) => {
                    println!("got u128 for metadata: {}", var);
                }
                UnknownValue::Fixed64(var) => {
                    println!("got u64 for metadata: {}", var);
                }
                UnknownValue::Fixed32(var) => {
                    println!("got u32 for metadata: {}", var);
                }
            }
        }

        println!("idx: {idx}");
        //break;
    }
}

/// Builds Vector's `EventMetadata` from the series' metadata. Currently this is only
/// utilized for passing through origin metadata set by the Agent.
fn get_event_metadata(metadata: Option<&metric_payload::Metadata>) -> EventMetadata {
    metadata.map_or(EventMetadata::default(), |metadata| {
        metadata
            .origin
            .as_ref()
            .map_or(EventMetadata::default(), |origin| {
                // TODO remove
                println!(
                    "origin_product: `{}` origin_category: `{}` origin_service: `{}`",
                    origin.origin_product, origin.origin_category, origin.origin_service
                );
                EventMetadata::default().with_origin_metadata(
                    DatadogMetricOriginMetadata::default()
                        .with_product(origin.origin_product)
                        .with_category(origin.origin_category)
                        .with_service(origin.origin_service),
                )
            })
    })
}

pub(crate) fn decode_ddseries_v2(
    frame: Bytes,
    api_key: &Option<Arc<str>>,
) -> crate::Result<Vec<Event>> {
    // decode_with_unknown_origin(&frame);

    let payload = MetricPayload::decode(frame)?;
    let decoded_metrics: Vec<Event> = payload
        .series
        .into_iter()
        .flat_map(|serie| {
            let (namespace, name) = namespace_name_from_dd_metric(&serie.metric);
            let mut tags = into_metric_tags(serie.tags);

            let event_metadata = get_event_metadata(serie.metadata.as_ref());

            serie.resources.into_iter().for_each(|r| {
                // As per https://github.com/DataDog/datadog-agent/blob/a62ac9fb13e1e5060b89e731b8355b2b20a07c5b/pkg/serializer/internal/metrics/iterable_series.go#L180-L189
                // the hostname can be found in MetricSeries::resources and that is the only value stored there.
                if r.r#type.eq("host") {
                    log_schema()
                        .host_key()
                        .and_then(|key| tags.replace(key.to_string(), r.name));
                } else {
                    // But to avoid losing information if this situation changes, any other resource type/name will be saved in the tags map
                    tags.replace(format!("resource.{}", r.r#type), r.name);
                }
            });
            (!serie.source_type_name.is_empty())
                .then(|| tags.replace("source_type_name".into(), serie.source_type_name));
            // As per https://github.com/DataDog/datadog-agent/blob/a62ac9fb13e1e5060b89e731b8355b2b20a07c5b/pkg/serializer/internal/metrics/iterable_series.go#L224
            // serie.unit is omitted
            match metric_payload::MetricType::from_i32(serie.r#type) {
                Some(metric_payload::MetricType::Count) => serie
                    .points
                    .iter()
                    .map(|dd_point| {
                        Metric::new_with_metadata(
                            name.to_string(),
                            MetricKind::Incremental,
                            MetricValue::Counter {
                                value: dd_point.value,
                            },
                            event_metadata.clone(),
                        )
                        .with_timestamp(Some(
                            Utc.timestamp_opt(dd_point.timestamp, 0)
                                .single()
                                .expect("invalid timestamp"),
                        ))
                        .with_tags(Some(tags.clone()))
                        .with_namespace(namespace)
                    })
                    .collect::<Vec<_>>(),
                Some(metric_payload::MetricType::Gauge) => serie
                    .points
                    .iter()
                    .map(|dd_point| {
                        Metric::new_with_metadata(
                            name.to_string(),
                            MetricKind::Absolute,
                            MetricValue::Gauge {
                                value: dd_point.value,
                            },
                            event_metadata.clone(),
                        )
                        .with_timestamp(Some(
                            Utc.timestamp_opt(dd_point.timestamp, 0)
                                .single()
                                .expect("invalid timestamp"),
                        ))
                        .with_tags(Some(tags.clone()))
                        .with_namespace(namespace)
                    })
                    .collect::<Vec<_>>(),
                Some(metric_payload::MetricType::Rate) => serie
                    .points
                    .iter()
                    .map(|dd_point| {
                        let i = Some(serie.interval)
                            .filter(|v| *v != 0)
                            .map(|v| v as u32)
                            .unwrap_or(1);
                        Metric::new_with_metadata(
                            name.to_string(),
                            MetricKind::Incremental,
                            MetricValue::Counter {
                                value: dd_point.value * (i as f64),
                            },
                            event_metadata.clone(),
                        )
                        .with_timestamp(Some(
                            Utc.timestamp_opt(dd_point.timestamp, 0)
                                .single()
                                .expect("invalid timestamp"),
                        ))
                        // serie.interval is in seconds, convert to ms
                        .with_interval_ms(NonZeroU32::new(i * 1000))
                        .with_tags(Some(tags.clone()))
                        .with_namespace(namespace)
                    })
                    .collect::<Vec<_>>(),
                Some(metric_payload::MetricType::Unspecified) | None => {
                    warn!("Unspecified metric type ({}).", serie.r#type);
                    Vec::new()
                }
            }
        })
        .map(|mut metric| {
            if let Some(k) = &api_key {
                metric.metadata_mut().set_datadog_api_key(Arc::clone(k));
            }
            metric.into()
        })
        .collect();

    Ok(decoded_metrics)
}

fn decode_datadog_series_v1(
    body: Bytes,
    api_key: Option<Arc<str>>,
    schema_definition: &Arc<schema::Definition>,
    events_received: &Registered<EventsReceived>,
) -> Result<Vec<Event>, ErrorMessage> {
    if body.is_empty() {
        // The datadog agent may send an empty payload as a keep alive
        debug!(
            message = "Empty payload ignored.",
            internal_log_rate_limit = true
        );
        return Ok(Vec::new());
    }

    let metrics: DatadogSeriesRequest = serde_json::from_slice(&body).map_err(|error| {
        ErrorMessage::new(
            StatusCode::BAD_REQUEST,
            format!("Error parsing JSON: {:?}", error),
        )
    })?;

    let decoded_metrics: Vec<Event> = metrics
        .series
        .into_iter()
        .flat_map(|m| into_vector_metric(m, api_key.clone(), schema_definition))
        .collect();

    events_received.emit(CountByteSize(
        decoded_metrics.len(),
        decoded_metrics.estimated_json_encoded_size_of(),
    ));

    Ok(decoded_metrics)
}

fn into_metric_tags(tags: Vec<String>) -> MetricTags {
    tags.iter().map(extract_tag_key_and_value).collect()
}

fn into_vector_metric(
    dd_metric: DatadogSeriesMetric,
    api_key: Option<Arc<str>>,
    schema_definition: &Arc<schema::Definition>,
) -> Vec<Event> {
    let mut tags = into_metric_tags(dd_metric.tags.unwrap_or_default());

    if let Some(key) = log_schema().host_key() {
        dd_metric
            .host
            .and_then(|host| tags.replace(key.to_string(), host));
    }

    dd_metric
        .source_type_name
        .and_then(|source| tags.replace("source_type_name".into(), source));
    dd_metric
        .device
        .and_then(|dev| tags.replace("device".into(), dev));

    let (namespace, name) = namespace_name_from_dd_metric(&dd_metric.metric);

    match dd_metric.r#type {
        DatadogMetricType::Count => dd_metric
            .points
            .iter()
            .map(|dd_point| {
                Metric::new(
                    name.to_string(),
                    MetricKind::Incremental,
                    MetricValue::Counter { value: dd_point.1 },
                )
                .with_timestamp(Some(
                    Utc.timestamp_opt(dd_point.0, 0)
                        .single()
                        .expect("invalid timestamp"),
                ))
                .with_tags(Some(tags.clone()))
                .with_namespace(namespace)
            })
            .collect::<Vec<_>>(),
        DatadogMetricType::Gauge => dd_metric
            .points
            .iter()
            .map(|dd_point| {
                Metric::new(
                    name.to_string(),
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: dd_point.1 },
                )
                .with_timestamp(Some(
                    Utc.timestamp_opt(dd_point.0, 0)
                        .single()
                        .expect("invalid timestamp"),
                ))
                .with_tags(Some(tags.clone()))
                .with_namespace(namespace)
            })
            .collect::<Vec<_>>(),
        // Agent sends rate only for dogstatsd counter https://github.com/DataDog/datadog-agent/blob/f4a13c6dca5e2da4bb722f861a8ac4c2f715531d/pkg/metrics/counter.go#L8-L10
        // for consistency purpose (w.r.t. (dog)statsd source) they are turned back into counters
        DatadogMetricType::Rate => dd_metric
            .points
            .iter()
            .map(|dd_point| {
                let i = dd_metric.interval.filter(|v| *v != 0).unwrap_or(1);
                Metric::new(
                    name.to_string(),
                    MetricKind::Incremental,
                    MetricValue::Counter {
                        value: dd_point.1 * (i as f64),
                    },
                )
                .with_timestamp(Some(
                    Utc.timestamp_opt(dd_point.0, 0)
                        .single()
                        .expect("invalid timestamp"),
                ))
                // dd_metric.interval is in seconds, convert to ms
                .with_interval_ms(NonZeroU32::new(i * 1000))
                .with_tags(Some(tags.clone()))
                .with_namespace(namespace)
            })
            .collect::<Vec<_>>(),
    }
    .into_iter()
    .map(|mut metric| {
        if let Some(k) = &api_key {
            metric.metadata_mut().set_datadog_api_key(Arc::clone(k));
        }

        metric
            .metadata_mut()
            .set_schema_definition(schema_definition);

        metric.into()
    })
    .collect()
}

/// Parses up to the first '.' of the input metric name into a namespace.
/// If no delimiter, the namespace is None type.
fn namespace_name_from_dd_metric(dd_metric_name: &str) -> (Option<&str>, &str) {
    // ex: "system.fs.util" -> ("system", "fs.util")
    match dd_metric_name.split_once('.') {
        Some((namespace, name)) => (Some(namespace), name),
        None => (None, dd_metric_name),
    }
}

pub(crate) fn decode_ddsketch(
    frame: Bytes,
    api_key: &Option<Arc<str>>,
) -> crate::Result<Vec<Event>> {
    let payload = SketchPayload::decode(frame)?;
    // payload.metadata is always empty for payload coming from dd agents
    Ok(payload
        .sketches
        .into_iter()
        .flat_map(|sketch_series| {
            // sketch_series.distributions is also always empty from payload coming from dd agents
            let mut tags = into_metric_tags(sketch_series.tags);
            log_schema()
                .host_key()
                .and_then(|key| tags.replace(key.to_string(), sketch_series.host.clone()));

            sketch_series.dogsketches.into_iter().map(move |sketch| {
                let k: Vec<i16> = sketch.k.iter().map(|k| *k as i16).collect();
                let n: Vec<u16> = sketch.n.iter().map(|n| *n as u16).collect();
                let val = MetricValue::from(
                    AgentDDSketch::from_raw(
                        sketch.cnt as u32,
                        sketch.min,
                        sketch.max,
                        sketch.sum,
                        sketch.avg,
                        &k,
                        &n,
                    )
                    .unwrap_or_else(AgentDDSketch::with_agent_defaults),
                );
                let (namespace, name) = namespace_name_from_dd_metric(&sketch_series.metric);
                let mut metric = Metric::new(name.to_string(), MetricKind::Incremental, val)
                    .with_tags(Some(tags.clone()))
                    .with_timestamp(Some(
                        Utc.timestamp_opt(sketch.ts, 0)
                            .single()
                            .expect("invalid timestamp"),
                    ))
                    .with_namespace(namespace);
                if let Some(k) = &api_key {
                    metric.metadata_mut().set_datadog_api_key(Arc::clone(k));
                }

                metric.into()
            })
        })
        .collect())
}
