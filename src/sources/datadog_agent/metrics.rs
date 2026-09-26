use std::{collections::HashSet, num::NonZeroU32, sync::Arc};

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

use super::ddmetric_proto::{
    Metadata, MetricPayload, SketchPayload, metric_payload,
    metric_payload::{MetricPoint, MetricSeries, Resource},
};
use super::ddmetric_v3_proto::{
    Metadata as MetricMetadataV3, MetricData as MetricDataV3, Payload as MetricPayloadV3,
};
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

const MAX_V3_EXPANDED_TAGSET_BYTES: usize = 16 * 1024 * 1024;
const MAX_V3_EXPANDED_RESOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_V3_EXPANDED_SERIES_BYTES: usize = 16 * 1024 * 1024;
const MAX_V3_EXPANDED_EVENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_V3_DECODED_REPEATED_BYTES: usize = 16 * 1024 * 1024;

// `MetricData.types` packs three independent values into each integer. These masks mirror
// `metricType`, `valueType`, and `metricFlags` in Datadog's `intake_v3.proto`.
const V3_METRIC_TYPE_MASK: u64 = 0x0f;
const V3_VALUE_TYPE_MASK: u64 = 0xf0;
const V3_FLAG_NO_INDEX: u64 = 0x100;
const V3_FLAG_HAS_UNIT: u64 = 0x200;
const V3_METRIC_TYPE_SKETCH: u64 = 4;
const V3_VALUE_TYPE_ZERO: u64 = 0;
const V3_VALUE_TYPE_SINT64: u64 = 0x10;
const V3_VALUE_TYPE_FLOAT32: u64 = 0x20;
const V3_VALUE_TYPE_FLOAT64: u64 = 0x30;

// Datadog's origin metric-type value for metrics excluded from indexing. This is event metadata,
// not the metric kind encoded in the low bits of `MetricData.types`.
const DATADOG_ORIGIN_METRIC_TYPE_NO_INDEX: i32 = 9;

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
        .and(path!("api" / "intake" / "metrics" / "v3" / "series" / ..).and(warp::path::full()))
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
    validate_v3_predecode_allocations(&frame)?;
    let payload = MetricPayloadV3::decode(frame)?;
    // `types` combines the metric kind, point value encoding, and flags. `flagNoIndex` is not
    // represented by the v2 `MetricSeries` used during translation, so carry its Datadog origin
    // value alongside each translated series until `decode_ddseries` builds the Vector events.
    let metric_types = payload.metric_data.as_ref().map(|metric_data| {
        metric_data
            .types
            .iter()
            .map(|packed_type| {
                (packed_type & V3_FLAG_NO_INDEX != 0).then_some(DATADOG_ORIGIN_METRIC_TYPE_NO_INDEX)
            })
            .collect::<Vec<_>>()
    });
    let series = payload.metric_data.map_or(Ok(Vec::new()), |metric_data| {
        decode_v3_metric_data(&metric_data, payload.metadata.as_ref())
    })?;
    decode_ddseries(
        series,
        api_key,
        split_metric_namespace,
        metric_types.as_deref(),
    )
}

fn validate_v3_metric_columns(
    data: &MetricDataV3,
    metadata: Option<&MetricMetadataV3>,
) -> crate::Result<()> {
    if metadata.is_some_and(|metadata| !metadata.resources.len().is_multiple_of(2)) {
        return Err("Datadog v3 metadata resources must be pairs".into());
    }

    let series_count = data.types.len();
    if !data.dict_origin_info.len().is_multiple_of(3)
        || data.name_refs.len() < series_count
        || data.tagset_refs.len() < series_count
        || data.resources_refs.len() < series_count
        || data.intervals.len() < series_count
        || data.num_points.len() < series_count
        || data.source_type_name_refs.len() < series_count
        || data.origin_info_refs.len() < series_count
    {
        return Err("invalid Datadog v3 metric columns".into());
    }
    Ok(())
}

fn v3_string_list_bytes<'a>(values: impl IntoIterator<Item = &'a String>) -> crate::Result<usize> {
    values
        .into_iter()
        .try_fold(0, |total, value| checked_v3_string_bytes(total, value))
}

