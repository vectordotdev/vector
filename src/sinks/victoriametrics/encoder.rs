//! Encodes metrics as a zstd-compressed Prometheus `WriteRequest` protobuf.
//!
//! The protobuf is written directly instead of building `prost` messages first. Each emitted
//! sample becomes its own `TimeSeries`, so no map is needed to group samples by label set, and
//! labels are written from borrowed strings. Label sets are emitted sorted by name: `MetricTags`
//! iterates in key order, and the handful of labels added by the sink (`__name__`, `le`,
//! `quantile`, `vmrange`, and the tenant labels) are merged into that order.
//!
//! The encoded request is compressed with a single zstd call, as `vmagent` does, so the frame
//! records its content size and VictoriaMetrics can decompress it into a buffer of the right size.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    io,
};

use bytes::{BufMut, BytesMut};
use chrono::Utc;
use prost::encoding::{WireType, encode_key, encode_varint, encoded_len_varint};
use vector_lib::event::{
    MetricValue,
    metric::{MetricSketch, MetricTags},
};

use super::{Tenant, sink::VmMetric};
use crate::sinks::{prelude::*, util::encode_namespace};

const METRIC_NAME_LABEL: &str = "__name__";
const ACCOUNT_ID_LABEL: &str = "vm_account_id";
const PROJECT_ID_LABEL: &str = "vm_project_id";

/// Quantiles reported for sketch metrics, as VictoriaMetrics reports for Datadog sketches.
const SKETCH_QUANTILES: [f64; 5] = [0.5, 0.75, 0.9, 0.95, 0.99];

/// Reused encode buffers larger than this are released after use.
const MAX_RETAINED_BUFFER_BYTES: usize = 16 * 1024 * 1024;

// Field numbers from `prometheus-remote.proto` and `prometheus-types.proto`.
const WRITE_REQUEST_TIMESERIES: u32 = 1;
const WRITE_REQUEST_METADATA: u32 = 3;
const TIMESERIES_LABELS: u32 = 1;
const TIMESERIES_SAMPLES: u32 = 2;
const LABEL_NAME: u32 = 1;
const LABEL_VALUE: u32 = 2;
const SAMPLE_VALUE: u32 = 1;
const SAMPLE_TIMESTAMP: u32 = 2;
const METADATA_TYPE: u32 = 1;
const METADATA_FAMILY_NAME: u32 = 2;
// VictoriaMetrics extensions of `MetricMetadata`, used by `vminsert` to choose the tenant of
// metadata sent to the multitenant endpoint.
const METADATA_ACCOUNT_ID: u32 = 11;
const METADATA_PROJECT_ID: u32 = 12;

/// `MetricMetadata.MetricType` values.
#[derive(Clone, Copy)]
enum MetricType {
    Counter = 1,
    Gauge = 2,
    Histogram = 3,
    Summary = 5,
}

impl MetricType {
    const fn of(value: &MetricValue) -> Self {
        match value {
            MetricValue::Counter { .. } => Self::Counter,
            MetricValue::Gauge { .. } | MetricValue::Set { .. } => Self::Gauge,
            MetricValue::Distribution { .. } | MetricValue::AggregatedHistogram { .. } => {
                Self::Histogram
            }
            MetricValue::AggregatedSummary { .. } | MetricValue::Sketch { .. } => Self::Summary,
        }
    }
}

