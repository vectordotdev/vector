use std::{collections::HashSet, sync::Arc};

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use prost::Message;

use super::decode_ddseries;
use crate::{
    common::datadog::DATADOG_ORIGIN_METRIC_TYPE_NO_INDEX,
    event::{Event, metric::TagValueSet},
    sources::datadog_agent::{
        ddmetric_proto::{
            Metadata, Origin,
            metric_payload::{self, MetricPoint, MetricSeries, Resource},
        },
        ddmetric_v3_proto::{
            Metadata as MetricMetadataV3, MetricData as MetricDataV3, Payload as MetricPayloadV3,
        },
    },
};

const MAX_V3_DECODED_EXPANDED_BYTES: usize = 16 * 1024 * 1024;
// `decode_ddseries` converts every wire tag/resource into a BTreeMap entry. Account for the
// key/value containers plus a conservative three-pointer share of the tree node allocation.
const V3_METRIC_TAG_ENTRY_BYTES: usize =
    std::mem::size_of::<(String, TagValueSet)>() + 3 * std::mem::size_of::<usize>();
// Conservatively cover a borrowed string key, hash-table control/capacity overhead, and a
// pointer in the companion list used while deduplicating metadata tags.
const V3_METADATA_TAG_DEDUP_BYTES: usize = 4 * std::mem::size_of::<usize>();
const V3_TAGSET_MEMBER_BYTES: usize = 3 * std::mem::size_of::<usize>();

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

#[allow(
    clippy::ref_option,
    reason = "Match the v2 decoder API while the shared signature remains unchanged. https://github.com/vectordotdev/vector/issues/23659"
)]
pub(crate) fn decode_ddseries_v3(
    frame: Bytes,
    api_key: &Option<Arc<str>>,
    split_metric_namespace: bool,
) -> crate::Result<Vec<Event>> {
    validate_v3_predecode_allocations(&frame)?;
    let payload = MetricPayloadV3::decode(frame)?;
    // Validate and budget the decoded payload before allocating the no-index side column. Keeping
    // that column separate preserves v2 metadata semantics while allowing `metric_data` to be
    // dropped before event expansion begins.
    let (series, metric_types) =
        payload
            .metric_data
            .map_or(Ok((Vec::new(), None)), |metric_data| -> crate::Result<_> {
                let series = decode_v3_metric_data(&metric_data, payload.metadata.as_ref())?;
                let metric_types = metric_data
                    .types
                    .iter()
                    .map(|packed_type| {
                        (packed_type & V3_FLAG_NO_INDEX != 0)
                            .then_some(DATADOG_ORIGIN_METRIC_TYPE_NO_INDEX)
                    })
                    .collect::<Vec<_>>();
                Ok((series, Some(metric_types)))
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

    fn decode_unit<'a>(
        &mut self,
        data: &MetricDataV3,
        units: &'a [String],
        packed_type: u64,
    ) -> crate::Result<&'a str> {
        if packed_type & V3_FLAG_HAS_UNIT == 0 {
            return Ok("");
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
        Ok(&units[index])
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

    #[expect(
        clippy::cast_precision_loss,
        reason = "Vector metrics store protocol integer samples as f64 values."
    )]
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
    bytes: usize,
}

#[derive(Clone, Copy)]
struct V3TagResourceSize {
    tag_bytes: usize,
    tag_count: usize,
    resource_bytes: usize,
    resource_count: usize,
}