fn v3_resource_list_bytes(resources: &[(String, String)]) -> crate::Result<usize> {
    resources
        .iter()
        .try_fold(0, |total, (resource_type, name)| {
            let total = checked_v3_string_bytes(total, resource_type)?;
            checked_v3_string_bytes(total, name)
        })
}

#[derive(Default)]
struct V3ReferenceDecoder {
    name: i64,
    tagset: i64,
    resources: i64,
    source_type: i64,
    origin: i64,
    unit: i64,
    unit_index: usize,
}

struct V3References {
    name: usize,
    tagset: usize,
    resources: usize,
    source_type: usize,
    origin: usize,
}

impl V3ReferenceDecoder {
    fn advance_reference(
        current: &mut i64,
        delta: i64,
        dictionary_len: usize,
        overflow_error: &'static str,
    ) -> crate::Result<usize> {
        *current = current.checked_add(delta).ok_or(overflow_error)?;
        usize::try_from(*current)
            .ok()
            .filter(|index| *index < dictionary_len)
            .ok_or_else(|| "invalid Datadog v3 dictionary reference".into())
    }

    fn advance(
        &mut self,
        data: &MetricDataV3,
        index: usize,
        dictionary_lengths: [usize; 5],
    ) -> crate::Result<V3References> {
        Ok(V3References {
            name: Self::advance_reference(
                &mut self.name,
                data.name_refs[index],
                dictionary_lengths[0],
                "Datadog v3 name reference overflow",
            )?,
            tagset: Self::advance_reference(
                &mut self.tagset,
                data.tagset_refs[index],
                dictionary_lengths[1],
                "Datadog v3 tagset reference overflow",
            )?,
            resources: Self::advance_reference(
                &mut self.resources,
                data.resources_refs[index],
                dictionary_lengths[2],
                "Datadog v3 resource reference overflow",
            )?,
            source_type: Self::advance_reference(
                &mut self.source_type,
                data.source_type_name_refs[index],
                dictionary_lengths[3],
                "Datadog v3 source type reference overflow",
            )?,
            origin: Self::advance_reference(
                &mut self.origin,
                data.origin_info_refs[index],
                dictionary_lengths[4],
                "Datadog v3 origin reference overflow",
            )?,
        })
    }

    fn decode_unit(
        &mut self,
        data: &MetricDataV3,
        units: &[String],
        packed_type: u64,
    ) -> crate::Result<String> {
        if packed_type & V3_FLAG_HAS_UNIT == 0 {
            return Ok(String::new());
        }
        let delta = *data
            .unit_refs
            .get(self.unit_index)
            .ok_or("invalid Datadog v3 unit column")?;
        self.unit_index += 1;
        let index = Self::advance_reference(
            &mut self.unit,
            delta,
            units.len(),
            "Datadog v3 unit reference overflow",
        )?;
        Ok(units[index].clone())
    }
}

struct V3PointDecoder<'a> {
    data: &'a MetricDataV3,
    timestamp: i64,
    timestamp_index: usize,
    sint64_index: usize,
    float32_index: usize,
    float64_index: usize,
}

impl<'a> V3PointDecoder<'a> {
    const fn new(data: &'a MetricDataV3) -> Self {
        Self {
            data,
            timestamp: 0,
            timestamp_index: 0,
            sint64_index: 0,
            float32_index: 0,
            float64_index: 0,
        }
    }

    fn validate_count(&self, value_type: u64, point_count: usize) -> crate::Result<()> {
        let timestamp_count = self
            .data
            .timestamps
            .len()
            .saturating_sub(self.timestamp_index);
        let value_count = match value_type {
            V3_VALUE_TYPE_ZERO => timestamp_count,
            V3_VALUE_TYPE_SINT64 => self
                .data
                .vals_sint64
                .len()
                .saturating_sub(self.sint64_index),
            V3_VALUE_TYPE_FLOAT32 => self
                .data
                .vals_float32
                .len()
                .saturating_sub(self.float32_index),
            V3_VALUE_TYPE_FLOAT64 => self
                .data
                .vals_float64
                .len()
                .saturating_sub(self.float64_index),
            _ => return Err("invalid Datadog v3 value type".into()),
        };
        if point_count > timestamp_count || point_count > value_count {
            return Err("invalid Datadog v3 point count".into());
        }
        Ok(())
    }