thread_local! {
    static ENCODE_BUFFER: RefCell<BytesMut> = RefCell::new(BytesMut::new());
    static COMPRESSOR: RefCell<Option<(i32, zstd::bulk::Compressor<'static>)>> =
        const { RefCell::new(None) };
}

/// Compresses `data` into a single zstd frame that records its content size.
pub(super) fn compress(level: i32, data: &[u8]) -> io::Result<Vec<u8>> {
    COMPRESSOR.with(|cell| {
        let mut cached = cell.borrow_mut();
        if !matches!(&*cached, Some((cached_level, _)) if *cached_level == level) {
            *cached = Some((level, zstd::bulk::Compressor::new(level)?));
        }
        let (_, compressor) = cached.as_mut().expect("compressor was just initialized");
        compressor.compress(data)
    })
}

#[derive(Clone, Debug)]
pub(super) struct VmEncoder {
    pub(super) default_namespace: Option<String>,
    pub(super) send_metadata: bool,
    pub(super) compression_level: i32,
}

impl VmEncoder {
    /// Appends the uncompressed `WriteRequest` for `input` to `buf`.
    fn encode_uncompressed(
        &self,
        input: &[VmMetric],
        buf: &mut BytesMut,
        byte_size: &mut GroupedCountByteSize,
    ) {
        let mut output = SeriesWriter::new(buf, Utc::now().timestamp_millis());
        // Metric families whose metadata was written, per tenant.
        let mut metadata: Option<HashMap<Option<(u32, u32)>, HashSet<String>>> =
            self.send_metadata.then(HashMap::new);

        for item in input {
            byte_size.add_event(&item.metric, item.metric.estimated_json_encoded_size_of());

            let name = encode_namespace(
                item.metric
                    .namespace()
                    .or(self.default_namespace.as_deref()),
                '_',
                item.metric.name(),
            );
            if let Some(metadata) = metadata.as_mut() {
                let tenant = item.labels_tenant.as_ref();
                let families = metadata
                    .entry(tenant.map(|t| (t.account_id(), t.project_id())))
                    .or_default();
                if !families.contains(name.as_str()) {
                    output.write_metadata(&name, MetricType::of(item.metric.value()), tenant);
                    families.insert(name.clone());
                }
            }
            encode_metric(&mut output, item, &name);
        }
    }
}

impl encoding::Encoder<Vec<VmMetric>> for VmEncoder {
    fn encode_input(
        &self,
        input: Vec<VmMetric>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();
        let body = ENCODE_BUFFER.with(|cell| {
            let mut buf = cell.borrow_mut();
            buf.clear();
            self.encode_uncompressed(&input, &mut buf, &mut byte_size);
            let body = compress(self.compression_level, &buf);
            if buf.capacity() > MAX_RETAINED_BUFFER_BYTES {
                *buf = BytesMut::new();
            }
            body
        })?;
        writer.write_all(&body)?;
        Ok((body.len(), byte_size))
    }
}

/// Estimates the encoded size of the series written for `item`, used to bound request size.
pub(super) fn estimated_encoded_len(item: &VmMetric) -> usize {
    // Per series: `TimeSeries` framing (3), the `__name__` label with a suffix (21 plus the name),
    // one sink-added label such as `vmrange` (34), and the sample (18), rounded up.
    const SERIES_OVERHEAD: usize = 96;
    // Label framing (6) around a tag name and value.
    const TAG_OVERHEAD: usize = 6;
    // Both tenant labels with 10-digit values (58), rounded up.
    const TENANT_LABELS: usize = 64;

    let metric = &item.metric;
    let tags_len = metric.tags().map_or(0, |tags| {
        tags.iter_single()
            .map(|(name, value)| name.len() + value.len() + TAG_OVERHEAD)
            .sum()
    });
    let name_len = metric.namespace().map_or(0, str::len) + metric.name().len();
    let tenant_len = if item.labels_tenant.is_some() {
        TENANT_LABELS
    } else {
        0
    };
    let series = match metric.value() {
        MetricValue::Counter { .. } | MetricValue::Gauge { .. } | MetricValue::Set { .. } => 1,
        MetricValue::Distribution { .. } => {
            item.histogram.as_ref().map_or(0, |h| h.bucket_len()) + 2
        }
        MetricValue::AggregatedHistogram { buckets, .. } => buckets.len() + 3,
        MetricValue::AggregatedSummary { quantiles, .. } => quantiles.len() + 2,
        MetricValue::Sketch { .. } => SKETCH_QUANTILES.len() + 4,
    };
    series * (tags_len + name_len + tenant_len + SERIES_OVERHEAD)
}

fn encode_metric(output: &mut SeriesWriter<'_>, item: &VmMetric, name: &str) {
    let metric = &item.metric;
    let mut series = Series {
        name,
        tags: metric.tags(),
        tenant: [None, None],
        timestamp: metric
            .timestamp()
            .map_or(output.default_timestamp, |ts| ts.timestamp_millis()),
    };
    if let Some(tenant) = &item.labels_tenant {
        // Both labels are always written, as `vmagent` does.
        series.tenant = [
            Some((ACCOUNT_ID_LABEL, tenant.account_label())),
            Some((PROJECT_ID_LABEL, tenant.project_label())),
        ];
    }

    match metric.value() {
        MetricValue::Counter { value } | MetricValue::Gauge { value } => {
            output.write(&series, "", None, *value);
        }
        MetricValue::Set { values } => {
            output.write(&series, "", None, values.len() as f64);
        }
        MetricValue::Distribution { .. } => {
            let Some(histogram) = &item.histogram else {
                return;
            };
            if histogram.count() == 0 {
                return;
            }
            for (range, count) in histogram.buckets() {
                output.write(&series, "_bucket", Some(("vmrange", range)), count as f64);
            }
            output.write(&series, "_sum", None, histogram.sum());
            output.write(&series, "_count", None, histogram.count() as f64);
        }
        MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        } => {
            // Prometheus buckets are cumulative. The `+Inf` bucket is always written from the
            // total count, so an infinite upper limit in the input is skipped.
            let mut cumulative = 0;
            for bucket in buckets {
                if bucket.upper_limit.is_infinite() {
                    continue;
                }
                cumulative += bucket.count;
                let le = bucket.upper_limit.to_string();
                output.write(&series, "_bucket", Some(("le", &le)), cumulative as f64);
            }
            output.write(&series, "_bucket", Some(("le", "+Inf")), *count as f64);
            output.write(&series, "_sum", None, *sum);
            output.write(&series, "_count", None, *count as f64);
        }
        MetricValue::AggregatedSummary {
            quantiles,
            count,
            sum,
        } => {
            for quantile in quantiles {
                let q = quantile.quantile.to_string();
                output.write(&series, "", Some(("quantile", &q)), quantile.value);
            }
            output.write(&series, "_sum", None, *sum);
            output.write(&series, "_count", None, *count as f64);
        }
        MetricValue::Sketch {
            sketch: MetricSketch::AgentDDSketch(sketch),
        } => {
            for q in SKETCH_QUANTILES {
                let label = q.to_string();
                let value = sketch.quantile(q).unwrap_or(0.0);
                output.write(&series, "", Some(("quantile", &label)), value);
            }
            output.write(&series, "_sum", None, sketch.sum().unwrap_or(0.0));
            output.write(&series, "_count", None, f64::from(sketch.count()));
            if let Some(min) = sketch.min() {
                output.write(&series, "_min", None, min);
            }
            if let Some(max) = sketch.max() {
                output.write(&series, "_max", None, max);
            }
        }
    }
}

