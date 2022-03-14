use std::{collections::BTreeMap, sync::Arc};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use futures::future;
use http::StatusCode;
use prost::Message;
use serde::{Deserialize, Serialize};
use vector_core::{metrics::AgentDDSketch, ByteSizeOf};
use warp::{filters::BoxedFilter, path, path::FullPath, reply::Response, Filter, Rejection};

use crate::{
    common::datadog::{DatadogMetricType, DatadogSeriesMetric},
    config::log_schema,
    event::{
        metric::{Metric, MetricValue},
        Event, MetricKind,
    },
    internal_events::EventsReceived,
    schema,
    sources::{
        datadog::agent::{self, handle_request, ApiKeyQueryParams, DatadogAgentSource},
        util::ErrorMessage,
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
    let sketches_service = sketches_service(
        acknowledgements,
        multiple_outputs,
        out.clone(),
        source.clone(),
    );
    let series_v1_service = series_v1_service(acknowledgements, multiple_outputs, out, source);
    let series_v2_service = series_v2_service();
    sketches_service
        .or(series_v1_service)
        .unify()
        .or(series_v2_service)
        .unify()
        .boxed()
}

fn sketches_service(
    acknowledgements: bool,
    multiple_outputs: bool,
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
                            &source.metrics_schema_definition,
                        )
                    });
                if multiple_outputs {
                    handle_request(events, acknowledgements, out.clone(), Some(agent::METRICS))
                } else {
                    handle_request(events, acknowledgements, out.clone(), None)
                }
            },
        )
        .boxed()
}

fn series_v1_service(
    acknowledgements: bool,
    multiple_outputs: bool,
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
                        decode_datadog_series(
                            body,
                            source.api_key_extractor.extract(
                                path.as_str(),
                                api_token,
                                query_params.dd_api_key,
                            ),
                            &source.metrics_schema_definition,
                        )
                    });
                if multiple_outputs {
                    handle_request(events, acknowledgements, out.clone(), Some(agent::METRICS))
                } else {
                    handle_request(events, acknowledgements, out.clone(), None)
                }
            },
        )
        .boxed()
}

fn series_v2_service() -> BoxedFilter<(Response,)> {
    warp::post()
        // This should not happen anytime soon as the v2 series endpoint does not exist yet
        // but the route exists in the agent codebase
        .and(path!("api" / "v2" / "series" / ..))
        .and_then(|| {
            error!(message = "/api/v2/series route is not supported.");
            let response: Result<Response, Rejection> =
                Err(warp::reject::custom(ErrorMessage::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Vector does not support the /api/v2/series route".to_string(),
                )));
            future::ready(response)
        })
        .boxed()
}

fn decode_datadog_sketches(
    body: Bytes,
    api_key: Option<Arc<str>>,
    schema_definition: &Arc<schema::Definition>,
) -> Result<Vec<Event>, ErrorMessage> {
    if body.is_empty() {
        // The datadog agent may send an empty payload as a keep alive
        debug!(
            message = "Empty payload ignored.",
            internal_log_rate_secs = 30
        );
        return Ok(Vec::new());
    }

    let metrics = decode_ddsketch(body, &api_key, schema_definition).map_err(|error| {
        ErrorMessage::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Error decoding Datadog sketch: {:?}", error),
        )
    })?;

    emit!(&EventsReceived {
        byte_size: metrics.size_of(),
        count: metrics.len(),
    });

    Ok(metrics)
}