    fn decode_points(
        &mut self,
        value_type: u64,
        point_count: usize,
    ) -> crate::Result<Vec<MetricPoint>> {
        self.validate_count(value_type, point_count)?;
        let mut points = Vec::with_capacity(point_count);
        for _ in 0..point_count {
            self.timestamp = self
                .timestamp
                .checked_add(self.data.timestamps[self.timestamp_index])
                .ok_or("Datadog v3 timestamp overflow")?;
            if Utc.timestamp_opt(self.timestamp, 0).single().is_none() {
                return Err("invalid Datadog v3 timestamp".into());
            }
            self.timestamp_index += 1;
            let value = match value_type {
                V3_VALUE_TYPE_ZERO => 0.0,
                V3_VALUE_TYPE_SINT64 => {
                    let value = self.data.vals_sint64[self.sint64_index] as f64;
                    self.sint64_index += 1;
                    value
                }
                V3_VALUE_TYPE_FLOAT32 => {
                    let value = f64::from(self.data.vals_float32[self.float32_index]);
                    self.float32_index += 1;
                    value
                }
                V3_VALUE_TYPE_FLOAT64 => {
                    let value = self.data.vals_float64[self.float64_index];
                    self.float64_index += 1;
                    value
                }
                _ => return Err("invalid Datadog v3 value type".into()),
            };
            points.push(MetricPoint {
                value,
                timestamp: self.timestamp,
            });
        }
        Ok(points)
    }
}

#[derive(Default)]
struct V3AllocationBudget {
    series_bytes: usize,
    event_bytes: usize,
}

impl V3AllocationBudget {
    /// Accounts for every clone and vector expansion performed for one series.
    ///
    /// Callers must invoke this before allocating points, tags, resources, or events for the
    /// series. Successful return means both cumulative expansion limits still hold.
    fn check_before_allocating(
        &mut self,
        point_count: usize,
        name: &str,
        unit: &str,
        source_type: &str,
        tag_bytes: usize,
        resource_bytes: usize,
    ) -> crate::Result<()> {
        let tag_resource_bytes = tag_bytes
            .checked_add(source_type.len())
            .and_then(|total| total.checked_add(resource_bytes))
            .ok_or("Datadog v3 expanded series size overflow")?;
        let tag_resource_copies = point_count
            .checked_add(1)
            .ok_or("Datadog v3 expanded series size overflow")?;
        let expanded_tag_resource_bytes = tag_resource_bytes
            .checked_mul(tag_resource_copies)
            .ok_or("Datadog v3 expanded series size overflow")?;
        let dictionary_string_bytes = checked_v3_string_bytes(0, name)?;
        let dictionary_string_bytes = checked_v3_string_bytes(dictionary_string_bytes, unit)?;
        let dictionary_string_bytes =
            checked_v3_string_bytes(dictionary_string_bytes, source_type)?;
        let series_bytes = std::mem::size_of::<MetricSeries>()
            .checked_add(
                point_count
                    .checked_mul(std::mem::size_of::<MetricPoint>())
                    .ok_or("Datadog v3 expanded series size overflow")?,
            )
            .and_then(|total| total.checked_add(expanded_tag_resource_bytes))
            .and_then(|total| total.checked_add(dictionary_string_bytes))
            .ok_or("Datadog v3 expanded series size overflow")?;
        self.series_bytes = self
            .series_bytes
            .checked_add(series_bytes)
            .ok_or("Datadog v3 expanded series size overflow")?;
        if self.series_bytes > MAX_V3_EXPANDED_SERIES_BYTES {
            return Err("Datadog v3 expanded series exceed size limit".into());
        }

        let event_bytes = std::mem::size_of::<Event>()
            .checked_add(name.len())
            .and_then(|total| total.checked_add(tag_bytes))
            .and_then(|total| total.checked_add(resource_bytes))
            .and_then(|total| total.checked_add(source_type.len()))
            .and_then(|total| total.checked_add(unit.len()))
            .ok_or("Datadog v3 expanded event size overflow")?;
        self.event_bytes = self
            .event_bytes
            .checked_add(
                event_bytes
                    .checked_mul(point_count)
                    .ok_or("Datadog v3 expanded event size overflow")?,
            )
            .ok_or("Datadog v3 expanded event size overflow")?;
        if self.event_bytes > MAX_V3_EXPANDED_EVENT_BYTES {
            return Err("Datadog v3 expanded events exceed size limit".into());
        }
        Ok(())
    }
}