/// The parts of a series shared by every sample emitted for one metric.
struct Series<'a> {
    name: &'a str,
    tags: Option<&'a MetricTags>,
    tenant: [Option<(&'static str, &'a str)>; 2],
    timestamp: i64,
}

/// Appends `WriteRequest` fields to the request buffer.
struct SeriesWriter<'a> {
    buf: &'a mut BytesMut,
    name: String,
    default_timestamp: i64,
}

impl<'a> SeriesWriter<'a> {
    const fn new(buf: &'a mut BytesMut, default_timestamp: i64) -> Self {
        Self {
            buf,
            name: String::new(),
            default_timestamp,
        }
    }

    /// Writes one `TimeSeries` holding a single sample.
    fn write(
        &mut self,
        series: &Series<'_>,
        suffix: &str,
        extra: Option<(&'static str, &str)>,
        value: f64,
    ) {
        self.name.clear();
        self.name.push_str(series.name);
        self.name.push_str(suffix);

        // Labels added by the sink, sorted by name. `__name__` sorts before every lowercase name.
        let mut fixed = [("", ""); 4];
        let mut fixed_len = 0;
        for label in [Some((METRIC_NAME_LABEL, self.name.as_str())), extra]
            .into_iter()
            .chain(series.tenant)
            .flatten()
        {
            fixed[fixed_len] = label;
            fixed_len += 1;
        }
        let fixed = &mut fixed[..fixed_len];
        fixed.sort_unstable_by_key(|(name, _)| *name);

        let mut labels_len = 0;
        for_each_label(fixed, series.tags, |name, value| {
            labels_len += len_delimited_len(label_len(name, value));
        });
        let sample_len = sample_len(series.timestamp);
        let series_len = labels_len + len_delimited_len(sample_len);

        let buf = &mut *self.buf;
        buf.reserve(len_delimited_len(series_len));
        encode_key(WRITE_REQUEST_TIMESERIES, WireType::LengthDelimited, buf);
        encode_varint(series_len as u64, buf);
        for_each_label(fixed, series.tags, |name, value| {
            encode_key(TIMESERIES_LABELS, WireType::LengthDelimited, buf);
            encode_varint(label_len(name, value) as u64, buf);
            encode_string(LABEL_NAME, name, buf);
            encode_string(LABEL_VALUE, value, buf);
        });
        encode_key(TIMESERIES_SAMPLES, WireType::LengthDelimited, buf);
        encode_varint(sample_len as u64, buf);
        encode_key(SAMPLE_VALUE, WireType::SixtyFourBit, buf);
        buf.put_f64_le(value);
        encode_key(SAMPLE_TIMESTAMP, WireType::Varint, buf);
        encode_varint(series.timestamp as u64, buf);
    }

    /// Writes one `MetricMetadata`. The tenant fields are omitted when zero, as in VictoriaMetrics.
    fn write_metadata(
        &mut self,
        family_name: &str,
        metric_type: MetricType,
        tenant: Option<&Tenant>,
    ) {
        let (account_id, project_id) = tenant.map_or((0, 0), |t| (t.account_id(), t.project_id()));
        let metadata_len =
            2 + string_len(family_name) + uint_field_len(account_id) + uint_field_len(project_id);

        let buf = &mut *self.buf;
        encode_key(WRITE_REQUEST_METADATA, WireType::LengthDelimited, buf);
        encode_varint(metadata_len as u64, buf);
        encode_key(METADATA_TYPE, WireType::Varint, buf);
        encode_varint(metric_type as u64, buf);
        encode_string(METADATA_FAMILY_NAME, family_name, buf);
        encode_uint(METADATA_ACCOUNT_ID, account_id, buf);
        encode_uint(METADATA_PROJECT_ID, project_id, buf);
    }
}

/// Calls `f` for each label in name order, merging the sorted `fixed` labels with `tags`. A fixed
/// label replaces a tag with the same name.
fn for_each_label<'a>(
    fixed: &[(&'a str, &'a str)],
    tags: Option<&'a MetricTags>,
    mut f: impl FnMut(&'a str, &'a str),
) {
    let mut fixed = fixed.iter().copied().peekable();
    for (name, value) in tags.into_iter().flat_map(MetricTags::iter_single) {
        let mut replaced = false;
        while let Some((fixed_name, fixed_value)) =
            fixed.next_if(|(fixed_name, _)| *fixed_name <= name)
        {
            replaced |= fixed_name == name;
            f(fixed_name, fixed_value);
        }
        if !replaced {
            f(name, value);
        }
    }
    for (name, value) in fixed {
        f(name, value);
    }
}

