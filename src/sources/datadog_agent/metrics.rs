use std::{num::NonZeroU32, sync::Arc};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use http::StatusCode;
use prost::Message;
use serde::{Deserialize, Serialize};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    event::{DatadogMetricOriginMetadata, EventMetadata},
    internal_event::{CountByteSize, InternalEventHandle as _, Registered},
    metrics::AgentDDSketch,
};
use warp::{Filter, filters::BoxedFilter, path, path::FullPath, reply::Response};

use super::ddmetric_proto::{Metadata, MetricPayload, SketchPayload, metric_payload};
use super::{DatadogAgentSource, RequestHandler};
use crate::{
    common::{
        datadog::{DATADOG_METRIC_RESOURCE_TAG_PREFIX, DatadogMetricType, DatadogSeriesMetric},
        http::ErrorMessage,
    },
    config::log_schema,
    event::{
        Event, MetricKind, MetricTags,
        metric::{Metric, MetricValue},
    },
    internal_events::EventsReceived,
    schema,
    sources::util::{extract_tag_key_and_value, http::capped_body},
};

mod v3;

pub(crate) use v3::decode_ddseries_v3;
#[cfg(test)]
pub(crate) use v3::decode_v3_metric_data;

const MAX_DATADOG_SERIES_UNIT_EVENT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
pub(crate) struct DatadogSeriesRequest {
    pub(crate) series: Vec<DatadogSeriesMetric>,
}

pub(super) fn build_warp_filter(
    handler: RequestHandler,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    let sketches_service = sketches_service(handler.clone(), source.clone());
    let series_v1_service = series_v1_service(handler.clone(), source.clone());
    let series_v2_service = series_v2_service(handler.clone(), source.clone());
    let series_v3_service = series_v3_service(handler, source);
    sketches_service
        .or(series_v1_service)
        .unify()
        .or(series_v2_service)
        .unify()
        .or(series_v3_service)
        .unify()
        .boxed()
}

fn sketches_service(
    handler: RequestHandler,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(path!("api" / "beta" / "sketches" / ..))
        .and(source.validated_api_key_filter())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(capped_body())
        .and_then({
            move |path: FullPath,
                  api_key: Option<Arc<str>>,
                  encoding_header: Option<String>,
                  body: Bytes| {
                let events = source
                    .decode(&encoding_header, body, path.as_str())
                    .and_then(|body| {
                        decode_datadog_sketches(
                            body,
                            api_key,
                            source.split_metric_namespace,
                            &source.events_received,
                        )
                    });
                handler.clone().handle_request(events, super::METRICS)
            }
        })
        .boxed()
}

fn series_v1_service(
    handler: RequestHandler,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(path!("api" / "v1" / "series" / ..))
        .and(source.validated_api_key_filter())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(capped_body())
        .and_then({
            move |path: FullPath,
                  api_key: Option<Arc<str>>,
                  encoding_header: Option<String>,
                  body: Bytes| {
                let events = source
                    .decode(&encoding_header, body, path.as_str())
                    .and_then(|body| {
                        decode_datadog_series_v1(
                            body,
                            api_key,
                            // Currently metrics do not have schemas defined, so for now we just pass a
                            // default one.
                            &Arc::new(schema::Definition::default_legacy_namespace()),
                            source.split_metric_namespace,
                            &source.events_received,
                        )
                    });
                handler.clone().handle_request(events, super::METRICS)
            }
        })
        .boxed()
}

fn series_v2_service(
    handler: RequestHandler,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(path!("api" / "v2" / "series" / ..))
        .and(source.validated_api_key_filter())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(capped_body())
        .and_then({
            move |path: FullPath,
                  api_key: Option<Arc<str>>,
                  encoding_header: Option<String>,
                  body: Bytes| {
                let events = source
                    .decode(&encoding_header, body, path.as_str())
                    .and_then(|body| {
                        decode_datadog_series_v2(
                            body,
                            api_key,
                            source.split_metric_namespace,
                            &source.events_received,
                        )
                    });
                handler.clone().handle_request(events, super::METRICS)
            }
        })
        .boxed()
}