pub(super) fn decode_v3_metric_data(
    data: &MetricDataV3,
    metadata: Option<&MetricMetadataV3>,
) -> crate::Result<Vec<MetricSeries>> {
    validate_v3_metric_columns(data, metadata)?;
    let names = decode_v3_strings(&data.dict_name_str, false)?;
    let tag_strings = decode_v3_strings(&data.dict_tag_str, true)?;
    let units = decode_v3_strings(&data.dict_unit_str, false)?;
    let tagsets = decode_v3_tagsets(&data.dict_tagsets, &tag_strings, metadata)?;
    let resources = decode_v3_resources(data)?;
    let source_types = decode_v3_strings(&data.dict_source_type_name, false)?;
    let origin_count = data.dict_origin_info.len() / 3 + 1;

    // The size of a dictionary entry is stable, so calculate it once rather than rescanning the
    // same tags and resources for every series that references it.
    let tagset_bytes = tagsets
        .iter()
        .map(|tags| v3_string_list_bytes(tags))
        .collect::<crate::Result<Vec<_>>>()?;
    let metadata_resource_bytes = metadata
        .map(|metadata| v3_string_list_bytes(&metadata.resources))
        .transpose()?
        .unwrap_or(0);
    let resource_bytes = resources
        .iter()
        .map(|entry| {
            v3_resource_list_bytes(entry)?
                .checked_add(metadata_resource_bytes)
                .ok_or_else(|| "Datadog v3 expanded resource size overflow".into())
        })
        .collect::<crate::Result<Vec<_>>>()?;

    let dictionary_lengths = [
        names.len(),
        tagsets.len(),
        resources.len(),
        source_types.len(),
        origin_count,
    ];
    let mut references = V3ReferenceDecoder::default();
    let mut point_decoder = V3PointDecoder::new(data);
    let mut allocation_budget = V3AllocationBudget::default();
    let mut series = Vec::new();

    for index in 0..data.types.len() {
        let refs = references.advance(data, index, dictionary_lengths)?;
        let packed_type = data.types[index];
        let metric_type = packed_type & V3_METRIC_TYPE_MASK;
        let value_type = packed_type & V3_VALUE_TYPE_MASK;
        if metric_type == V3_METRIC_TYPE_SKETCH {
            return Err("Datadog v3 series payload contains a sketch".into());
        }
        let unit = references.decode_unit(data, &units, packed_type)?;
        let point_count = usize::try_from(data.num_points[index])
            .map_err(|_| "invalid Datadog v3 point count")?;
        let name = &names[refs.name];
        let source_type = &source_types[refs.source_type];
        allocation_budget.check_before_allocating(
            point_count,
            name,
            &unit,
            source_type,
            tagset_bytes[refs.tagset],
            resource_bytes[refs.resources],
        )?;
        let points = point_decoder.decode_points(value_type, point_count)?;

        let metric_type = match metric_type {
            1 => metric_payload::MetricType::Count,
            2 => metric_payload::MetricType::Rate,
            3 => metric_payload::MetricType::Gauge,
            _ => metric_payload::MetricType::Unspecified,
        };
        let series_metadata = if refs.origin == 0 {
            None
        } else {
            let offset = (refs.origin - 1) * 3;
            Some(Metadata {
                origin: Some(super::ddmetric_proto::Origin {
                    origin_product: u32::try_from(data.dict_origin_info[offset])
                        .map_err(|_| "invalid Datadog v3 origin product")?,
                    origin_category: u32::try_from(data.dict_origin_info[offset + 1])
                        .map_err(|_| "invalid Datadog v3 origin category")?,
                    origin_service: u32::try_from(data.dict_origin_info[offset + 2])
                        .map_err(|_| "invalid Datadog v3 origin service")?,
                }),
            })
        };
        let mut decoded_resources: Vec<Resource> = resources[refs.resources]
            .iter()
            .map(|(resource_type, name)| Resource {
                r#type: resource_type.clone(),
                name: name.clone(),
            })
            .collect();
        if let Some(payload_metadata) = metadata {
            decoded_resources.extend(payload_metadata.resources.chunks_exact(2).map(|pair| {
                Resource {
                    r#type: pair[0].clone(),
                    name: pair[1].clone(),
                }
            }));
        }
        let interval =
            u32::try_from(data.intervals[index]).map_err(|_| "invalid Datadog v3 interval")?;
        interval
            .checked_mul(1000)
            .ok_or("Datadog v3 interval milliseconds overflow")?;
        series.push(MetricSeries {
            resources: decoded_resources,
            metric: name.clone(),
            tags: tagsets[refs.tagset].clone(),
            points,
            r#type: metric_type as i32,
            unit,
            source_type_name: source_type.clone(),
            interval: i64::from(interval),
            metadata: series_metadata,
        });
    }
    Ok(series)
}

