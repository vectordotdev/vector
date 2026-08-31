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
use super::ddmetric_v3_proto::Payload as MetricPayloadV3;
use super::{ApiKeyQueryParams, DatadogAgentSource, RequestHandler};
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
                        decode_datadog_sketches(
                            body,
                            source.api_key_extractor.extract(
                                path.as_str(),
                                api_token,
                                query_params.dd_api_key,
                            ),
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
                        decode_datadog_series_v2(
                            body,
                            source.api_key_extractor.extract(
                                path.as_str(),
                                api_token,
                                query_params.dd_api_key,
                            ),
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
        .and(
            path!("api" / "intake" / "metrics" / "v3" / "series" / ..)
                .or(path!(
                    "api" / "intake" / "metrics" / "v3beta" / "series" / ..
                ))
                .unify()
                .and(warp::path::full()),
        )
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
                        decode_datadog_series_v3(
                            body,
                            source.api_key_extractor.extract(
                                path.as_str(),
                                api_token,
                                query_params.dd_api_key,
                            ),
                            source.split_metric_namespace,
                            &source.events_received,
                        )
                    });
                handler.clone().handle_request(events, super::METRICS)
            }
        })
        .boxed()
}

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

pub(crate) fn decode_ddseries_v3(
    frame: Bytes,
    api_key: &Option<Arc<str>>,
    split_metric_namespace: bool,
) -> crate::Result<Vec<Event>> {
    let payload = MetricPayloadV3::decode(frame)?;
    let series = payload.metric_data.map_or(Ok(Vec::new()), |metric_data| {
        decode_v3_metric_data(&metric_data, payload.metadata.as_ref())
    })?;
    decode_ddseries(series, api_key, split_metric_namespace)
}