fn series_v3_service(
    handler: RequestHandler,
    source: DatadogAgentSource,
) -> BoxedFilter<(Response,)> {
    warp::post()
        .and(path!("api" / "intake" / "metrics" / "v3" / "series" / ..))
        .and(source.validated_api_key_filter())
        .and(warp::header::optional::<String>("content-encoding"))
        .and(capped_body())
        .and_then({
            move |path: FullPath,
                  api_key: Option<Arc<str>>,
                  encoding_header: Option<String>,
                  body: Bytes| {
                let events = source
                    .decode(&encoding_header, body, path.as_str())
                    .and_then(|body| {
                        decode_datadog_series_v3(
                            body,
                            api_key,
                            source.split_metric_namespace,
                            &source.events_received,
                        )
                    });
                handler.clone().handle_request(events, super::METRICS)
            }
        })
        .boxed()
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Request handlers own payload and token values at this boundary."
)]
fn decode_datadog_sketches(
    body: Bytes,
    api_key: Option<Arc<str>>,
    split_metric_namespace: bool,
    events_received: &Registered<EventsReceived>,
) -> Result<Vec<Event>, ErrorMessage> {
    if body.is_empty() {
        // The datadog agent may send an empty payload as a keep alive
        debug!(message = "Empty payload ignored.");
        return Ok(Vec::new());
    }

    let metrics = decode_ddsketch(body, &api_key, split_metric_namespace).map_err(|error| {
        ErrorMessage::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Error decoding Datadog sketch: {error:?}"),
        )
    })?;

    events_received.emit(CountByteSize(
        metrics.len(),
        metrics.estimated_json_encoded_size_of(),
    ));

    Ok(metrics)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Request handlers own payload and token values at this boundary."
)]
fn decode_datadog_series_v2(
    body: Bytes,
    api_key: Option<Arc<str>>,
    split_metric_namespace: bool,
    events_received: &Registered<EventsReceived>,
) -> Result<Vec<Event>, ErrorMessage> {
    if body.is_empty() {
        // The datadog agent may send an empty payload as a keep alive
        debug!(message = "Empty payload ignored.");
        return Ok(Vec::new());
    }

    let metrics = decode_ddseries_v2(body, &api_key, split_metric_namespace).map_err(|error| {
        ErrorMessage::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Error decoding Datadog sketch: {error:?}"),
        )
    })?;

    events_received.emit(CountByteSize(
        metrics.len(),
        metrics.estimated_json_encoded_size_of(),
    ));

    Ok(metrics)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Request handlers own payload and token values at this boundary."
)]
fn decode_datadog_series_v3(
    body: Bytes,
    api_key: Option<Arc<str>>,
    split_metric_namespace: bool,
    events_received: &Registered<EventsReceived>,
) -> Result<Vec<Event>, ErrorMessage> {
    if body.is_empty() {
        debug!(message = "Empty payload ignored.");
        return Ok(Vec::new());
    }

    let metrics = decode_ddseries_v3(body, &api_key, split_metric_namespace).map_err(|error| {
        ErrorMessage::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Error decoding Datadog v3 series: {error:?}"),
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
fn get_event_metadata(metadata: Option<&Metadata>, metric_type: Option<i32>) -> EventMetadata {
    let origin = metadata.and_then(|metadata| metadata.origin.as_ref());
    let metric_type = metric_type.or_else(|| {
        origin
            .map(|origin| origin.metric_type)
            .filter(|metric_type| *metric_type != 0)
    });
    if origin.is_none() && metric_type.is_none() {
        EventMetadata::default()
    } else {
        let (origin_product, origin_category, origin_service) =
            origin.map_or((None, None, None), |origin| {
                trace!(
                    "Deserialized origin_product: `{}` origin_category: `{}` origin_service: `{}`.",
                    origin.origin_product, origin.origin_category, origin.origin_service,
                );
                (
                    Some(origin.origin_product),
                    Some(origin.origin_category),
                    Some(origin.origin_service),
                )
            });
        EventMetadata::default().with_origin_metadata(
            DatadogMetricOriginMetadata::new(origin_product, origin_category, origin_service)
                .with_metric_type(metric_type),
        )
    }
}

#[allow(
    clippy::ref_option,
    reason = "Changing this shared optional metadata signature is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
)]
#[allow(
    clippy::too_many_lines,
    reason = "Splitting this source routine is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Preserve the existing numeric conversion semantics of this source. https://github.com/vectordotdev/vector/issues/23659"
)]
pub(crate) fn decode_ddseries_v2(
    frame: Bytes,
    api_key: &Option<Arc<str>>,
    split_metric_namespace: bool,
) -> crate::Result<Vec<Event>> {
    let payload = MetricPayload::decode(frame)?;
    decode_ddseries(payload.series, api_key, split_metric_namespace, None)
}