fn decode_v3_strings(raw: &[u8], sanitize: bool) -> crate::Result<Vec<String>> {
    let mut values = vec![String::new()];
    let mut expanded_bytes = std::mem::size_of::<String>();
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
        let valid_string_bytes = expanded_bytes
            .checked_add(std::mem::size_of::<String>())
            .and_then(|total| total.checked_add(length))
            .ok_or("Datadog v3 expanded string dictionary size overflow")?;
        if valid_string_bytes > MAX_V3_EXPANDED_SERIES_BYTES {
            return Err("Datadog v3 expanded string dictionary exceeds size limit".into());
        }
        let value = match std::str::from_utf8(bytes) {
            Ok(value) => value.to_owned(),
            Err(_) if sanitize => {
                let worst_case_length = length
                    .checked_mul(3)
                    .ok_or("Datadog v3 expanded string dictionary size overflow")?;
                let worst_case_bytes = expanded_bytes
                    .checked_add(std::mem::size_of::<String>())
                    .and_then(|total| total.checked_add(worst_case_length))
                    .ok_or("Datadog v3 expanded string dictionary size overflow")?;
                if worst_case_bytes > MAX_V3_EXPANDED_SERIES_BYTES {
                    return Err("Datadog v3 expanded string dictionary exceeds size limit".into());
                }
                String::from_utf8_lossy(bytes).into_owned()
            }
            Err(error) => return Err(error.into()),
        };
        expanded_bytes = checked_v3_string_bytes(expanded_bytes, &value)?;
        if expanded_bytes > MAX_V3_EXPANDED_SERIES_BYTES {
            return Err("Datadog v3 expanded string dictionary exceeds size limit".into());
        }
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

fn validate_v3_predecode_allocations(raw: &[u8]) -> crate::Result<()> {
    let mut offset = 0;
    let mut metadata_bytes = 0usize;
    let mut repeated_numeric_bytes = 0usize;
    while offset < raw.len() {
        let (key, consumed) = decode_v3_varint(&raw[offset..])?;
        offset += consumed;
        let field = key >> 3;
        let wire_type = key & 0x07;
        if matches!(field, 2 | 3) && wire_type == 2 {
            let (message, end) = decode_v3_length_delimited(raw, offset)?;
            if field == 2 {
                validate_v3_metadata_allocations(message, &mut metadata_bytes)?;
            } else {
                validate_v3_metric_data_allocations(message, &mut repeated_numeric_bytes)?;
            }
            offset = end;
        } else {
            offset = skip_v3_field(raw, offset, wire_type)?;
        }
    }
    Ok(())
}

fn validate_v3_metadata_allocations(raw: &[u8], decoded_bytes: &mut usize) -> crate::Result<()> {
    let mut offset = 0;
    while offset < raw.len() {
        let (key, consumed) = decode_v3_varint(&raw[offset..])?;
        offset += consumed;
        let field = key >> 3;
        let wire_type = key & 0x07;
        if matches!(field, 1 | 2) && wire_type == 2 {
            let (value, end) = decode_v3_length_delimited(raw, offset)?;
            *decoded_bytes = decoded_bytes
                .checked_add(std::mem::size_of::<String>())
                .and_then(|total| total.checked_add(value.len()))
                .ok_or("Datadog v3 metadata string allocation overflow")?;
            if *decoded_bytes > MAX_V3_DECODED_REPEATED_BYTES {
                return Err("Datadog v3 metadata strings exceed size limit".into());
            }
            offset = end;
        } else {
            offset = skip_v3_field(raw, offset, wire_type)?;
        }
    }
    Ok(())
}

fn validate_v3_metric_data_allocations(raw: &[u8], decoded_bytes: &mut usize) -> crate::Result<()> {
    let mut offset = 0;
    while offset < raw.len() {
        let (key, consumed) = decode_v3_varint(&raw[offset..])?;
        offset += consumed;
        let field = key >> 3;
        let wire_type = key & 0x07;
        let element_size = match field {
            3 | 5..=7 | 10..=17 | 19 | 20 | 23 | 24 | 26 => 8,
            9 | 18 | 21 | 22 => 4,
            _ => {
                offset = skip_v3_field(raw, offset, wire_type)?;
                continue;
            }
        };
        let fixed_width = match field {
            18 => Some(4),
            19 => Some(8),
            _ => None,
        };
        let count = if wire_type == 2 {
            let (packed, end) = decode_v3_length_delimited(raw, offset)?;
            offset = end;
            match fixed_width {
                Some(width) => {
                    if packed.len() % width != 0 {
                        return Err("invalid packed Datadog v3 numeric column".into());
                    }
                    packed.len() / width
                }
                None => count_v3_packed_varints(packed)?,
            }
        } else if fixed_width.is_some_and(|width| wire_type == if width == 4 { 5 } else { 1 })
            || fixed_width.is_none() && wire_type == 0
        {
            offset = skip_v3_field(raw, offset, wire_type)?;
            1
        } else {
            offset = skip_v3_field(raw, offset, wire_type)?;
            continue;
        };
        let allocation = count
            .checked_mul(element_size)
            .ok_or("Datadog v3 numeric column allocation overflow")?;
        *decoded_bytes = decoded_bytes
            .checked_add(allocation)
            .ok_or("Datadog v3 numeric column allocation overflow")?;
        if *decoded_bytes > MAX_V3_DECODED_REPEATED_BYTES {
            return Err("Datadog v3 numeric columns exceed size limit".into());
        }
    }
    Ok(())
}

fn count_v3_packed_varints(raw: &[u8]) -> crate::Result<usize> {
    let mut offset = 0;
    let mut count = 0usize;
    while offset < raw.len() {
        let (_, consumed) = decode_v3_varint(&raw[offset..])?;
        offset = offset
            .checked_add(consumed)
            .ok_or("Datadog v3 packed numeric count overflow")?;
        count = count
            .checked_add(1)
            .ok_or("Datadog v3 packed numeric count overflow")?;
    }
    Ok(count)
}

fn decode_v3_length_delimited(raw: &[u8], offset: usize) -> crate::Result<(&[u8], usize)> {
    let (length, consumed) = decode_v3_varint(
        raw.get(offset..)
            .ok_or("truncated Datadog v3 protobuf field")?,
    )?;
    let length =
        usize::try_from(length).map_err(|_| "Datadog v3 protobuf field length overflow")?;
    let start = offset
        .checked_add(consumed)
        .ok_or("Datadog v3 protobuf field length overflow")?;
    let end = start
        .checked_add(length)
        .ok_or("Datadog v3 protobuf field length overflow")?;
    let value = raw
        .get(start..end)
        .ok_or("truncated Datadog v3 protobuf field")?;
    Ok((value, end))
}

fn skip_v3_field(raw: &[u8], offset: usize, wire_type: u64) -> crate::Result<usize> {
    match wire_type {
        0 => {
            let (_, consumed) = decode_v3_varint(
                raw.get(offset..)
                    .ok_or("truncated Datadog v3 protobuf field")?,
            )?;
            offset
                .checked_add(consumed)
                .ok_or_else(|| "Datadog v3 protobuf field length overflow".into())
        }
        1 => offset
            .checked_add(8)
            .filter(|&end| end <= raw.len())
            .ok_or_else(|| "truncated Datadog v3 protobuf field".into()),
        2 => {
            let (length, consumed) = decode_v3_varint(
                raw.get(offset..)
                    .ok_or("truncated Datadog v3 protobuf field")?,
            )?;
            let length =
                usize::try_from(length).map_err(|_| "Datadog v3 protobuf field length overflow")?;
            offset
                .checked_add(consumed)
                .and_then(|start| start.checked_add(length))
                .filter(|&end| end <= raw.len())
                .ok_or_else(|| "truncated Datadog v3 protobuf field".into())
        }
        5 => offset
            .checked_add(4)
            .filter(|&end| end <= raw.len())
            .ok_or_else(|| "truncated Datadog v3 protobuf field".into()),
        _ => Err("invalid Datadog v3 protobuf wire type".into()),
    }
}

fn decode_v3_tagsets(
    packed: &[i64],
    dictionary: &[String],
    metadata: Option<&super::ddmetric_v3_proto::Metadata>,
) -> crate::Result<Vec<Vec<String>>> {
    let mut tagsets: Vec<Vec<String>> = vec![Vec::new()];
    let mut expanded_bytes = std::mem::size_of::<Vec<String>>();
    let mut offset = 0;
    while offset < packed.len() {
        expanded_bytes = expanded_bytes
            .checked_add(std::mem::size_of::<Vec<String>>())
            .ok_or("Datadog v3 expanded tagset size overflow")?;
        if expanded_bytes > MAX_V3_EXPANDED_TAGSET_BYTES {
            return Err("Datadog v3 expanded tagsets exceed size limit".into());
        }
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
                let referenced = tagsets
                    .get(index)
                    .ok_or("invalid Datadog v3 tagset reference")?;
                let referenced_bytes = referenced.iter().try_fold(0usize, |total, tag| {
                    total
                        .checked_add(std::mem::size_of::<String>())
                        .and_then(|total| total.checked_add(tag.len()))
                        .ok_or("Datadog v3 expanded tagset size overflow")
                })?;
                expanded_bytes = expanded_bytes
                    .checked_add(referenced_bytes)
                    .ok_or("Datadog v3 expanded tagset size overflow")?;
                if expanded_bytes > MAX_V3_EXPANDED_TAGSET_BYTES {
                    return Err("Datadog v3 expanded tagsets exceed size limit".into());
                }
                tags.extend(referenced.iter().cloned());
            } else {
                let tag = dictionary
                    .get(reference as usize)
                    .ok_or("invalid Datadog v3 tag reference")?;
                expanded_bytes = expanded_bytes
                    .checked_add(std::mem::size_of::<String>())
                    .and_then(|total| total.checked_add(tag.len()))
                    .ok_or("Datadog v3 expanded tagset size overflow")?;
                if expanded_bytes > MAX_V3_EXPANDED_TAGSET_BYTES {
                    return Err("Datadog v3 expanded tagsets exceed size limit".into());
                }
                tags.push(tag.clone());
            }
        }
        tagsets.push(tags);
        offset = end;
    }
    if let Some(metadata) = metadata {
        let mut seen = HashSet::new();
        let metadata_tags = metadata
            .tags
            .iter()
            .filter(|tag| seen.insert(tag.as_str()))
            .collect::<Vec<_>>();
        for tagset in &mut tagsets {
            let mut tagset_members = tagset.iter().map(String::as_str).collect::<HashSet<_>>();
            let mut additional_tags = Vec::new();
            for &tag in &metadata_tags {
                if tagset_members.insert(tag) {
                    expanded_bytes = checked_v3_string_bytes(expanded_bytes, tag)?;
                    if expanded_bytes > MAX_V3_EXPANDED_TAGSET_BYTES {
                        return Err("Datadog v3 expanded tagsets exceed size limit".into());
                    }
                    additional_tags.push(tag.clone());
                }
            }
            tagset.extend(additional_tags);
        }
    }
    Ok(tagsets)
}