pub(super) fn decode_v3_metric_data(
    data: &super::ddmetric_v3_proto::MetricData,
    metadata: Option<&super::ddmetric_v3_proto::Metadata>,
) -> crate::Result<Vec<super::ddmetric_proto::metric_payload::MetricSeries>> {
    let names = decode_v3_strings(&data.dict_name_str, false)?;
    let tag_strings = decode_v3_strings(&data.dict_tag_str, true)?;
    let units = decode_v3_strings(&data.dict_unit_str, false)?;
    let tagsets = decode_v3_tagsets(&data.dict_tagsets, &tag_strings, metadata)?;
    let resources = decode_v3_resources(data, metadata)?;
    let source_types = decode_v3_strings(&data.dict_source_type_name, false)?;
    let origin_count = data.dict_origin_info.len() / 3 + 1;

    if !data.dict_origin_info.len().is_multiple_of(3)
        || data.name_refs.len() < data.types.len()
        || data.tagset_refs.len() < data.types.len()
        || data.resources_refs.len() < data.types.len()
        || data.intervals.len() < data.types.len()
        || data.num_points.len() < data.types.len()
        || data.source_type_name_refs.len() < data.types.len()
        || data.origin_info_refs.len() < data.types.len()
    {
        return Err("invalid Datadog v3 metric columns".into());
    }

    let mut name_ref = 0;
    let mut tagset_ref = 0;
    let mut resources_ref = 0;
    let mut source_type_ref = 0;
    let mut origin_ref = 0;
    let mut unit_ref = 0;
    let mut unit_idx = 0;
    let mut timestamp: i64 = 0;
    let mut timestamp_idx = 0;
    let mut sint64_idx = 0;
    let mut float32_idx = 0;
    let mut float64_idx = 0;
    let mut series = Vec::with_capacity(data.types.len());

    for index in 0..data.types.len() {
        name_ref += data.name_refs[index];
        tagset_ref += data.tagset_refs[index];
        resources_ref += data.resources_refs[index];
        source_type_ref += data.source_type_name_refs[index];
        origin_ref += data.origin_info_refs[index];
        if name_ref < 0
            || name_ref as usize >= names.len()
            || tagset_ref < 0
            || tagset_ref as usize >= tagsets.len()
            || resources_ref < 0
            || resources_ref as usize >= resources.len()
            || source_type_ref < 0
            || source_type_ref as usize >= source_types.len()
            || origin_ref < 0
            || origin_ref as usize >= origin_count
        {
            return Err("invalid Datadog v3 dictionary reference".into());
        }

        let packed_type = data.types[index];
        let metric_type = packed_type & 0x0f;
        let value_type = packed_type & 0xf0;
        if metric_type == 4 {
            return Err("Datadog v3 series payload contains a sketch".into());
        }
        let unit = if packed_type & 0x200 != 0 {
            if unit_idx >= data.unit_refs.len() {
                return Err("invalid Datadog v3 unit column".into());
            }
            unit_ref += data.unit_refs[unit_idx];
            unit_idx += 1;
            if unit_ref < 0 || unit_ref as usize >= units.len() {
                return Err("invalid Datadog v3 unit reference".into());
            }
            units[unit_ref as usize].clone()
        } else {
            String::new()
        };

        let point_count = usize::try_from(data.num_points[index])
            .map_err(|_| "invalid Datadog v3 point count")?;
        let remaining_timestamps = data.timestamps.len().saturating_sub(timestamp_idx);
        let remaining_values = match value_type {
            0 => remaining_timestamps,
            0x10 => data.vals_sint64.len().saturating_sub(sint64_idx),
            0x20 => data.vals_float32.len().saturating_sub(float32_idx),
            0x30 => data.vals_float64.len().saturating_sub(float64_idx),
            _ => return Err("invalid Datadog v3 value type".into()),
        };
        if point_count > remaining_timestamps || point_count > remaining_values {
            return Err("invalid Datadog v3 point count".into());
        }
        let mut points = Vec::with_capacity(point_count);
        for _ in 0..point_count {
            if timestamp_idx >= data.timestamps.len() {
                return Err("invalid Datadog v3 timestamp column".into());
            }
            timestamp = timestamp
                .checked_add(data.timestamps[timestamp_idx])
                .ok_or("Datadog v3 timestamp overflow")?;
            timestamp_idx += 1;
            let value = match value_type {
                0 => 0.0,
                0x10 => {
                    let value = data
                        .vals_sint64
                        .get(sint64_idx)
                        .ok_or("invalid Datadog v3 value column")?;
                    sint64_idx += 1;
                    *value as f64
                }
                0x20 => {
                    let value = data
                        .vals_float32
                        .get(float32_idx)
                        .ok_or("invalid Datadog v3 value column")?;
                    float32_idx += 1;
                    *value as f64
                }
                0x30 => {
                    let value = data
                        .vals_float64
                        .get(float64_idx)
                        .ok_or("invalid Datadog v3 value column")?;
                    float64_idx += 1;
                    *value
                }
                _ => return Err("invalid Datadog v3 value type".into()),
            };
            points.push(super::ddmetric_proto::metric_payload::MetricPoint { value, timestamp });
        }

        let metric_type = match metric_type {
            1 => metric_payload::MetricType::Count,
            2 => metric_payload::MetricType::Rate,
            3 => metric_payload::MetricType::Gauge,
            _ => metric_payload::MetricType::Unspecified,
        };
        let no_index = packed_type & 0x100 != 0;
        let metadata = if origin_ref == 0 && !no_index {
            None
        } else {
            let (origin_product, origin_category, origin_service) = if origin_ref == 0 {
                (0, 0, 0)
            } else {
                let offset = (origin_ref as usize - 1) * 3;
                (
                    u32::try_from(data.dict_origin_info[offset])
                        .map_err(|_| "invalid Datadog v3 origin product")?,
                    u32::try_from(data.dict_origin_info[offset + 1])
                        .map_err(|_| "invalid Datadog v3 origin category")?,
                    u32::try_from(data.dict_origin_info[offset + 2])
                        .map_err(|_| "invalid Datadog v3 origin service")?,
                )
            };
            Some(Metadata {
                origin: Some(super::ddmetric_proto::Origin {
                    metric_type: if no_index { 9 } else { 0 },
                    origin_product,
                    origin_category,
                    origin_service,
                }),
            })
        };
        let resources = resources[resources_ref as usize]
            .iter()
            .map(
                |(resource_type, name)| super::ddmetric_proto::metric_payload::Resource {
                    r#type: resource_type.clone(),
                    name: name.clone(),
                },
            )
            .collect();
        series.push(super::ddmetric_proto::metric_payload::MetricSeries {
            resources,
            metric: names[name_ref as usize].clone(),
            tags: tagsets[tagset_ref as usize].clone(),
            points,
            r#type: metric_type as i32,
            unit,
            source_type_name: source_types[source_type_ref as usize].clone(),
            interval: i64::from(
                u32::try_from(data.intervals[index]).map_err(|_| "invalid Datadog v3 interval")?,
            ),
            metadata,
        });
    }
    Ok(series)
}

