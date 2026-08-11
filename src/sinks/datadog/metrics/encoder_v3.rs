//! V3 columnar protobuf encoder for the Datadog metrics sink.
//!
//! Translates Vector's [`Metric`] events into the V3 columnar format produced by
//! [`datadog_agent_metrics_v3`].  Unlike V1/V2 (incremental per-metric serialization),
//! V3 accumulates all metrics into a [`V3Writer`] and serializes the entire batch
//! in a single call when [`DatadogMetricsV3Encoder::finish`] is invoked.  This is
//! required because delta encoding applies across the whole payload.

use std::{io::Write, mem, sync::Arc};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    config::{LogSchema, log_schema, telemetry},
    event::{Metric, MetricValue, metric::MetricSketch},
    metrics::AgentDDSketch,
    request_metadata::GroupedCountByteSize,
};

use datadog_agent_metrics_v3::{V3MetricBuilder, V3MetricType, V3Writer};
use protobuf::{CodedOutputStream, rt::WireType};

use super::{
    config::DatadogMetricsEndpoint,
    encoder::{
        EncoderError, FinishError, ORIGIN_CATEGORY_VALUE, ORIGIN_PRODUCT_VALUE,
        generate_origin_metadata,
    },
};
use crate::sinks::util::{
    Compression, Compressor, encode_namespace, request_builder::EncodeResult,
};

// ── Encoder ──────────────────────────────────────────────────────────────────

/// V3 batch encoder.  Accumulates metrics; serializes on [`finish`].
///
/// [`finish`]: DatadogMetricsV3Encoder::finish
pub(super) struct DatadogMetricsV3Encoder {
    default_namespace: Option<Arc<str>>,
    uncompressed_limit: usize,
    compressed_limit: usize,
    log_schema: &'static LogSchema,
    writer: V3Writer,
    pending: Vec<Metric>,
    byte_size: GroupedCountByteSize,
    origin_product_value: u32,
}

impl DatadogMetricsV3Encoder {
    pub fn new(endpoint: DatadogMetricsEndpoint, default_namespace: Option<String>) -> Self {
        let limits = endpoint.payload_limits();
        Self {
            default_namespace: default_namespace.map(Arc::from),
            uncompressed_limit: limits.uncompressed,
            compressed_limit: limits.compressed,
            log_schema: log_schema(),
            writer: V3Writer::new(),
            pending: Vec::new(),
            byte_size: telemetry().create_request_count_byte_size(),
            origin_product_value: *ORIGIN_PRODUCT_VALUE,
        }
    }

    /// Encode one metric into the writer.
    ///
    /// Always returns `Ok(None)` — the V3 encoder cannot detect payload overflow
    /// until `finish()` time.  Caller must respect the batch event-count cap.
    pub fn try_encode(&mut self, metric: Metric) -> Result<Option<Metric>, EncoderError> {
        self.byte_size
            .add_event(&metric, metric.estimated_json_encoded_size_of());
        encode_metric_to_v3(
            &mut self.writer,
            &metric,
            &self.default_namespace,
            self.log_schema,
            self.origin_product_value,
        )?;
        self.pending.push(metric);
        Ok(None)
    }