fn decode_v3_resources(
    data: &super::ddmetric_v3_proto::MetricData,
) -> crate::Result<Vec<Vec<(String, String)>>> {
    let dictionary = decode_v3_strings(&data.dict_resource_str, false)?;
    let mut resources = vec![Vec::new()];
    let mut expanded_bytes = std::mem::size_of::<Vec<(String, String)>>();
    let mut entry_offset: usize = 0;
    for &resource_len in &data.dict_resource_len {
        expanded_bytes = expanded_bytes
            .checked_add(std::mem::size_of::<Vec<(String, String)>>())
            .ok_or("Datadog v3 expanded resource size overflow")?;
        if expanded_bytes > MAX_V3_EXPANDED_RESOURCE_BYTES {
            return Err("Datadog v3 expanded resources exceed size limit".into());
        }
        let size = usize::try_from(resource_len).map_err(|_| "invalid Datadog v3 resource size")?;
        let end = entry_offset
            .checked_add(size)
            .ok_or("Datadog v3 resource length overflow")?;
        if end > data.dict_resource_type.len() || end > data.dict_resource_name.len() {
            return Err("truncated Datadog v3 resource dictionary".into());
        }
        let mut set = Vec::new();
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
            let resource_type = dictionary
                .get(type_ref as usize)
                .ok_or("invalid Datadog v3 resource reference")?;
            let resource_name = dictionary
                .get(name_ref as usize)
                .ok_or("invalid Datadog v3 resource reference")?;
            expanded_bytes = checked_v3_string_bytes(expanded_bytes, resource_type)?;
            expanded_bytes = checked_v3_string_bytes(expanded_bytes, resource_name)?;
            if expanded_bytes > MAX_V3_EXPANDED_RESOURCE_BYTES {
                return Err("Datadog v3 expanded resources exceed size limit".into());
            }
            set.push((resource_type.clone(), resource_name.clone()));
        }
        resources.push(set);
        entry_offset = end;
    }
    Ok(resources)
}