fn decode_v3_strings(raw: &[u8], sanitize: bool) -> crate::Result<Vec<String>> {
    let mut values = vec![String::new()];
    let mut offset = 0;
    while offset < raw.len() {
        let (length, consumed) = decode_v3_varint(&raw[offset..])?;
        offset += consumed;
        let length = usize::try_from(length).map_err(|_| "Datadog v3 string length overflow")?;
        let end = offset
            .checked_add(length)
            .ok_or("Datadog v3 string length overflow")?;
        if end > raw.len() {
            return Err("truncated Datadog v3 string dictionary".into());
        }
        let bytes = &raw[offset..end];
        let value = match std::str::from_utf8(bytes) {
            Ok(value) => value.to_owned(),
            Err(_) if sanitize => String::from_utf8_lossy(bytes).into_owned(),
            Err(error) => return Err(error.into()),
        };
        values.push(value);
        offset = end;
    }
    Ok(values)
}

fn decode_v3_varint(raw: &[u8]) -> crate::Result<(u64, usize)> {
    let mut value = 0;
    for (index, byte) in raw.iter().copied().enumerate() {
        if index == 10 {
            return Err("Datadog v3 varint overflow".into());
        }
        if index == 9 && byte > 1 {
            return Err("Datadog v3 varint overflow".into());
        }
        value |= u64::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            return Ok((value, index + 1));
        }
    }
    Err("truncated Datadog v3 varint".into())
}

fn decode_v3_tagsets(
    packed: &[i64],
    dictionary: &[String],
    metadata: Option<&super::ddmetric_v3_proto::Metadata>,
) -> crate::Result<Vec<Vec<String>>> {
    let mut tagsets = vec![Vec::new()];
    let mut offset = 0;
    while offset < packed.len() {
        let size = usize::try_from(packed[offset]).map_err(|_| "invalid Datadog v3 tagset size")?;
        offset += 1;
        let end = offset
            .checked_add(size)
            .ok_or("Datadog v3 tagset length overflow")?;
        if end > packed.len() {
            return Err("truncated Datadog v3 tagset dictionary".into());
        }
        let mut tags = Vec::new();
        let mut reference: i64 = 0;
        for delta in &packed[offset..end] {
            reference = reference
                .checked_add(*delta)
                .ok_or("Datadog v3 tagset reference overflow")?;
            if reference < 0 {
                let index = usize::try_from(
                    reference
                        .checked_neg()
                        .ok_or("Datadog v3 tagset reference overflow")?,
                )
                .map_err(|_| "invalid Datadog v3 tagset reference")?;
                tags.extend(
                    tagsets
                        .get(index)
                        .ok_or("invalid Datadog v3 tagset reference")?
                        .iter()
                        .cloned(),
                );
            } else {
                tags.push(
                    dictionary
                        .get(reference as usize)
                        .ok_or("invalid Datadog v3 tag reference")?
                        .clone(),
                );
            }
        }
        tagsets.push(tags);
        offset = end;
    }
    if let Some(metadata) = metadata {
        for tagset in &mut tagsets {
            for tag in &metadata.tags {
                if !tagset.contains(tag) {
                    tagset.push(tag.clone());
                }
            }
        }
    }
    Ok(tagsets)
}