/// Encoded length of a field holding a nested message or bytes of length `len`, excluding the
/// key of the enclosing field.
fn len_delimited_len(len: usize) -> usize {
    1 + encoded_len_varint(len as u64) + len
}

/// Encoded length of a `Label` message body.
fn label_len(name: &str, value: &str) -> usize {
    string_len(name) + string_len(value)
}

/// Encoded length of a `Sample` message body.
fn sample_len(timestamp: i64) -> usize {
    9 + 1 + encoded_len_varint(timestamp as u64)
}

/// Encoded length of a string field. Empty strings are omitted, as in proto3.
fn string_len(value: &str) -> usize {
    if value.is_empty() {
        0
    } else {
        len_delimited_len(value.len())
    }
}

/// Encoded length of a `uint32` field. Zero is omitted, as in proto3.
fn uint_field_len(value: u32) -> usize {
    if value == 0 {
        0
    } else {
        1 + encoded_len_varint(u64::from(value))
    }
}

fn encode_uint(tag: u32, value: u32, buf: &mut BytesMut) {
    if value != 0 {
        encode_key(tag, WireType::Varint, buf);
        encode_varint(u64::from(value), buf);
    }
}

fn encode_string(tag: u32, value: &str, buf: &mut BytesMut) {
    if !value.is_empty() {
        encode_key(tag, WireType::LengthDelimited, buf);
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, io::Read};

    use bytes::Buf;
    use chrono::{TimeZone, Utc};
    use prost::Message;
    use vector_common::decompression::CappedDecoder;
    use vector_lib::{
        event::{
            Metric, MetricKind, StatisticKind,
            metric::{Bucket, Quantile, Sample},
        },
        metric_tags,
        metrics::AgentDDSketch,
        prometheus::parser::proto,
    };

    use super::{super::Tenant, *};
    use crate::sinks::{util::encoding::Encoder as _, victoriametrics::histogram::VmHistogram};

    const TIMESTAMP_MS: i64 = 1_700_000_000_123;

    fn metric(name: &str, value: MetricValue) -> Metric {
        Metric::new(name, MetricKind::Absolute, value)
            .with_tags(Some(metric_tags!("region" => "us-west-1", "Host" => "a")))
            .with_timestamp(Some(Utc.timestamp_millis_opt(TIMESTAMP_MS).unwrap()))
    }

    fn item(metric: Metric) -> VmMetric {
        VmMetric::new(metric, None, None, None)
    }

    /// Encodes `items`, checks the zstd frame, and returns the uncompressed body.
    fn encode_body(encoder: &VmEncoder, items: Vec<VmMetric>) -> Vec<u8> {
        let mut out = Vec::new();
        let (written, _) = encoder.encode_input(items, &mut out).unwrap();
        assert_eq!(written, out.len());

        let mut body = Vec::new();
        CappedDecoder::zstd(out.as_slice().reader())
            .unwrap()
            .into_reader()
            .read_to_end(&mut body)
            .unwrap();
        // A single-shot frame records its content size, like `vmagent` blocks.
        assert_eq!(
            zstd::zstd_safe::get_frame_content_size(&out).unwrap(),
            Some(body.len() as u64)
        );
        body
    }

    fn encode(encoder: &VmEncoder, items: Vec<VmMetric>) -> proto::WriteRequest {
        proto::WriteRequest::decode(encode_body(encoder, items).as_slice()).unwrap()
    }

    fn encoder() -> VmEncoder {
        VmEncoder {
            default_namespace: None,
            send_metadata: false,
            compression_level: 3,
        }
    }

    fn metadata_encoder() -> VmEncoder {
        VmEncoder {
            send_metadata: true,
            ..encoder()
        }
    }

    /// Flattens a request into `(labels, value, timestamp)` rows.
    fn rows(request: &proto::WriteRequest) -> Vec<(BTreeMap<String, String>, f64, i64)> {
        request
            .timeseries
            .iter()
            .map(|series| {
                assert_eq!(series.samples.len(), 1);
                let labels = series
                    .labels
                    .iter()
                    .map(|label| (label.name.clone(), label.value.clone()))
                    .collect();
                (labels, series.samples[0].value, series.samples[0].timestamp)
            })
            .collect()
    }

    fn label_names(series: &proto::TimeSeries) -> Vec<&str> {
        series
            .labels
            .iter()
            .map(|label| label.name.as_str())
            .collect()
    }

    #[test]
    fn encodes_gauge_with_sorted_labels() {
        let request = encode(
            &encoder(),
            vec![item(metric("cpu", MetricValue::Gauge { value: 1.5 }))],
        );

        assert_eq!(request.timeseries.len(), 1);
        assert!(request.metadata.is_empty());
        let series = &request.timeseries[0];
        // `Host` sorts before `__name__`, `region` after it.
        assert_eq!(label_names(series), vec!["Host", "__name__", "region"]);
        assert_eq!(series.labels[1].value, "cpu");
        assert_eq!(series.samples[0].value, 1.5);
        assert_eq!(series.samples[0].timestamp, TIMESTAMP_MS);
    }

    #[test]
    fn matches_prost_encoding() {
        let expected = proto::WriteRequest {
            timeseries: vec![proto::TimeSeries {
                labels: vec![
                    proto::Label {
                        name: "Host".into(),
                        value: "a".into(),
                    },
                    proto::Label {
                        name: "__name__".into(),
                        value: "requests".into(),
                    },
                    proto::Label {
                        name: "region".into(),
                        value: "us-west-1".into(),
                    },
                ],
                samples: vec![proto::Sample {
                    value: 42.0,
                    timestamp: TIMESTAMP_MS,
                }],
            }],
            metadata: vec![proto::MetricMetadata {
                r#type: proto::MetricType::Counter as i32,
                metric_family_name: "requests".into(),
                help: String::new(),
                unit: String::new(),
            }],
        };

        let request = encode(
            &metadata_encoder(),
            vec![item(metric(
                "requests",
                MetricValue::Counter { value: 42.0 },
            ))],
        );

        // Metadata is written before the series that introduces it, and prost writes all series
        // first, so compare decoded messages rather than bytes.
        assert_eq!(request, expected);
    }

    #[test]
    fn default_timestamp_and_namespace() {
        let metric = Metric::new(
            "up",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.0 },
        );
        let request = encode(
            &VmEncoder {
                default_namespace: Some("app".into()),
                ..encoder()
            },
            vec![item(metric)],
        );

        let series = &request.timeseries[0];
        assert_eq!(series.labels[0].value, "app_up");
        assert!(series.samples[0].timestamp > 0);
    }

    #[test]
    fn encodes_distribution_as_vmrange() {
        let mut histogram = VmHistogram::default();
        histogram.record(0.5, 2);
        histogram.record(20.0, 1);
        let metric = metric(
            "latency",
            MetricValue::Distribution {
                samples: vec![Sample {
                    value: 0.5,
                    rate: 1,
                }],
                statistic: StatisticKind::Histogram,
            },
        );

        let request = encode(
            &encoder(),
            vec![VmMetric::new(metric, Some(histogram), None, None)],
        );
        let rows = rows(&request);

        let summary = rows
            .iter()
            .map(|(labels, value, _)| {
                (
                    labels["__name__"].as_str(),
                    labels.get("vmrange").map(String::as_str),
                    *value,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            vec![
                ("latency_bucket", Some("4.642e-01...5.275e-01"), 2.0),
                ("latency_bucket", Some("1.896e+01...2.154e+01"), 1.0),
                ("latency_sum", None, 21.0),
                ("latency_count", None, 3.0),
            ]
        );
        assert_eq!(
            label_names(&request.timeseries[0]),
            vec!["Host", "__name__", "region", "vmrange"]
        );
    }

    #[test]
    fn skips_empty_distribution() {
        let metric = metric(
            "latency",
            MetricValue::Distribution {
                samples: vec![],
                statistic: StatisticKind::Histogram,
            },
        );
        let request = encode(
            &encoder(),
            vec![VmMetric::new(
                metric,
                Some(VmHistogram::default()),
                None,
                None,
            )],
        );
        assert!(request.timeseries.is_empty());
    }

    #[test]
    fn encodes_aggregated_histogram() {
        let metric = metric(
            "size",
            MetricValue::AggregatedHistogram {
                buckets: vec![
                    Bucket {
                        upper_limit: 1.0,
                        count: 2,
                    },
                    Bucket {
                        upper_limit: 5.0,
                        count: 3,
                    },
                    Bucket {
                        upper_limit: f64::INFINITY,
                        count: 1,
                    },
                ],
                count: 6,
                sum: 12.0,
            },
        );
        let request = encode(&encoder(), vec![item(metric)]);

        let summary = rows(&request)
            .into_iter()
            .map(|(labels, value, _)| {
                (labels["__name__"].clone(), labels.get("le").cloned(), value)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            vec![
                ("size_bucket".into(), Some("1".into()), 2.0),
                ("size_bucket".into(), Some("5".into()), 5.0),
                ("size_bucket".into(), Some("+Inf".into()), 6.0),
                ("size_sum".into(), None, 12.0),
                ("size_count".into(), None, 6.0),
            ]
        );
    }

    #[test]
    fn encodes_aggregated_summary() {
        let metric = metric(
            "rtt",
            MetricValue::AggregatedSummary {
                quantiles: vec![Quantile {
                    quantile: 0.99,
                    value: 7.0,
                }],
                count: 4,
                sum: 10.0,
            },
        );
        let request = encode(&encoder(), vec![item(metric)]);

        let summary = rows(&request)
            .into_iter()
            .map(|(labels, value, _)| {
                (
                    labels["__name__"].clone(),
                    labels.get("quantile").cloned(),
                    value,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            vec![
                ("rtt".into(), Some("0.99".into()), 7.0),
                ("rtt_sum".into(), None, 10.0),
                ("rtt_count".into(), None, 4.0),
            ]
        );
    }

    #[test]
    fn fixed_label_replaces_tag() {
        let metric = metric("cpu", MetricValue::Gauge { value: 1.0 }).with_tags(Some(
            metric_tags!("__name__" => "spoofed", "vm_account_id" => "7"),
        ));
        let tenant = Tenant::parse("42:3").unwrap();
        let request = encode(
            &encoder(),
            vec![VmMetric::new(metric, None, Some(tenant), None)],
        );

        let (labels, _, _) = &rows(&request)[0];
        assert_eq!(labels["__name__"], "cpu");
        assert_eq!(labels["vm_account_id"], "42");
        assert_eq!(labels["vm_project_id"], "3");
        assert_eq!(
            label_names(&request.timeseries[0]),
            vec!["__name__", "vm_account_id", "vm_project_id"]
        );
    }

    #[test]
    fn metadata_is_written_once_per_family() {
        let request = encode(
            &metadata_encoder(),
            vec![
                item(metric("temp", MetricValue::Gauge { value: 1.0 })),
                item(metric("temp", MetricValue::Gauge { value: 2.0 })),
            ],
        );

        assert_eq!(request.timeseries.len(), 2);
        assert_eq!(request.metadata.len(), 1);
        assert_eq!(request.metadata[0].r#type, proto::MetricType::Gauge as i32);
        assert_eq!(request.metadata[0].metric_family_name, "temp");
    }

    #[test]
    fn labels_mode_writes_both_tenant_labels() {
        let request = encode(
            &encoder(),
            vec![VmMetric::new(
                metric("cpu", MetricValue::Gauge { value: 1.0 }),
                None,
                Tenant::parse("042"),
                None,
            )],
        );

        let (labels, _, _) = &rows(&request)[0];
        assert_eq!(labels["vm_account_id"], "42");
        assert_eq!(labels["vm_project_id"], "0");
    }

    #[test]
    fn metadata_carries_labels_mode_tenant() {
        let gauge = |tenant: &str| {
            VmMetric::new(
                metric("temp", MetricValue::Gauge { value: 1.0 }),
                None,
                Tenant::parse(tenant),
                None,
            )
        };
        let body = encode_body(
            &metadata_encoder(),
            vec![gauge("7:3"), gauge("8"), gauge("7:3")],
        );

        // Metadata is written once per tenant and metric family. `prometheus-types.proto` has no
        // tenant fields, so they are read from the raw message.
        assert_eq!(raw_metadata_tenants(&body), vec![(7, 3), (8, 0)]);
        let request = proto::WriteRequest::decode(body.as_slice()).unwrap();
        assert_eq!(request.metadata.len(), 2);
        assert_eq!(request.timeseries.len(), 3);
    }

    /// Returns the `(AccountID, ProjectID)` fields of every `MetricMetadata` in a request.
    fn raw_metadata_tenants(body: &[u8]) -> Vec<(u64, u64)> {
        use prost::encoding::{decode_key, decode_varint};

        let mut tenants = Vec::new();
        let mut buf = body;
        while buf.has_remaining() {
            let (tag, _) = decode_key(&mut buf).unwrap();
            let len = decode_varint(&mut buf).unwrap() as usize;
            let (mut message, rest) = buf.split_at(len);
            buf = rest;
            if tag != WRITE_REQUEST_METADATA {
                continue;
            }
            let (mut account, mut project) = (0, 0);
            while message.has_remaining() {
                let (field, wire_type) = decode_key(&mut message).unwrap();
                match wire_type {
                    WireType::Varint => {
                        let value = decode_varint(&mut message).unwrap();
                        match field {
                            METADATA_ACCOUNT_ID => account = value,
                            METADATA_PROJECT_ID => project = value,
                            _ => {}
                        }
                    }
                    WireType::LengthDelimited => {
                        let len = decode_varint(&mut message).unwrap() as usize;
                        message.advance(len);
                    }
                    other => panic!("unexpected wire type {other:?}"),
                }
            }
            tenants.push((account, project));
        }
        tenants
    }

    #[test]
    fn encodes_sketch_like_victoriametrics() {
        let mut sketch = AgentDDSketch::with_agent_defaults();
        sketch.insert_many(&[1.0, 2.0, 3.0]);
        let request = encode(
            &encoder(),
            vec![item(metric(
                "latency",
                MetricValue::Sketch {
                    sketch: MetricSketch::AgentDDSketch(sketch),
                },
            ))],
        );

        let names = rows(&request)
            .into_iter()
            .map(|(labels, _, _)| (labels["__name__"].clone(), labels.get("quantile").cloned()))
            .collect::<Vec<_>>();
        let quantile = |q: &str| ("latency".to_owned(), Some(q.to_owned()));
        let plain = |name: &str| (name.to_owned(), None);
        assert_eq!(
            names,
            vec![
                quantile("0.5"),
                quantile("0.75"),
                quantile("0.9"),
                quantile("0.95"),
                quantile("0.99"),
                plain("latency_sum"),
                plain("latency_count"),
                plain("latency_min"),
                plain("latency_max"),
            ]
        );
    }

    #[test]
    fn estimate_bounds_encoded_size() {
        let mut histogram = VmHistogram::default();
        let mut value = 1e-9;
        while value < 1e18 {
            histogram.record(value, 1);
            value *= 1.05;
        }
        let distribution = || {
            metric(
                "latency",
                MetricValue::Distribution {
                    samples: vec![],
                    statistic: StatisticKind::Histogram,
                },
            )
        };
        let cases = vec![
            item(metric("cpu", MetricValue::Gauge { value: 1.0 })),
            VmMetric::new(distribution(), Some(histogram.clone()), None, None),
            VmMetric::new(
                distribution(),
                Some(histogram),
                Tenant::parse("4294967295:4294967295"),
                None,
            ),
        ];

        for case in cases {
            let estimate = estimated_encoded_len(&case);
            let actual = encode_body(&encoder(), vec![case]).len();
            assert!(estimate >= actual, "estimate {estimate} < actual {actual}");
            assert!(
                estimate <= actual * 3,
                "estimate {estimate} far above {actual}"
            );
        }
    }
}