fn checked_v3_string_bytes(total: usize, value: &str) -> crate::Result<usize> {
    total
        .checked_add(std::mem::size_of::<String>())
        .and_then(|total| total.checked_add(value.len()))
        .ok_or_else(|| "Datadog v3 expanded string size overflow".into())
}

/// Builds Vector's `EventMetadata` from the received metadata. Currently this is only
/// utilized for passing through origin metadata set by the Agent.
fn get_event_metadata(metadata: Option<&Metadata>, metric_type: Option<i32>) -> EventMetadata {
    let origin = metadata.and_then(|metadata| metadata.origin.as_ref());
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

pub(crate) fn decode_ddseries_v2(
    frame: Bytes,
    api_key: &Option<Arc<str>>,
    split_metric_namespace: bool,
) -> crate::Result<Vec<Event>> {
    let payload = MetricPayload::decode(frame)?;
    decode_ddseries(payload.series, api_key, split_metric_namespace, None)
}

fn decode_ddseries(
    series: Vec<metric_payload::MetricSeries>,
    api_key: &Option<Arc<str>>,
    split_metric_namespace: bool,
    metric_types: Option<&[Option<i32>]>,
) -> crate::Result<Vec<Event>> {
    let decoded_metrics: Vec<Event> = series
        .into_iter()
        .enumerate()
        .flat_map(|(index, serie)| {
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