fn decode_datadog_series(
    body: Bytes,
    api_key: Option<Arc<str>>,
    schema_definition: &Arc<schema::Definition>,
) -> Result<Vec<Event>, ErrorMessage> {
    if body.is_empty() {
        // The datadog agent may send an empty payload as a keep alive
        debug!(
            message = "Empty payload ignored.",
            internal_log_rate_secs = 30
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

    emit!(&EventsReceived {
        byte_size: decoded_metrics.size_of(),
        count: decoded_metrics.len(),
    });

    Ok(decoded_metrics)
}

fn into_vector_metric(
    dd_metric: DatadogSeriesMetric,
    api_key: Option<Arc<str>>,
    schema_definition: &Arc<schema::Definition>,
) -> Vec<Event> {
    let mut tags: BTreeMap<String, String> = dd_metric
        .tags
        .unwrap_or_default()
        .iter()
        .map(|tag| {
            let kv = tag.split_once(":").unwrap_or((tag, ""));
            (kv.0.trim().into(), kv.1.trim().into())
        })
        .collect();

    dd_metric
        .host
        .and_then(|host| tags.insert(log_schema().host_key().to_owned(), host));
    dd_metric
        .source_type_name
        .and_then(|source| tags.insert("source_type_name".into(), source));
    dd_metric
        .device
        .and_then(|dev| tags.insert("device".into(), dev));

    match dd_metric.r#type {
        DatadogMetricType::Count => dd_metric
            .points
            .iter()
            .map(|dd_point| {
                Metric::new(
                    dd_metric.metric.clone(),
                    MetricKind::Incremental,
                    MetricValue::Counter { value: dd_point.1 },
                )
                .with_timestamp(Some(Utc.timestamp(dd_point.0, 0)))
                .with_tags(Some(tags.clone()))
            })
            .collect::<Vec<_>>(),
        DatadogMetricType::Gauge => dd_metric
            .points
            .iter()
            .map(|dd_point| {
                Metric::new(
                    dd_metric.metric.clone(),
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: dd_point.1 },
                )
                .with_timestamp(Some(Utc.timestamp(dd_point.0, 0)))
                .with_tags(Some(tags.clone()))
            })
            .collect::<Vec<_>>(),
        // Agent sends rate only for dogstatsd counter https://github.com/DataDog/datadog-agent/blob/f4a13c6dca5e2da4bb722f861a8ac4c2f715531d/pkg/metrics/counter.go#L8-L10
        // for consistency purpose (w.r.t. (dog)statsd source) they are turned back into counters
        DatadogMetricType::Rate => dd_metric
            .points
            .iter()
            .map(|dd_point| {
                let i = dd_metric.interval.filter(|v| *v != 0).unwrap_or(1) as f64;
                Metric::new(
                    dd_metric.metric.clone(),
                    MetricKind::Incremental,
                    MetricValue::Counter {
                        value: dd_point.1 * i,
                    },
                )
                .with_timestamp(Some(Utc.timestamp(dd_point.0, 0)))
                .with_tags(Some(tags.clone()))
            })
            .collect::<Vec<_>>(),
    }
    .into_iter()
    .map(|mut metric| {
        if let Some(k) = &api_key {
            metric
                .metadata_mut()
                .set_datadog_api_key(Some(Arc::clone(k)));
        }

        metric
            .metadata_mut()
            .set_schema_definition(schema_definition);

        metric.into()
    })
    .collect()
}

mod dd_proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

use dd_proto::SketchPayload;

pub(crate) fn decode_ddsketch(
    frame: Bytes,
    api_key: &Option<Arc<str>>,
    schema_definition: &Arc<schema::Definition>,
) -> crate::Result<Vec<Event>> {
    let payload = SketchPayload::decode(frame)?;
    // payload.metadata is always empty for payload coming from dd agents
    Ok(payload
        .sketches
        .into_iter()
        .flat_map(|sketch_series| {
            // sketch_series.distributions is also always empty from payload coming from dd agents
            let mut tags: BTreeMap<String, String> = sketch_series
                .tags
                .iter()
                .map(|tag| {
                    let kv = tag.split_once(":").unwrap_or((tag, ""));
                    (kv.0.trim().into(), kv.1.trim().into())
                })
                .collect();

            tags.insert(
                log_schema().host_key().to_string(),
                sketch_series.host.clone(),
            );
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
                let mut metric =
                    Metric::new(sketch_series.metric.clone(), MetricKind::Incremental, val)
                        .with_tags(Some(tags.clone()))
                        .with_timestamp(Some(Utc.timestamp(sketch.ts, 0)));
                if let Some(k) = &api_key {
                    metric
                        .metadata_mut()
                        .set_datadog_api_key(Some(Arc::clone(k)));
                }

                metric
                    .metadata_mut()
                    .set_schema_definition(schema_definition);
                metric.into()
            })
        })
        .collect())
}