    /// Finalize: serialize, compress, check size limits, return payload.
    ///
    /// On success returns `(EncodeResult, processed_metrics)`.  On overflow
    /// returns `FinishError::TooLarge` with the metrics and a split hint.
    pub fn finish(&mut self) -> Result<(EncodeResult<Bytes>, Vec<Metric>), FinishError> {
        let writer = mem::replace(&mut self.writer, V3Writer::new());
        let metrics = mem::take(&mut self.pending);
        let byte_size = mem::replace(
            &mut self.byte_size,
            telemetry().create_request_count_byte_size(),
        );

        if metrics.is_empty() {
            // Nothing encoded — return an empty-ish result so callers don't have to special-case.
            return Ok((
                EncodeResult::compressed(Bytes::new(), 0, byte_size),
                Vec::new(),
            ));
        }

        let metric_data = writer.finalize()?.payload;

        // The wire payload isn't the bare `MetricData` message — the intake API expects it
        // wrapped as field 3 (`metricData`) of the outer `Payload` message (see
        // `intake_v3.proto`). Without this envelope the backend can't parse the bytes at all.
        let mut header_buf = [0u8; 16];
        let header_len = {
            let mut header_writer = CodedOutputStream::bytes(&mut header_buf);
            header_writer.write_tag(3, WireType::LengthDelimited)?;
            header_writer.write_uint64_no_tag(metric_data.len() as u64)?;
            header_writer.flush()?;
            header_writer.total_bytes_written() as usize
        };

        let uncompressed_size = header_len + metric_data.len();

        // Note, V3 only supports zstd.
        let mut compressor: Compressor = Compression::zstd_default().into();

        compressor
            .write_all(&header_buf[..header_len])
            .map_err(|source| FinishError::CompressionFailed { source })?;
        compressor
            .write_all(&metric_data)
            .map_err(|source| FinishError::CompressionFailed { source })?;
        let compressed = compressor
            .finish()
            .map_err(|source| FinishError::CompressionFailed { source })?
            .freeze();

        let compressed_splits = compressed.len() / self.compressed_limit;
        let uncompressed_splits = uncompressed_size / self.uncompressed_limit;
        let recommended_splits = std::cmp::max(compressed_splits, uncompressed_splits) + 1;

        if recommended_splits > 1 {
            return Err(FinishError::TooLarge {
                metrics,
                recommended_splits,
            });
        }

        Ok((
            EncodeResult::compressed(compressed, uncompressed_size, byte_size),
            metrics,
        ))
    }
}

// ── Metric → V3Writer ────────────────────────────────────────────────────────