#[allow(
    clippy::ref_option,
    reason = "Changing this shared optional metadata signature is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
)]
#[allow(
    clippy::too_many_lines,
    reason = "Splitting this source routine is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Preserve the existing numeric conversion semantics of this source. https://github.com/vectordotdev/vector/issues/23659"
)]
fn decode_ddseries(
    series: Vec<metric_payload::MetricSeries>,
    api_key: &Option<Arc<str>>,
    split_metric_namespace: bool,
    metric_types: Option<&[Option<i32>]>,
) -> crate::Result<Vec<Event>> {
    validate_unit_event_expansion(
        series
            .iter()
            .map(|series| (series.unit.as_str(), series.points.len())),
    )?;

    let decoded_metrics: Vec<Event> = series
        .into_iter()
        .enumerate()
        .flat_map(|(index, serie)| {
            // A series without points cannot emit an event. Skip it before expanding its tags
            // into a MetricTags map, since a compressed v3 payload can otherwise make this an
            // arbitrarily large allocation that is immediately discarded.
            if serie.points.is_empty() {
                return Vec::new();
            }

            let (namespace, name) = if split_metric_namespace {
                namespace_name_from_dd_metric(&serie.metric)
            } else {
                (None, serie.metric.as_str())
            };
            let mut tags = into_metric_tags(serie.tags);

            let metric_type = metric_types
                .and_then(|metric_types| metric_types.get(index))
                .copied()
                .flatten();
            let mut event_metadata = get_event_metadata(serie.metadata.as_ref(), metric_type);
            if !serie.unit.is_empty() {
                event_metadata.set_datadog_metric_unit(serie.unit.clone());
            }
            if let Some(api_key) = api_key {
                event_metadata.set_datadog_api_key(Arc::clone(api_key));
            }

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
                // As per https://github.com/DataDog/datadog-agent/blob/965622d50073913d95176606ebcbd0f7553627b6/pkg/serializer/internal/metrics/iterable_series.go#L201-L264
                // MetricSeries::resources can contain host, device, and other series resources.
                if r.r#type.eq("host") {
                    log_schema()
                        .host_key()
                        .and_then(|key| tags.replace(key.to_string(), r.name));
                } else if r.r#type.eq("device") {
                    // The `device` resource type is used by Agent checks (disk, SNMP/NDM, etc.)
                    // and must be preserved as a plain `device` tag to match the v1 series behavior.
                    tags.replace("device".into(), r.name);
                } else {
                    // Preserve other resources in the generic metric tags.
                    tags.insert(
                        format!("{DATADOG_METRIC_RESOURCE_TAG_PREFIX}{}", r.r#type),
                        r.name,
                    );
                }
            });
            (!serie.source_type_name.is_empty())
                .then(|| tags.replace("source_type_name".into(), serie.source_type_name));
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
                            .map_or(1, |v| v as u32);
                        Metric::new_with_metadata(
                            name.to_string(),
                            MetricKind::Incremental,
                            MetricValue::Counter {
                                value: dd_point.value * f64::from(i),
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
        .map(Event::from)
        .collect();

    Ok(decoded_metrics)
}

fn validate_unit_event_expansion<'a>(
    series: impl IntoIterator<Item = (&'a str, usize)>,
) -> crate::Result<()> {
    let expanded_bytes = series
        .into_iter()
        .try_fold(0usize, |total, (unit, points)| {
            unit.len()
                .checked_mul(points)
                .and_then(|bytes| total.checked_add(bytes))
                .ok_or("Datadog series unit expansion size overflow")
        })?;

    if expanded_bytes > MAX_DATADOG_SERIES_UNIT_EVENT_BYTES {
        return Err("Datadog series unit expansion exceeds size limit".into());
    }

    Ok(())
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Request handlers own payload and token values at this boundary."
)]
fn decode_datadog_series_v1(
    body: Bytes,
    api_key: Option<Arc<str>>,
    schema_definition: &Arc<schema::Definition>,
    split_metric_namespace: bool,
    events_received: &Registered<EventsReceived>,
) -> Result<Vec<Event>, ErrorMessage> {
    if body.is_empty() {
        // The datadog agent may send an empty payload as a keep alive
        debug!(message = "Empty payload ignored.");
        return Ok(Vec::new());
    }

    let metrics: DatadogSeriesRequest = serde_json::from_slice(&body).map_err(|error| {
        ErrorMessage::new(
            StatusCode::BAD_REQUEST,
            format!("Error parsing JSON: {error:?}"),
        )
    })?;

    validate_unit_event_expansion(metrics.series.iter().map(|series| {
        (
            series.unit.as_deref().unwrap_or_default(),
            series.points.len(),
        )
    }))
    .map_err(|error| {
        ErrorMessage::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Error decoding Datadog series: {error:?}"),
        )
    })?;

    let decoded_metrics: Vec<Event> = metrics
        .series
        .into_iter()
        .flat_map(|m| {
            into_vector_metric(
                m,
                api_key.clone(),
                schema_definition,
                split_metric_namespace,
            )
        })
        .collect();

    events_received.emit(CountByteSize(
        decoded_metrics.len(),
        decoded_metrics.estimated_json_encoded_size_of(),
    ));

    Ok(decoded_metrics)
}