impl V3AllocationBudget {
    fn charge(&mut self, bytes: usize) -> crate::Result<()> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or("Datadog v3 decoded allocation size overflow")?;
        if self.bytes > MAX_V3_DECODED_EXPANDED_BYTES {
            return Err("Datadog v3 decoded allocations exceed size limit".into());
        }
        Ok(())
    }

    /// Accounts for every clone and vector expansion performed for one series.
    ///
    /// Callers must invoke this before allocating points, tags, resources, or events for the
    /// series. Successful return means the shared decoded/expanded allocation limit still holds.
    fn check_before_allocating(
        &mut self,
        point_count: usize,
        name: &str,
        unit: &str,
        source_type: &str,
        tag_resources: V3TagResourceSize,
    ) -> crate::Result<()> {
        let tag_resource_bytes = tag_resources
            .tag_bytes
            .checked_add(source_type.len())
            .and_then(|total| total.checked_add(tag_resources.resource_bytes))
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
        self.charge(series_bytes)?;

        let event_tag_count = tag_resources
            .tag_count
            .checked_add(tag_resources.resource_count)
            .and_then(|total| total.checked_add(usize::from(!source_type.is_empty())))
            .ok_or("Datadog v3 expanded event size overflow")?;
        let event_tag_entry_bytes = event_tag_count
            .checked_mul(V3_METRIC_TAG_ENTRY_BYTES)
            .ok_or("Datadog v3 expanded event size overflow")?;
        let event_bytes = std::mem::size_of::<Event>()
            .checked_add(name.len())
            .and_then(|total| total.checked_add(tag_resources.tag_bytes))
            .and_then(|total| total.checked_add(tag_resources.resource_bytes))
            .and_then(|total| total.checked_add(source_type.len()))
            .and_then(|total| total.checked_add(unit.len()))
            .and_then(|total| total.checked_add(event_tag_entry_bytes))
            .ok_or("Datadog v3 expanded event size overflow")?;
        self.charge(
            event_bytes
                .checked_mul(point_count)
                .ok_or("Datadog v3 expanded event size overflow")?,
        )
    }
}

fn charge_v3_decoded_allocations(
    budget: &mut V3AllocationBudget,
    data: &MetricDataV3,
    metadata: Option<&MetricMetadataV3>,
) -> crate::Result<()> {
    if let Some(metadata) = metadata {
        for value in metadata.tags.iter().chain(&metadata.resources) {
            budget.charge(
                std::mem::size_of::<String>()
                    .checked_add(value.len())
                    .ok_or("Datadog v3 metadata allocation size overflow")?,
            )?;
        }
    }

    for bytes in [
        &data.dict_name_str,
        &data.dict_tag_str,
        &data.dict_resource_str,
        &data.dict_source_type_name,
        &data.dict_unit_str,
    ] {
        budget.charge(bytes.len())?;
    }

    for bytes in [
        v3_slice_bytes(&data.dict_tagsets),
        v3_slice_bytes(&data.dict_resource_len),
        v3_slice_bytes(&data.dict_resource_type),
        v3_slice_bytes(&data.dict_resource_name),
        v3_slice_bytes(&data.dict_origin_info),
        v3_slice_bytes(&data.types),
        v3_slice_bytes(&data.name_refs),
        v3_slice_bytes(&data.tagset_refs),
        v3_slice_bytes(&data.resources_refs),
        v3_slice_bytes(&data.intervals),
        v3_slice_bytes(&data.num_points),
        v3_slice_bytes(&data.source_type_name_refs),
        v3_slice_bytes(&data.origin_info_refs),
        v3_slice_bytes(&data.unit_refs),
        v3_slice_bytes(&data.timestamps),
        v3_slice_bytes(&data.vals_sint64),
        v3_slice_bytes(&data.vals_float32),
        v3_slice_bytes(&data.vals_float64),
        v3_slice_bytes(&data.sketch_num_bins),
        v3_slice_bytes(&data.sketch_bin_keys),
        v3_slice_bytes(&data.sketch_bin_cnts),
    ] {
        budget.charge(bytes?)?;
    }
    budget.charge(
        data.types
            .len()
            .checked_mul(std::mem::size_of::<Option<i32>>())
            .ok_or("Datadog v3 metric type allocation size overflow")?,
    )?;
    Ok(())
}

fn v3_slice_bytes<T>(values: &[T]) -> crate::Result<usize> {
    values
        .len()
        .checked_mul(std::mem::size_of::<T>())
        .ok_or_else(|| "Datadog v3 decoded column allocation size overflow".into())
}