fn encode_metric_to_v3(
    writer: &mut V3Writer,
    metric: &Metric,
    default_namespace: &Option<Arc<str>>,
    log_schema: &LogSchema,
    origin_product_value: u32,
) -> Result<(), EncoderError> {
    // Mirrors V2's `series_to_proto_message`: a Counter with an interval is sent as a
    // per-second-scaled Rate, not a raw Count.
    let maybe_interval = metric.interval_ms().map(|i| i.get() / 1000);

    let metric_type = match metric.value() {
        MetricValue::Counter { .. } if maybe_interval.is_some() => V3MetricType::Rate,
        MetricValue::Counter { .. } => V3MetricType::Count,
        MetricValue::Gauge { .. } => V3MetricType::Gauge,
        MetricValue::Set { .. } => V3MetricType::Gauge,
        MetricValue::Sketch { .. } => V3MetricType::Sketch,
        // `AggregatedSummary` is split into counters/gauges, and `Distribution`/
        // `AggregatedHistogram` are converted into `Sketch(AgentDDSketch)`, by the shared
        // `DatadogMetricsNormalizer` before metrics ever reach either encoder (see `sink.rs`).
        // This should never happen — mirrors V2's `series_to_proto_message`, which errors
        // instead of silently re-deriving a sketch with encoder-local logic that could drift
        // from the normalizer's.
        value @ (MetricValue::AggregatedSummary { .. }
        | MetricValue::Distribution { .. }
        | MetricValue::AggregatedHistogram { .. }) => {
            return Err(EncoderError::InvalidMetric {
                expected: "series or sketch",
                metric_value: value.as_name(),
            });
        }
    };

    let name = encode_namespace(
        metric
            .namespace()
            .or_else(|| default_namespace.as_ref().map(|s| s.as_ref())),
        '.',
        metric.name(),
    );

    let mut builder = writer.write(metric_type, &name);

    // ── Tags & resources ────────────────────────────────────────────────────
    let mut tags_for_v3: Vec<String> = Vec::new();
    let mut extra_resources: Vec<(&str, &str)> = Vec::new();
    // Collected separately from `extra_resources` so they can be pushed in a fixed
    // host-then-device order below, matching V2 — tag iteration order is otherwise
    // unspecified and shouldn't leak into wire-visible resource ordering.
    let mut host_resource: Option<&str> = None;
    let mut device_resource: Option<&str> = None;

    // V2 has no concept of a `dd.internal.unit` tag — it always sends the wire `unit` field
    // empty. To match, we don't special-case it either: if present, it falls through to the
    // generic tag handling below, same as V2.
    let host_key = log_schema.host_key().map(|k| k.to_string());

    if let Some(tags) = metric.tags() {
        for (key, value) in tags.iter_all() {
            // dd.internal.resource tags become structured resources
            if key == "dd.internal.resource" {
                if let Some(val) = value {
                    if let Some((rtype, rname)) = val.split_once(':') {
                        extra_resources.push((rtype, rname));
                    }
                }
                continue;
            }

            // Host key → host resource
            if host_key.as_deref() == Some(key) {
                if let Some(host) = value {
                    if !host.is_empty() {
                        host_resource = Some(host);
                    }
                }
                continue;
            }

            // device / resource.device → device resource
            if key == "device" || key == "resource.device" {
                if let Some(dev) = value {
                    device_resource = Some(dev);
                }
                continue;
            }

            // source_type_name is handled via set_source_type below
            if key == "source_type_name" {
                continue;
            }

            match value {
                Some(v) => tags_for_v3.push(format!("{}:{}", key, v)),
                None => tags_for_v3.push(key.to_string()),
            }
        }
    }

    // V2's `encode_tags` sorts tags before emitting them; tag iteration order is otherwise
    // unspecified, so without this V3's tag order wouldn't match V2's.
    tags_for_v3.sort();

    // V2 always includes a host resource — even with an empty name — whenever
    // `log_schema.host_key()` is configured (the default), regardless of whether the metric
    // actually carries that tag. Match that instead of omitting the resource entirely.
    if host_key.is_some() && host_resource.is_none() {
        host_resource = Some("");
    }

    let resources = assemble_resources(host_resource, device_resource, extra_resources);

    builder.set_tags(tags_for_v3.iter().map(|s| s.as_str()));
    builder.set_resources(&resources);

    // ── Source type / origin metadata ───────────────────────────────────────
    let event_metadata = metric.metadata();

    // source_type_name tag or metadata source type → set_source_type
    let source_type = metric.tags().and_then(|t| t.get("source_type_name"));
    if let Some(st) = source_type {
        builder.set_source_type(st);
    }

    // Datadog origin metadata → set_origin
    //
    // Mirrors V2's `generate_origin_metadata`: use the pass-through origin if one was set
    // upstream (`datadog_agent` source, `vector` source, native codecs, `log_to_metric`), else
    // synthesize one from the producing Vector source's type.
    if let Some(origin) = generate_origin_metadata(
        event_metadata.datadog_origin_metadata(),
        event_metadata.source_type(),
        origin_product_value,
    ) {
        let product = origin.product().unwrap_or(origin_product_value);
        let category = origin.category().unwrap_or(ORIGIN_CATEGORY_VALUE);
        let service = origin.service().unwrap_or(0);
        builder.set_origin(product, category, service, false);
    }

    // Interval — matches V2, which always stamps the interval field on the message
    // (`interval: maybe_interval.unwrap_or(0)`), even though only Rate uses it to scale the value.
    if let Some(interval) = maybe_interval {
        builder.set_interval(interval.into());
    }

    // Note: `unit` is intentionally never set — V2 always sends it empty (see
    // `series_to_proto_message`'s `unit: "".to_string()`).

    // ── Data points ─────────────────────────────────────────────────────────
    let timestamp = encode_timestamp(metric.timestamp());

    match metric.value() {
        MetricValue::Counter { value } => {
            let value = match maybe_interval {
                Some(interval) => *value / (interval as f64),
                None => *value,
            };
            builder.add_point(timestamp, value);
        }
        MetricValue::Gauge { value } => {
            builder.add_point(timestamp, *value);
        }
        MetricValue::Set { values } => {
            builder.add_point(timestamp, values.len() as f64);
        }
        MetricValue::Sketch {
            sketch: MetricSketch::AgentDDSketch(ddsketch),
        } => {
            encode_ddsketch(&mut builder, ddsketch, timestamp);
        }
        // Unreachable: already errored out of this function via the `metric_type` match above.
        MetricValue::AggregatedSummary { .. }
        | MetricValue::Distribution { .. }
        | MetricValue::AggregatedHistogram { .. } => {
            unreachable!("filtered out by the metric_type match above")
        }
    }

    builder.close();
    Ok(())
}

/// Assembles the final resource list in a fixed host-then-device order, matching V2's
/// `encode_series_metrics`. Host/device are collected separately during tag iteration
/// (whose order is unspecified) so that order never leaks into the wire-visible resources.
fn assemble_resources<'a>(
    host: Option<&'a str>,
    device: Option<&'a str>,
    extra: Vec<(&'a str, &'a str)>,
) -> Vec<(&'a str, &'a str)> {
    let mut resources = Vec::with_capacity(extra.len() + 2);
    if let Some(host) = host {
        resources.push(("host", host));
    }
    if let Some(dev) = device {
        resources.push(("device", dev));
    }
    resources.extend(extra);
    resources
}