fn into_metric_tags(tags: Vec<String>) -> MetricTags {
    tags.into_iter()
        .map(|tag| extract_tag_key_and_value(&tag))
        .collect()
}

fn into_vector_metric(
    dd_metric: DatadogSeriesMetric,
    api_key: Option<Arc<str>>,
    schema_definition: &Arc<schema::Definition>,
    split_metric_namespace: bool,
) -> Vec<Event> {
    // Avoid copying large units or expanding tags for a series that cannot emit an event.
    if dd_metric.points.is_empty() {
        return Vec::new();
    }

    let mut tags = into_metric_tags(dd_metric.tags.unwrap_or_default());
    let mut event_metadata = dd_metric
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.origin.as_ref())
        .map_or_else(EventMetadata::default, |origin| {
            EventMetadata::default().with_origin_metadata(origin.clone())
        });
    if let Some(unit) = dd_metric.unit.filter(|unit| !unit.is_empty()) {
        event_metadata.set_datadog_metric_unit(unit);
    }
    if let Some(api_key) = api_key {
        event_metadata.set_datadog_api_key(api_key);
    }
    event_metadata.set_schema_definition(schema_definition);

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

    let (namespace, name) = if split_metric_namespace {
        namespace_name_from_dd_metric(&dd_metric.metric)
    } else {
        (None, dd_metric.metric.as_str())
    };

    match dd_metric.r#type {
        DatadogMetricType::Count => dd_metric
            .points
            .iter()
            .map(|dd_point| {
                Metric::new_with_metadata(
                    name.to_string(),
                    MetricKind::Incremental,
                    MetricValue::Counter { value: dd_point.1 },
                    event_metadata.clone(),
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
                Metric::new_with_metadata(
                    name.to_string(),
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: dd_point.1 },
                    event_metadata.clone(),
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
                Metric::new_with_metadata(
                    name.to_string(),
                    MetricKind::Incremental,
                    MetricValue::Counter {
                        value: dd_point.1 * f64::from(i),
                    },
                    event_metadata.clone(),
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
    .map(Event::from)
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

#[allow(
    clippy::ref_option,
    reason = "Changing this shared optional metadata signature is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Preserve the existing numeric conversion semantics of this source. https://github.com/vectordotdev/vector/issues/23659"
)]
pub(crate) fn decode_ddsketch(
    frame: Bytes,
    api_key: &Option<Arc<str>>,
    split_metric_namespace: bool,
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

            let event_metadata = get_event_metadata(sketch_series.metadata.as_ref(), None);

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
                let (namespace, name) = if split_metric_namespace {
                    namespace_name_from_dd_metric(&sketch_series.metric)
                } else {
                    (None, sketch_series.metric.as_str())
                };
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