#[allow(
    clippy::too_many_lines,
    reason = "Keep the cursor advances and allocation-budget checks together so their ordering remains auditable."
)]
pub(crate) fn decode_v3_metric_data(
    data: &MetricDataV3,
    metadata: Option<&MetricMetadataV3>,
) -> crate::Result<Vec<MetricSeries>> {
    validate_v3_metric_columns(data, metadata)?;
    let mut allocation_budget = V3AllocationBudget::default();
    charge_v3_decoded_allocations(&mut allocation_budget, data, metadata)?;
    let names = decode_v3_strings(&data.dict_name_str, false, &mut allocation_budget)?;
    let tag_strings = decode_v3_strings(&data.dict_tag_str, true, &mut allocation_budget)?;
    let units = decode_v3_strings(&data.dict_unit_str, false, &mut allocation_budget)?;
    let tagsets = decode_v3_tagsets(
        &data.dict_tagsets,
        &tag_strings,
        metadata,
        &mut allocation_budget,
    )?;
    let resources = decode_v3_resources(data, &mut allocation_budget)?;
    let source_types =
        decode_v3_strings(&data.dict_source_type_name, false, &mut allocation_budget)?;
    let origin_count = data.dict_origin_info.len() / 3 + 1;

    // The size of a dictionary entry is stable, so calculate it once rather than rescanning the
    // same tags and resources for every series that references it. Charge both bookkeeping
    // vectors before collecting either one so rejection cannot temporarily exceed the budget.
    allocation_budget.charge(
        tagsets
            .len()
            .checked_mul(std::mem::size_of::<usize>())
            .ok_or("Datadog v3 bookkeeping allocation size overflow")?,
    )?;
    allocation_budget.charge(
        tagsets
            .len()
            .checked_mul(std::mem::size_of::<usize>())
            .ok_or("Datadog v3 bookkeeping allocation size overflow")?,
    )?;
    let tagset_bytes = tagsets
        .iter()
        .map(|tags| v3_string_list_bytes(tags))
        .collect::<crate::Result<Vec<_>>>()?;
    let tagset_counts = tagsets.iter().map(Vec::len).collect::<Vec<_>>();
    let metadata_resource_bytes = metadata
        .map(|metadata| v3_string_list_bytes(&metadata.resources))
        .transpose()?
        .unwrap_or(0);
    // As above, reserve the full bookkeeping cost before either resource vector is allocated.
    allocation_budget.charge(
        resources
            .len()
            .checked_mul(std::mem::size_of::<usize>())
            .ok_or("Datadog v3 bookkeeping allocation size overflow")?,
    )?;
    allocation_budget.charge(
        resources
            .len()
            .checked_mul(std::mem::size_of::<usize>())
            .ok_or("Datadog v3 bookkeeping allocation size overflow")?,
    )?;
    let resource_bytes = resources
        .iter()
        .map(|entry| {
            v3_resource_list_bytes(entry)?
                .checked_add(metadata_resource_bytes)
                .ok_or_else(|| "Datadog v3 expanded resource size overflow".into())
        })
        .collect::<crate::Result<Vec<_>>>()?;
    let metadata_resource_count = metadata.map_or(0, |metadata| metadata.resources.len() / 2);
    let resource_counts = resources
        .iter()
        .map(|entry| entry.len() + metadata_resource_count)
        .collect::<Vec<_>>();

    let dictionary_lengths = [
        names.len(),
        tagsets.len(),
        resources.len(),
        source_types.len(),
        origin_count,
    ];
    let mut references = V3ReferenceDecoder::default();
    let mut point_decoder = V3PointDecoder::new(data);
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
            unit,
            source_type,
            V3TagResourceSize {
                tag_bytes: tagset_bytes[refs.tagset],
                tag_count: tagset_counts[refs.tagset],
                resource_bytes: resource_bytes[refs.resources],
                resource_count: resource_counts[refs.resources],
            },
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
                origin: Some(Origin {
                    metric_type: 0,
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
            unit: unit.to_owned(),
            source_type_name: source_type.clone(),
            interval: i64::from(interval),
            metadata: series_metadata,
        });
    }
    Ok(series)
}

