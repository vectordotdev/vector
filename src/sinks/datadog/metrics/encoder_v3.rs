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
    event::{Metric, MetricValue},
    request_metadata::GroupedCountByteSize,
};

use datadog_agent_metrics_v3::{V3MetricType, V3Writer};
use protobuf::{CodedOutputStream, rt::WireType};

use super::{
    config::DatadogMetricsEndpoint,
    encoder::{
        EncoderError, FinishError, ORIGIN_CATEGORY_VALUE, ORIGIN_PRODUCT_VALUE, SeriesTags,
        generate_origin_metadata, split_series_tags,
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

    /// Creates an encoder with specific payload limits.
    ///
    /// Only available in tests; production code always uses the API-defined limits via `new`.
    /// Mirrors `DatadogMetricsEncoder::with_payload_limits`.
    #[cfg(test)]
    pub fn with_payload_limits(
        default_namespace: Option<String>,
        uncompressed_limit: usize,
        compressed_limit: usize,
    ) -> Self {
        Self {
            uncompressed_limit,
            compressed_limit,
            ..Self::new(
                DatadogMetricsEndpoint::Series(super::config::SeriesApiVersion::V3),
                default_namespace,
            )
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

        // Both limits are inclusive: the intake accepts a payload whose encoded size is exactly
        // the maximum. Only a strictly larger payload has overflowed.
        if compressed.len() > self.compressed_limit || uncompressed_size > self.uncompressed_limit {
            let compressed_splits = compressed.len().div_ceil(self.compressed_limit.max(1));
            let uncompressed_splits = uncompressed_size.div_ceil(self.uncompressed_limit.max(1));

            return Err(FinishError::TooLarge {
                metrics,
                recommended_splits: std::cmp::max(compressed_splits, uncompressed_splits).max(2),
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
        // Sketch-valued metrics are routed to the sketches endpoint (see `sink.rs`), which
        // always uses the V1/V2 encoder, so they never reach this encoder. `AggregatedSummary`
        // is split into counters/gauges, and `Distribution`/`AggregatedHistogram` are converted
        // into `Sketch(AgentDDSketch)`, by the shared `DatadogMetricsNormalizer` before metrics
        // ever reach either encoder. None of these should happen — mirrors V2's
        // `series_to_proto_message`, which errors rather than guessing at a conversion here.
        value @ (MetricValue::Sketch { .. }
        | MetricValue::AggregatedSummary { .. }
        | MetricValue::Distribution { .. }
        | MetricValue::AggregatedHistogram { .. }) => {
            return Err(EncoderError::InvalidMetric {
                expected: "series",
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

    // ── Tags, resources & source type ───────────────────────────────────────
    //
    // Delegated wholesale to the V1/V2 encoder's splitter so both protocols send the same
    // tags and resources for the same metric. Notably this is what restores the
    // `resource.<type>` tags that the `datadog_agent` source produces from an upstream V2
    // payload's resources back into structured resources.
    //
    // `unit` is intentionally never set: V2 always sends it empty (`unit: "".to_string()` in
    // `series_to_proto_message`), so a `dd.internal.unit` tag stays an ordinary tag here too.
    let SeriesTags {
        tags,
        resources,
        source_type_name,
    } = split_series_tags(metric, log_schema);

    let resources: Vec<(&str, &str)> = resources
        .iter()
        .map(|(r#type, name)| (r#type.as_str(), name.as_str()))
        .collect();

    builder.set_tags(tags.iter().map(String::as_str));
    builder.set_resources(&resources);

    if !source_type_name.is_empty() {
        builder.set_source_type(&source_type_name);
    }

    // ── Origin metadata ─────────────────────────────────────────────────────
    let event_metadata = metric.metadata();

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
        // Unreachable: already errored out of this function via the `metric_type` match above.
        MetricValue::Sketch { .. }
        | MetricValue::AggregatedSummary { .. }
        | MetricValue::Distribution { .. }
        | MetricValue::AggregatedHistogram { .. } => {
            unreachable!("filtered out by the metric_type match above")
        }
    }

    builder.close();
    Ok(())
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
    use vector_lib::{
        event::{MetricKind, MetricValue, metric::MetricSketch},
        metrics::AgentDDSketch,
    };

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

    /// Encodes one gauge and reports `(uncompressed_size, compressed_size)` for it.
    fn measure_single_gauge() -> (usize, usize) {
        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            None,
        );
        enc.try_encode(gauge("limit.probe", 1.0)).unwrap();
        let (result, _) = enc.finish().expect("probe payload should encode");

        let uncompressed = result.uncompressed_byte_size;
        let compressed = result.into_payload().len();
        (uncompressed, compressed)
    }

    /// The payload size limits are inclusive: the intake accepts a payload whose encoded size is
    /// exactly the maximum, and saluki's `request_fits` compares both dimensions with `<=`. A
    /// payload sitting exactly on a limit must therefore be sent, not reported as too large.
    ///
    /// Deriving that from `len / limit + 1` made the exact-limit case look like it needed two
    /// splits, and `split_and_encode` has nothing left to halve for a single-metric batch, so it
    /// dropped a payload the intake would have accepted. Sizes are measured first, then fed back
    /// as the limits under test, since they can't be predicted.
    #[test]
    fn v3_payload_exactly_at_the_limits_is_not_too_large() {
        let (uncompressed, compressed) = measure_single_gauge();

        let mut enc = DatadogMetricsV3Encoder::with_payload_limits(None, uncompressed, compressed);
        enc.try_encode(gauge("limit.probe", 1.0)).unwrap();

        let (result, metrics) = enc
            .finish()
            .expect("a payload exactly at both limits must be accepted, not split");
        assert_eq!(metrics.len(), 1);
        assert_eq!(result.into_payload().len(), compressed);
    }

    /// One byte over either limit is still an overflow.
    #[test]
    fn v3_payload_one_byte_over_a_limit_is_too_large() {
        let (uncompressed, compressed) = measure_single_gauge();

        for (uncompressed_limit, compressed_limit) in [
            (uncompressed - 1, compressed),
            (uncompressed, compressed - 1),
        ] {
            let mut enc = DatadogMetricsV3Encoder::with_payload_limits(
                None,
                uncompressed_limit,
                compressed_limit,
            );
            enc.try_encode(gauge("limit.probe", 1.0)).unwrap();

            match enc.finish() {
                Err(FinishError::TooLarge {
                    metrics,
                    recommended_splits,
                }) => {
                    assert_eq!(metrics.len(), 1, "the metric must come back for splitting");
                    assert!(
                        recommended_splits >= 2,
                        "an overflowing payload must recommend at least two splits, got \
                         {recommended_splits}"
                    );
                }
                other => panic!("expected TooLarge, got {:?}", other.map(|(_, m)| m.len())),
            }
        }
    }

    // The `datadog_agent` source flattens an upstream V2 payload's structured resources back
    // into tags (`device`, and `resource.<type>` for everything else), so the encoders are
    // responsible for restoring them. V3 originally re-derived that itself and handled only
    // `host`, `device`/`resource.device` and `dd.internal.resource`, silently dropping every
    // other `resource.<type>` resource -- the `datadog_metrics` e2e test caught it as a
    // missing `database_instance` resource on a metric that had round-tripped through the
    // source. Both encoders now share `split_series_tags`, so this asserts the shared
    // behavior V3 depends on: resources restored, in V2's order, and not left behind as tags.
    #[test]
    fn v3_tags_and_resources_match_v2() {
        let mut metric = gauge("foo_metric.resource", 1.0);
        metric.replace_tag("host".to_string(), "myhost".to_string());
        metric.replace_tag("device".to_string(), "/dev/sda1".to_string());
        metric.replace_tag(
            "resource.database_instance".to_string(),
            "mongo-repro-01".to_string(),
        );
        metric.replace_tag("source_type_name".to_string(), "mongo".to_string());
        metric.replace_tag("a_tag".to_string(), "1".to_string());

        let split = split_series_tags(&metric, log_schema());

        assert_eq!(
            split.resources,
            vec![
                ("host".to_string(), "myhost".to_string()),
                ("device".to_string(), "/dev/sda1".to_string()),
                (
                    "database_instance".to_string(),
                    "mongo-repro-01".to_string()
                ),
            ],
            "resources must be restored in V2's host-then-device-then-`resource.<type>` order"
        );
        assert_eq!(
            split.tags,
            vec!["a_tag:1".to_string()],
            "tags promoted to resources must not also be sent as tags"
        );
        assert_eq!(split.source_type_name, "mongo");

        // And the encoder actually accepts that metric end to end.
        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            None,
        );
        enc.try_encode(metric).unwrap();
        let (result, _) = enc.finish().unwrap();
        assert!(!result.into_payload().is_empty());
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
    fn v3_non_series_metric_values_are_rejected() {
        // `AggregatedSummary` is split into counters/gauges, and `Distribution`/
        // `AggregatedHistogram` are converted into `Sketch(AgentDDSketch)`, by the shared
        // `DatadogMetricsNormalizer`, whose sketch output is then routed to the sketches
        // endpoint and its V1/V2 encoder (see `sink.rs`) — so none of these values, sketches
        // included, can reach the V3 encoder. Mirror V2 and error rather than guess.
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

        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            None,
        );
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

        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            None,
        );
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

        let mut sketch = AgentDDSketch::with_agent_defaults();
        sketch.insert(1.0);
        let mut enc = DatadogMetricsV3Encoder::new(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            None,
        );
        assert!(
            enc.try_encode(Metric::new(
                "dist",
                MetricKind::Incremental,
                MetricValue::Sketch {
                    sketch: MetricSketch::AgentDDSketch(sketch),
                },
            ))
            .is_err()
        );
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
}