fn decode_v3_resources(
    data: &super::ddmetric_v3_proto::MetricData,
    metadata: Option<&super::ddmetric_v3_proto::Metadata>,
) -> crate::Result<Vec<Vec<(String, String)>>> {
    let dictionary = decode_v3_strings(&data.dict_resource_str, false)?;
    let mut resources = vec![Vec::new()];
    let mut entry_offset: usize = 0;
    for &resource_len in &data.dict_resource_len {
        let size = usize::try_from(resource_len).map_err(|_| "invalid Datadog v3 resource size")?;
        let end = entry_offset
            .checked_add(size)
            .ok_or("Datadog v3 resource length overflow")?;
        if end > data.dict_resource_type.len() || end > data.dict_resource_name.len() {
            return Err("truncated Datadog v3 resource dictionary".into());
        }
        let mut set = Vec::with_capacity(size);
        let mut type_ref: i64 = 0;
        let mut name_ref: i64 = 0;
        for index in entry_offset..end {
            type_ref = type_ref
                .checked_add(data.dict_resource_type[index])
                .ok_or("Datadog v3 resource reference overflow")?;
            name_ref = name_ref
                .checked_add(data.dict_resource_name[index])
                .ok_or("Datadog v3 resource reference overflow")?;
            if type_ref < 0 || name_ref < 0 {
                return Err("invalid Datadog v3 resource reference".into());
            }
            set.push((
                dictionary
                    .get(type_ref as usize)
                    .ok_or("invalid Datadog v3 resource reference")?
                    .clone(),
                dictionary
                    .get(name_ref as usize)
                    .ok_or("invalid Datadog v3 resource reference")?
                    .clone(),
            ));
        }
        resources.push(set);
        entry_offset = end;
    }
    if let Some(metadata) = metadata {
        if metadata.resources.len() % 2 != 0 {
            return Err("Datadog v3 metadata resources must be pairs".into());
        }
        for set in &mut resources {
            for pair in metadata.resources.chunks_exact(2) {
                set.push((pair[0].clone(), pair[1].clone()));
            }
        }
    }
    Ok(resources)
}

/// Builds Vector's `EventMetadata` from the received metadata. Currently this is only
/// utilized for passing through origin metadata set by the Agent.
fn get_event_metadata(metadata: Option<&Metadata>) -> EventMetadata {
    metadata
        .and_then(|metadata| metadata.origin.as_ref())
        .map_or_else(EventMetadata::default, |origin| {
            trace!(
                "Deserialized origin_product: `{}` origin_category: `{}` origin_service: `{}`.",
                origin.origin_product, origin.origin_category, origin.origin_service,
            );
            EventMetadata::default().with_origin_metadata(
                DatadogMetricOriginMetadata::new(
                    Some(origin.origin_product),
                    Some(origin.origin_category),
                    Some(origin.origin_service),
                )
                .with_metric_type((origin.metric_type != 0).then_some(origin.metric_type)),
            )
        })
}

pub(crate) fn decode_ddseries_v2(
    frame: Bytes,
    api_key: &Option<Arc<str>>,
    split_metric_namespace: bool,
) -> crate::Result<Vec<Event>> {
    let payload = MetricPayload::decode(frame)?;
    decode_ddseries(payload.series, api_key, split_metric_namespace)
}

fn decode_ddseries(
    series: Vec<metric_payload::MetricSeries>,
    api_key: &Option<Arc<str>>,
    split_metric_namespace: bool,
) -> crate::Result<Vec<Event>> {
    let decoded_metrics: Vec<Event> = series
        .into_iter()
        .flat_map(|serie| {
            let (namespace, name) = if split_metric_namespace {
                namespace_name_from_dd_metric(&serie.metric)
            } else {
                (None, serie.metric.as_str())
            };
            let mut tags = into_metric_tags(serie.tags);

            let mut event_metadata = get_event_metadata(serie.metadata.as_ref());
            if !serie.unit.is_empty() {
                event_metadata.set_datadog_metric_unit(serie.unit.clone());
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
    tags.iter().map(extract_tag_key_and_value).collect()
}

fn into_vector_metric(
    dd_metric: DatadogSeriesMetric,
    api_key: Option<Arc<str>>,
    schema_definition: &Arc<schema::Definition>,
    split_metric_namespace: bool,
) -> Vec<Event> {
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
                        value: dd_point.1 * (i as f64),
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