fn decode_v3_strings(
    raw: &[u8],
    sanitize: bool,
    allocation_budget: &mut V3AllocationBudget,
) -> crate::Result<Vec<String>> {
    allocation_budget.charge(std::mem::size_of::<String>())?;
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
        let valid_string_bytes = std::mem::size_of::<String>()
            .checked_add(length)
            .ok_or("Datadog v3 expanded string dictionary size overflow")?;
        let value = match std::str::from_utf8(bytes) {
            Ok(value) => {
                allocation_budget.charge(valid_string_bytes)?;
                value.to_owned()
            }
            Err(_) if sanitize => {
                let worst_case_length = length
                    .checked_mul(3)
                    .ok_or("Datadog v3 expanded string dictionary size overflow")?;
                let worst_case_bytes = std::mem::size_of::<String>()
                    .checked_add(worst_case_length)
                    .ok_or("Datadog v3 expanded string dictionary size overflow")?;
                allocation_budget.charge(worst_case_bytes)?;
                String::from_utf8_lossy(bytes).into_owned()
            }
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

fn validate_v3_predecode_allocations(raw: &[u8]) -> crate::Result<()> {
    let mut offset = 0;
    let mut allocation_budget = V3AllocationBudget::default();
    while offset < raw.len() {
        let (key, consumed) = decode_v3_varint(&raw[offset..])?;
        offset += consumed;
        let field = key >> 3;
        let wire_type = key & 0x07;
        if matches!(field, 2 | 3) && wire_type == 2 {
            let (message, end) = decode_v3_length_delimited(raw, offset)?;
            if field == 2 {
                validate_v3_metadata_allocations(message, &mut allocation_budget)?;
            } else {
                validate_v3_metric_data_allocations(message, &mut allocation_budget)?;
            }
            offset = end;
        } else {
            offset = skip_v3_field(raw, offset, wire_type)?;
        }
    }
    Ok(())
}

fn validate_v3_metadata_allocations(
    raw: &[u8],
    allocation_budget: &mut V3AllocationBudget,
) -> crate::Result<()> {
    let mut offset = 0;
    while offset < raw.len() {
        let (key, consumed) = decode_v3_varint(&raw[offset..])?;
        offset += consumed;
        let field = key >> 3;
        let wire_type = key & 0x07;
        if matches!(field, 1 | 2) && wire_type == 2 {
            let (value, end) = decode_v3_length_delimited(raw, offset)?;
            let decoded_bytes = std::mem::size_of::<String>()
                .checked_add(value.len())
                .ok_or("Datadog v3 metadata string allocation overflow")?;
            allocation_budget.charge(decoded_bytes)?;
            offset = end;
        } else {
            offset = skip_v3_field(raw, offset, wire_type)?;
        }
    }
    Ok(())
}

fn validate_v3_metric_data_allocations(
    raw: &[u8],
    allocation_budget: &mut V3AllocationBudget,
) -> crate::Result<()> {
    let mut offset = 0;
    while offset < raw.len() {
        let (key, consumed) = decode_v3_varint(&raw[offset..])?;
        offset += consumed;
        let field = key >> 3;
        let wire_type = key & 0x07;
        if matches!(field, 1 | 2 | 4 | 8 | 25) && wire_type == 2 {
            let (bytes, end) = decode_v3_length_delimited(raw, offset)?;
            allocation_budget.charge(bytes.len())?;
            offset = end;
            continue;
        }
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
        allocation_budget.charge(allocation)?;
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
    metadata: Option<&MetricMetadataV3>,
    allocation_budget: &mut V3AllocationBudget,
) -> crate::Result<Vec<Vec<String>>> {
    let mut tagsets: Vec<Vec<String>> = vec![Vec::new()];
    allocation_budget.charge(std::mem::size_of::<Vec<String>>())?;
    let mut offset = 0;
    while offset < packed.len() {
        allocation_budget.charge(std::mem::size_of::<Vec<String>>())?;
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
                allocation_budget.charge(referenced_bytes)?;
                tags.extend(referenced.iter().cloned());
            } else {
                let index =
                    usize::try_from(reference).map_err(|_| "invalid Datadog v3 tag reference")?;
                let tag = dictionary
                    .get(index)
                    .ok_or("invalid Datadog v3 tag reference")?;
                let tag_bytes = std::mem::size_of::<String>()
                    .checked_add(tag.len())
                    .ok_or("Datadog v3 expanded tagset size overflow")?;
                allocation_budget.charge(tag_bytes)?;
                tags.push(tag.clone());
            }
        }
        tagsets.push(tags);
        offset = end;
    }
    if let Some(metadata) = metadata {
        let mut seen = HashSet::new();
        let mut metadata_tags = Vec::new();
        for tag in &metadata.tags {
            if !seen.contains(tag.as_str()) {
                allocation_budget.charge(V3_METADATA_TAG_DEDUP_BYTES)?;
                seen.insert(tag.as_str());
                metadata_tags.push(tag);
            }
        }
        for tagset in &mut tagsets {
            allocation_budget.charge(
                tagset
                    .len()
                    .checked_mul(V3_TAGSET_MEMBER_BYTES)
                    .ok_or("Datadog v3 tagset deduplication size overflow")?,
            )?;
            let mut tagset_members = tagset.iter().map(String::as_str).collect::<HashSet<_>>();
            let mut additional_tags = Vec::new();
            for &tag in &metadata_tags {
                if !tagset_members.contains(tag.as_str()) {
                    allocation_budget.charge(V3_TAGSET_MEMBER_BYTES)?;
                    allocation_budget.charge(checked_v3_string_bytes(0, tag)?)?;
                    tagset_members.insert(tag.as_str());
                    additional_tags.push(tag.clone());
                }
            }
            tagset.extend(additional_tags);
        }
    }
    Ok(tagsets)
}

fn decode_v3_resources(
    data: &MetricDataV3,
    allocation_budget: &mut V3AllocationBudget,
) -> crate::Result<Vec<Vec<(String, String)>>> {
    let dictionary = decode_v3_strings(&data.dict_resource_str, false, allocation_budget)?;
    let mut resources = vec![Vec::new()];
    allocation_budget.charge(std::mem::size_of::<Vec<(String, String)>>())?;
    let mut entry_offset: usize = 0;
    for &resource_len in &data.dict_resource_len {
        allocation_budget.charge(std::mem::size_of::<Vec<(String, String)>>())?;
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
            let type_index =
                usize::try_from(type_ref).map_err(|_| "invalid Datadog v3 resource reference")?;
            let name_index =
                usize::try_from(name_ref).map_err(|_| "invalid Datadog v3 resource reference")?;
            let resource_type = dictionary
                .get(type_index)
                .ok_or("invalid Datadog v3 resource reference")?;
            let resource_name = dictionary
                .get(name_index)
                .ok_or("invalid Datadog v3 resource reference")?;
            allocation_budget.charge(checked_v3_string_bytes(0, resource_type)?)?;
            allocation_budget.charge(checked_v3_string_bytes(0, resource_name)?)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_v3_unit_reference_borrows_dictionary_entry() {
        let data = MetricDataV3 {
            unit_refs: vec![1],
            ..Default::default()
        };
        let units = vec![String::new(), "byte".to_owned()];
        let mut references = V3ReferenceDecoder::default();

        let unit = references
            .decode_unit(&data, &units, V3_FLAG_HAS_UNIT)
            .unwrap();

        assert!(std::ptr::eq(unit.as_ptr(), units[1].as_ptr()));
    }
}
