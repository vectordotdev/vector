use std::{num::NonZeroU32, sync::Arc};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use http::StatusCode;
use prost::Message;
use serde::{Deserialize, Serialize};
use warp::{filters::BoxedFilter, path, path::FullPath, reply::Response, Filter};

use vector_lib::internal_event::{CountByteSize, InternalEventHandle as _, Registered};
use vector_lib::{
    event::{DatadogMetricOriginMetadata, EventMetadata},
    metrics::AgentDDSketch,
    EstimatedJsonEncodedSizeOf,
};

use crate::common::http::ErrorMessage;
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
            ddmetric_proto::{metric_payload, Metadata, MetricPayload, SketchPayload},
            handle_request, ApiKeyQueryParams, DatadogAgentSource,
        },
        util::extract_tag_key_and_value,
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

/// Builds Vector's `EventMetadata` from the received metadata. Currently this is only
/// utilized for passing through origin metadata set by the Agent.
fn get_event_metadata(metadata: Option<&Metadata>) -> EventMetadata {
    metadata
        .and_then(|metadata| metadata.origin.as_ref())
        .map_or_else(EventMetadata::default, |origin| {
            trace!(
                "Deserialized origin_product: `{}` origin_category: `{}` origin_service: `{}`.",
                origin.origin_product,
                origin.origin_category,
                origin.origin_service,
            );
            EventMetadata::default().with_origin_metadata(DatadogMetricOriginMetadata::new(
                Some(origin.origin_product),
                Some(origin.origin_category),
                Some(origin.origin_service),
            ))
        })
}

pub(crate) fn decode_ddseries_v2(
    frame: Bytes,
    api_key: &Option<Arc<str>>,
) -> crate::Result<Vec<Event>> {
    let payload = MetricPayload::decode(frame)?;
    let decoded_metrics: Vec<Event> = payload
        .series
        .into_iter()
        .flat_map(|serie| {
            let (namespace, name) = namespace_name_from_dd_metric(&serie.metric);
            let mut tags = into_metric_tags(serie.tags);

            let event_metadata = get_event_metadata(serie.metadata.as_ref());

            // It is possible to receive non-rate metrics from the Agent with an interval set.
            // That interval can be applied with the `as_rate` function in the Datadog UI.
            // The scenario this happens is when DogStatsD emits non-rate series metrics to the Agent,
            // in which it sets an interval to 10. See
            //    - https://github.com/DataDog/datadog-agent/blob/9f0a85c926596ec9aebe2d8e1f2a8b1af6e45635/pkg/aggregator/time_sampler.go#L49C1-L49C1
            //    - https://github.com/DataDog/datadog-agent/blob/209b70529caff9ec1c30b6b2eed27bce725ed153/pkg/aggregator/aggregator.go#L39
            //
            // Note that DogStatsD is the only scenario this occurs; regular Agent checks/services do not set the
            // interval for non-rate series metrics.
            //
            // Note that because Vector does not yet have a specific Metric type to handle Rate,
            // we are distinguishing Rate from Count by setting an interval to Rate but not Count.
            // Luckily, the only time a Count metric type is emitted by DogStatsD, is in the Sketch endpoint.
            // (Regular Count metrics are emitted by DogStatsD as Rate metrics).
            //
            // In theory we should be safe to set this non-rate-interval to Count metrics below, but to be safe,
            // we will only set it for Rate and Gauge. Since Rates already need an interval, the only "odd" case
            // is Gauges.
            //
            // Ultimately if we had a unique internal representation of a Rate metric type, we wouldn't need to
            // have special handling for the interval, we would just apply it to all metrics that it came in with.
            let non_rate_interval = if serie.interval.is_positive() {
                NonZeroU32::new(serie.interval as u32 * 1000) // incoming is seconds, convert to milliseconds
            } else {
                None
            };

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
            match metric_payload::MetricType::try_from(serie.r#type) {
                Ok(metric_payload::MetricType::Count) => serie
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
                Ok(metric_payload::MetricType::Gauge) => serie
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
                        .with_interval_ms(non_rate_interval)
                    })
                    .collect::<Vec<_>>(),
                Ok(metric_payload::MetricType::Rate) => serie
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
                Ok(metric_payload::MetricType::Unspecified) | Err(_) => {
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

            let event_metadata = get_event_metadata(sketch_series.metadata.as_ref());

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
                let mut metric = Metric::new_with_metadata(
                    name.to_string(),
                    MetricKind::Incremental,
                    val,
                    event_metadata.clone(),
                )
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