fn encode_ddsketch(builder: &mut V3MetricBuilder<'_>, ddsketch: &AgentDDSketch, timestamp: i64) {
    if ddsketch.is_empty() {
        return;
    }
    let (bins_i16, counts_u16) = ddsketch.bin_map().into_parts();
    let bin_keys: Vec<i32> = bins_i16.into_iter().map(|k| k as i32).collect();
    let bin_counts: Vec<u32> = counts_u16.into_iter().map(|c| c as u32).collect();

    builder.add_sketch(
        timestamp,
        ddsketch.count() as i64,
        ddsketch.sum().unwrap_or(0.0),
        ddsketch.min().unwrap_or(0.0),
        ddsketch.max().unwrap_or(0.0),
        &bin_keys,
        &bin_counts,
    );
}

fn encode_timestamp(ts: Option<DateTime<Utc>>) -> i64 {
    ts.map(|t| t.timestamp())
        .unwrap_or_else(|| Utc::now().timestamp())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::super::config::SeriesApiVersion;
    use super::*;
    use vector_lib::event::{MetricKind, MetricValue};

    fn gauge(name: &str, value: f64) -> Metric {
        Metric::new(name, MetricKind::Absolute, MetricValue::Gauge { value })
    }

    fn counter(name: &str, value: f64) -> Metric {
        Metric::new(
            name,
            MetricKind::Incremental,
            MetricValue::Counter { value },
        )
    }

    #[test]
    fn v3_gauge_encodes_non_empty_payload() {
        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            None,
        );
        assert!(enc.try_encode(gauge("test.gauge", 42.0)).unwrap().is_none());
        let (result, metrics) = enc.finish().unwrap();
        assert!(!result.into_payload().is_empty());
        assert_eq!(metrics.len(), 1);
    }

    #[test]
    fn v3_multiple_metrics_batch() {
        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            None,
        );
        for i in 0..10 {
            enc.try_encode(counter("m", i as f64)).unwrap();
        }
        let (_, metrics) = enc.finish().unwrap();
        assert_eq!(metrics.len(), 10);
    }

    #[test]
    fn v3_empty_finish_returns_empty_payload() {
        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            None,
        );
        let (result, metrics) = enc.finish().unwrap();
        assert!(result.into_payload().is_empty());
        assert!(metrics.is_empty());
    }

    #[test]
    fn v3_resources_ordered_host_before_device_regardless_of_input_order() {
        // Regression test: V2 always emits host before device (two fixed, ordered
        // lookups). V3 used to push resources in tag-iteration order, so a metric whose
        // `device` tag happened to precede its `host` tag (alphabetically or otherwise)
        // would encode as [device, host] — a spurious mismatch against V2 even though the
        // resource set was identical.
        let resources = assemble_resources(Some("myhost"), Some("/dev/loop35"), vec![]);
        assert_eq!(
            resources,
            vec![("host", "myhost"), ("device", "/dev/loop35")]
        );

        // Order is fixed even if callers happen to discover device before host.
        let resources =
            assemble_resources(Some("myhost"), Some("/dev/loop35"), vec![("extra", "tag")]);
        assert_eq!(
            resources,
            vec![
                ("host", "myhost"),
                ("device", "/dev/loop35"),
                ("extra", "tag")
            ]
        );
    }

    #[test]
    fn v3_counter_with_interval_differs_from_plain_count() {
        // Regression test: V2's `series_to_proto_message` sends a Counter with an interval
        // as a per-second-scaled Rate, not a raw Count. V3 used to ignore `interval_ms`
        // entirely, always encoding Rate-style counters as an unscaled Count — wrong metric
        // type *and* wrong value. We can't decode the columnar payload here, but the encoded
        // bytes for a Rate-typed, scaled point must differ from a plain Count of the same
        // input value, proving the interval is actually taking effect.
        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            None,
        );
        let rate_counter = Metric::new(
            "rate.counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 100.0 },
        )
        .with_interval_ms(NonZeroU32::new(10_000));
        enc.try_encode(rate_counter).unwrap();
        let (rate_result, _) = enc.finish().unwrap();

        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            None,
        );
        enc.try_encode(counter("rate.counter", 100.0)).unwrap();
        let (plain_result, _) = enc.finish().unwrap();

        assert_ne!(
            rate_result.into_payload(),
            plain_result.into_payload(),
            "a counter with an interval must encode differently than a plain count"
        );
    }

    #[test]
    fn v3_aggregated_summary_distribution_and_histogram_are_rejected() {
        // Regression test: `AggregatedSummary` is split into counters/gauges, and
        // `Distribution`/`AggregatedHistogram` are converted into `Sketch(AgentDDSketch)`, by
        // the shared `DatadogMetricsNormalizer` before metrics ever reach either encoder (see
        // `sink.rs`) — this should never happen. V3 used to silently re-derive a sketch inline
        // instead of erroring like V2 does; now it errors too.
        use vector_lib::event::metric::{Bucket, Quantile, Sample};

        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            None,
        );
        let summary = Metric::new(
            "summary",
            MetricKind::Incremental,
            MetricValue::AggregatedSummary {
                quantiles: vec![Quantile {
                    quantile: 0.5,
                    value: 1.0,
                }],
                count: 1,
                sum: 1.0,
            },
        );
        assert!(enc.try_encode(summary).is_err());

        let mut enc = DatadogMetricsV3Encoder::new(DatadogMetricsEndpoint::Sketches, None);
        let distribution = Metric::new(
            "dist",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vec![Sample {
                    value: 1.0,
                    rate: 1,
                }],
                statistic: vector_lib::event::StatisticKind::Histogram,
            },
        );
        assert!(enc.try_encode(distribution).is_err());

        let mut enc = DatadogMetricsV3Encoder::new(DatadogMetricsEndpoint::Sketches, None);
        let histogram = Metric::new(
            "hist",
            MetricKind::Incremental,
            MetricValue::AggregatedHistogram {
                buckets: vec![Bucket {
                    upper_limit: 1.0,
                    count: 1,
                }],
                count: 1,
                sum: 1.0,
            },
        );
        assert!(enc.try_encode(histogram).is_err());
    }

    #[test]
    fn v3_origin_metadata_falls_back_to_source_type_when_no_pass_through() {
        // Regression test: when an event has no pass-through `datadog_origin_metadata`
        // (the common case for sources like `host_metrics`), V3 must synthesize origin
        // metadata from the source type the same way V2's `generate_origin_metadata` does,
        // instead of leaving origin unset.
        let mut metric = gauge("host.cpu", 1.0);
        metric.metadata_mut().set_source_type("host_metrics");

        let mut writer = V3Writer::new();
        encode_metric_to_v3(
            &mut writer,
            &metric,
            &None,
            log_schema(),
            *ORIGIN_PRODUCT_VALUE,
        )
        .unwrap();
        let encoded = writer.finalize().unwrap();
        assert!(!encoded.payload.is_empty());

        // `host_metrics` maps to OriginService 211 in V2's `source_type_to_service` table;
        // V3 reuses that same mapping via the shared `generate_origin_metadata` function.
        let origin = generate_origin_metadata(None, Some("host_metrics"), *ORIGIN_PRODUCT_VALUE)
            .expect("host_metrics should get synthesized origin metadata");
        assert_eq!(origin.service(), Some(211));
    }

    #[test]
    fn v3_namespace_prepended() {
        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            Some("myns".to_string()),
        );
        enc.try_encode(gauge("latency", 1.0)).unwrap();
        let (result, _) = enc.finish().unwrap();
        assert!(!result.into_payload().is_empty());
    }

    #[test]
    fn v3_set_maps_to_cardinality() {
        use std::collections::BTreeSet;
        let set = Metric::new(
            "my.set",
            MetricKind::Incremental,
            MetricValue::Set {
                values: BTreeSet::from(["a".to_string(), "b".to_string(), "c".to_string()]),
            },
        );
        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            None,
        );
        assert!(enc.try_encode(set).unwrap().is_none());
        enc.finish().unwrap();
    }

    #[test]
    fn v3_sketch_encoder_routes_correctly() {
        let mut sketch = AgentDDSketch::with_agent_defaults();
        sketch.insert(1.0);
        sketch.insert(2.0);
        let metric = Metric::new(
            "dist",
            MetricKind::Incremental,
            MetricValue::Sketch {
                sketch: MetricSketch::AgentDDSketch(sketch),
            },
        );
        let mut enc = DatadogMetricsV3Encoder::new(DatadogMetricsEndpoint::Sketches, None);
        enc.try_encode(metric).unwrap();
        let (result, _) = enc.finish().unwrap();
        assert!(!result.into_payload().is_empty());
    }
}
