use std::{
    cmp,
    io::{self, Write},
    mem,
    sync::{Arc, OnceLock},
};

use bytes::{BufMut, Bytes};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use snafu::{ResultExt, Snafu};
use vector_lib::request_metadata::GroupedCountByteSize;
use vector_lib::{
    config::{log_schema, telemetry, LogSchema},
    event::{metric::MetricSketch, DatadogMetricOriginMetadata, Metric, MetricTags, MetricValue},
    metrics::AgentDDSketch,
    EstimatedJsonEncodedSizeOf,
};

use super::config::{DatadogMetricsEndpoint, SeriesApiVersion};
use crate::{
    common::datadog::{
        DatadogMetricType, DatadogPoint, DatadogSeriesMetric, DatadogSeriesMetricMetadata,
    },
    proto::fds::protobuf_descriptors,
    sinks::util::{encode_namespace, request_builder::EncodeResult, Compression, Compressor},
};

const SERIES_PAYLOAD_HEADER: &[u8] = b"{\"series\":[";
const SERIES_PAYLOAD_FOOTER: &[u8] = b"]}";
const SERIES_PAYLOAD_DELIMITER: &[u8] = b",";

pub(super) const ORIGIN_CATEGORY_VALUE: u32 = 11;

const DEFAULT_DD_ORIGIN_PRODUCT_VALUE: u32 = 14;

pub(super) static ORIGIN_PRODUCT_VALUE: Lazy<u32> = Lazy::new(|| {
    option_env!("DD_ORIGIN_PRODUCT")
        .map(|p| {
            p.parse::<u32>()
                .expect("Env var DD_ORIGIN_PRODUCT must be an unsigned 32 bit integer.")
        })
        .unwrap_or(DEFAULT_DD_ORIGIN_PRODUCT_VALUE)
});

#[allow(warnings, clippy::pedantic, clippy::nursery)]
mod ddmetric_proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

#[derive(Debug, Snafu)]
pub enum CreateError {
    #[snafu(display("Invalid compressed/uncompressed payload size limits were given"))]
    InvalidLimits,
}

impl CreateError {
    /// Gets the telemetry-friendly string version of this error.
    ///
    /// The value will be a short string with only lowercase letters and underscores.
    pub const fn as_error_type(&self) -> &'static str {
        match self {
            Self::InvalidLimits => "invalid_payload_limits",
        }
    }
}

#[derive(Debug, Snafu)]
pub enum EncoderError {
    #[snafu(display(
        "Invalid metric value '{}' was given; '{}' expected",
        metric_value,
        expected
    ))]
    InvalidMetric {
        expected: &'static str,
        metric_value: &'static str,
    },

    #[snafu(
        context(false),
        display("Failed to encode series metric to JSON: {source}")
    )]
    JsonEncodingFailed { source: serde_json::Error },

    // Currently, the only time `prost` ever emits `EncodeError` is when there is insufficient
    // buffer capacity, so we don't need to hold on to the error, and we can just hardcode this.
    #[snafu(display(
        "Failed to encode sketch metric to Protocol Buffers: insufficient buffer capacity."
    ))]
    ProtoEncodingFailed,
}

impl EncoderError {
    /// Gets the telemetry-friendly string version of this error.
    ///
    /// The value will be a short string with only lowercase letters and underscores.
    pub const fn as_error_type(&self) -> &'static str {
        match self {
            Self::InvalidMetric { .. } => "invalid_metric",
            Self::JsonEncodingFailed { .. } => "failed_to_encode_series",
            Self::ProtoEncodingFailed => "failed_to_encode_sketch",
        }
    }
}

#[derive(Debug, Snafu)]
pub enum FinishError {
    #[snafu(display(
        "Failure occurred during writing to or finalizing the compressor: {}",
        source
    ))]
    CompressionFailed { source: io::Error },

    #[snafu(display("Finished payload exceeded the (un)compressed size limits"))]
    TooLarge {
        metrics: Vec<Metric>,
        recommended_splits: usize,
    },
}

impl FinishError {
    /// Gets the telemetry-friendly string version of this error.
    ///
    /// The value will be a short string with only lowercase letters and underscores.
    pub const fn as_error_type(&self) -> &'static str {
        match self {
            Self::CompressionFailed { .. } => "compression_failed",
            Self::TooLarge { .. } => "too_large",
        }
    }
}

struct EncoderState {
    writer: Compressor,
    written: usize,
    buf: Vec<u8>,
    processed: Vec<Metric>,
    byte_size: GroupedCountByteSize,
}

impl Default for EncoderState {
    fn default() -> Self {
        Self {
            writer: get_compressor(),
            written: 0,
            buf: Vec::with_capacity(1024),
            processed: Vec::new(),
            byte_size: telemetry().create_request_count_byte_size(),
        }
    }
}

pub struct DatadogMetricsEncoder {
    endpoint: DatadogMetricsEndpoint,
    default_namespace: Option<Arc<str>>,
    uncompressed_limit: usize,
    compressed_limit: usize,

    state: EncoderState,
    log_schema: &'static LogSchema,

    origin_product_value: u32,
}

impl DatadogMetricsEncoder {
    /// Creates a new `DatadogMetricsEncoder` for the given endpoint.
    pub fn new(
        endpoint: DatadogMetricsEndpoint,
        default_namespace: Option<String>,
    ) -> Result<Self, CreateError> {
        let payload_limits = endpoint.payload_limits();
        Self::with_payload_limits(
            endpoint,
            default_namespace,
            payload_limits.uncompressed,
            payload_limits.compressed,
        )
    }

    /// Creates a new `DatadogMetricsEncoder` for the given endpoint, with specific payload limits.
    pub fn with_payload_limits(
        endpoint: DatadogMetricsEndpoint,
        default_namespace: Option<String>,
        uncompressed_limit: usize,
        compressed_limit: usize,
    ) -> Result<Self, CreateError> {
        let (uncompressed_limit, compressed_limit) =
            validate_payload_size_limits(endpoint, uncompressed_limit, compressed_limit)
                .ok_or(CreateError::InvalidLimits)?;

        Ok(Self {
            endpoint,
            default_namespace: default_namespace.map(Arc::from),
            uncompressed_limit,
            compressed_limit,
            state: EncoderState::default(),
            log_schema: log_schema(),
            origin_product_value: *ORIGIN_PRODUCT_VALUE,
        })
    }
}

impl DatadogMetricsEncoder {
    fn reset_state(&mut self) -> EncoderState {
        mem::take(&mut self.state)
    }

    fn encode_single_metric(&mut self, metric: Metric) -> Result<Option<Metric>, EncoderError> {
        // We take special care in this method to capture errors which are not indicative of the
        // metric itself causing a failure in order to be able to return the metric to the caller.
        // The contract of the encoder is such that when an encoding attempt fails for normal
        // reasons, like being out of room, we give back the metric so the caller can finalize the
        // previously encoded metrics and then reset and try again to encode.
        //
        // If the encoder is in a persistent bad state, they'll get back a proper error when calling
        // `finish`, so they eventually get an error, we just make sure they can tidy up before that
        // and avoid needlessly dropping metrics due to unrelated errors.

        // Clear our temporary buffer before any encoding.
        self.state.buf.clear();

        self.state
            .byte_size
            .add_event(&metric, metric.estimated_json_encoded_size_of());

        // For V2 Series metrics, and Sketches: We encode a single Series or Sketch metric incrementally,
        // which means that we specifically write it as if we were writing a single field entry in the
        // overall `SketchPayload` message or `MetricPayload` type.
        //
        // By doing so, we can encode multiple metrics and concatenate all the buffers, and have the
        // resulting buffer appear as if it's a normal `<>Payload` message with a bunch of repeats
        // of the `sketches` / `series` field.
        //
        // Crucially, this code works because `SketchPayload` has two fields -- metadata and sketches --
        // and we never actually set the metadata field... so the resulting message generated overall
        // for `SketchPayload` with a single sketch looks just like as if we literally wrote out a
        // single value for the given field.
        //
        // Similary, `MetricPayload` has a single repeated `series` field.

        match self.endpoint {
            // V1 Series metrics are encoded via JSON, in an incremental fashion.
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V1) => {
                // A single `Metric` might generate multiple Datadog series metrics.
                let all_series = generate_series_metrics(
                    &metric,
                    &self.default_namespace,
                    self.log_schema,
                    self.origin_product_value,
                )?;

                // We handle adding the JSON array separator (comma) manually since the encoding is
                // happening incrementally.
                let has_processed = !self.state.processed.is_empty();
                for (i, series) in all_series.iter().enumerate() {
                    // Add a array delimiter if we already have other metrics encoded.
                    if (has_processed || i > 0)
                        && write_payload_delimiter(self.endpoint, &mut self.state.buf).is_err()
                    {
                        return Ok(Some(metric));
                    }
                    serde_json::to_writer(&mut self.state.buf, series)?;
                }
            }
            // V2 Series metrics are encoded via ProtoBuf, in an incremental fashion.
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V2) => match metric.value() {
                MetricValue::Counter { .. }
                | MetricValue::Gauge { .. }
                | MetricValue::Set { .. }
                | MetricValue::AggregatedSummary { .. } => {
                    let series_proto = series_to_proto_message(
                        &metric,
                        &self.default_namespace,
                        self.log_schema,
                        self.origin_product_value,
                    )?;

                    encode_proto_key_and_message(
                        series_proto,
                        get_series_payload_series_field_number(),
                        &mut self.state.buf,
                    )?;
                }
                value => {
                    return Err(EncoderError::InvalidMetric {
                        expected: "series",
                        metric_value: value.as_name(),
                    })
                }
            },
            // Sketches are encoded via ProtoBuf, also in an incremental fashion.
            DatadogMetricsEndpoint::Sketches => match metric.value() {
                MetricValue::Sketch { sketch } => match sketch {
                    MetricSketch::AgentDDSketch(ddsketch) => {
                        if let Some(sketch_proto) = sketch_to_proto_message(
                            &metric,
                            ddsketch,
                            &self.default_namespace,
                            self.log_schema,
                            self.origin_product_value,
                        ) {
                            encode_proto_key_and_message(
                                sketch_proto,
                                get_sketch_payload_sketches_field_number(),
                                &mut self.state.buf,
                            )?;
                        } else {
                            // If the sketch was empty, that's fine too
                        }
                    }
                },
                value => {
                    return Err(EncoderError::InvalidMetric {
                        expected: "sketches",
                        metric_value: value.as_name(),
                    })
                }
            },
        }

        // Try and see if our temporary buffer can be written to the compressor.
        match self.try_compress_buffer() {
            Err(_) | Ok(false) => Ok(Some(metric)),
            Ok(true) => {
                self.state.processed.push(metric);
                Ok(None)
            }
        }
    }

    fn try_compress_buffer(&mut self) -> io::Result<bool> {
        let n = self.state.buf.len();

        // If we're over our uncompressed size limit with this metric, inform the caller.
        if self.state.written + n > self.uncompressed_limit {
            return Ok(false);
        }

        // Calculating the compressed size is slightly more tricky, because we can only speculate
        // about how many bytes it would take when compressed.  If we write into the compressor, we
        // can't back out that write, even if we manually modify the underlying Vec<u8>, as the
        // compressor might have internal state around checksums, etc, that can't be similarly
        // rolled back.
        //
        // Our strategy is thus: simply take the encoded-but-decompressed size and see if it would
        // fit within the compressed limit.  In `get_endpoint_payload_size_limits`, we calculate the
        // expected maximum overhead of zlib based on all input data being incompressible, which
        // zlib ensures will be the worst case as it can figure out whether a compressed or
        // uncompressed block would take up more space _before_ choosing which strategy to go with.
        //
        // Thus, simply put, we've already accounted for the uncertainty by making our check here
        // assume the worst case while our limits assume the worst case _overhead_.  Maybe our
        // numbers are technically off in the end, but `finish` catches that for us, too.
        let compressed_len = self.state.writer.get_ref().len();
        let max_compressed_metric_len = n + max_compressed_overhead_len(n);
        if compressed_len + max_compressed_metric_len > self.compressed_limit {
            return Ok(false);
        }

        // We should be safe to write our holding buffer to the compressor and store the metric.
        self.state.writer.write_all(&self.state.buf)?;
        self.state.written += n;
        Ok(true)
    }

    /// Attempts to encode a single metric into this encoder.
    ///
    /// For some metric types, the metric will be encoded immediately and we will attempt to
    /// compress it.  For some other metric types, we will store the metric until `finish` is
    /// called, due to the inability to incrementally encode them.
    ///
    /// If the metric could not be encoded into this encoder due to hitting the limits on the size
    /// of the encoded/compressed payload, it will be returned via `Ok(Some(Metric))`, otherwise `Ok(None)`
    /// will be returned.
    ///
    /// If `Ok(Some(Metric))` is returned, callers must call `finish` to finalize the payload.
    /// Further calls to `try_encode` without first calling `finish` may or may not succeed.
    ///
    /// # Errors
    ///
    /// If an error is encountered while attempting to encode the metric, an error variant will be returned.
    pub fn try_encode(&mut self, metric: Metric) -> Result<Option<Metric>, EncoderError> {
        // Make sure we've written our header already.
        if self.state.written == 0 {
            match write_payload_header(self.endpoint, &mut self.state.writer) {
                Ok(n) => self.state.written += n,
                Err(_) => return Ok(Some(metric)),
            }
        }

        self.encode_single_metric(metric)
    }

    pub fn finish(&mut self) -> Result<(EncodeResult<Bytes>, Vec<Metric>), FinishError> {
        // Write any payload footer necessary for the configured endpoint.
        let n = write_payload_footer(self.endpoint, &mut self.state.writer)
            .context(CompressionFailedSnafu)?;
        self.state.written += n;

        let raw_bytes_written = self.state.written;
        let byte_size = self.state.byte_size.clone();

        // Consume the encoder state so we can do our final checks and return the necessary data.
        let state = self.reset_state();
        let payload = state
            .writer
            .finish()
            .context(CompressionFailedSnafu)?
            .freeze();
        let processed = state.processed;

        // We should have configured our limits such that if all calls to `try_compress_buffer` have
        // succeeded up until this point, then our payload should be within the limits after writing
        // the footer and finishing the compressor.
        //
        // We're not only double checking that here, but we're figuring out how much bigger than the
        // limit the payload is, if it is indeed bigger, so that we can recommend how many splits
        // should be used to bring the given set of metrics down to chunks that can be encoded
        // without hitting the limits.
        let compressed_splits = payload.len() / self.compressed_limit;
        let uncompressed_splits = state.written / self.uncompressed_limit;
        let recommended_splits = cmp::max(compressed_splits, uncompressed_splits) + 1;

        if recommended_splits == 1 {
            // "One" split means no splits needed: our payload didn't exceed either of the limits.
            Ok((
                EncodeResult::compressed(payload, raw_bytes_written, byte_size),
                processed,
            ))
        } else {
            Err(FinishError::TooLarge {
                metrics: processed,
                recommended_splits,
            })
        }
    }
}

fn generate_proto_metadata(
    maybe_pass_through: Option<&DatadogMetricOriginMetadata>,
    maybe_source_type: Option<&str>,
    origin_product_value: u32,
) -> Option<ddmetric_proto::Metadata> {
    generate_origin_metadata(maybe_pass_through, maybe_source_type, origin_product_value).map(
        |origin| {
            if origin.product().is_none()
                || origin.category().is_none()
                || origin.service().is_none()
            {
                warn!(
                    message = "Generated sketch origin metadata should have each field set.",
                    product = origin.product(),
                    category = origin.category(),
                    service = origin.service()
                );
            }
            ddmetric_proto::Metadata {
                origin: Some(ddmetric_proto::Origin {
                    origin_product: origin.product().unwrap_or_default(),
                    origin_category: origin.category().unwrap_or_default(),
                    origin_service: origin.service().unwrap_or_default(),
                }),
            }
        },
    )
}

fn get_sketch_payload_sketches_field_number() -> u32 {
    static SKETCH_PAYLOAD_SKETCHES_FIELD_NUM: OnceLock<u32> = OnceLock::new();
    *SKETCH_PAYLOAD_SKETCHES_FIELD_NUM.get_or_init(|| {
        let descriptors = protobuf_descriptors();
        let descriptor = descriptors
            .get_message_by_name("datadog.agentpayload.SketchPayload")
            .expect("should not fail to find `SketchPayload` message in descriptor pool");

        descriptor
            .get_field_by_name("sketches")
            .map(|field| field.number())
            .expect("`sketches` field must exist in `SketchPayload` message")
    })
}

fn get_series_payload_series_field_number() -> u32 {
    static SERIES_PAYLOAD_SERIES_FIELD_NUM: OnceLock<u32> = OnceLock::new();
    *SERIES_PAYLOAD_SERIES_FIELD_NUM.get_or_init(|| {
        let descriptors = protobuf_descriptors();
        let descriptor = descriptors
            .get_message_by_name("datadog.agentpayload.MetricPayload")
            .expect("should not fail to find `MetricPayload` message in descriptor pool");

        descriptor
            .get_field_by_name("series")
            .map(|field| field.number())
            .expect("`series` field must exist in `MetricPayload` message")
    })
}

fn sketch_to_proto_message(
    metric: &Metric,
    ddsketch: &AgentDDSketch,
    default_namespace: &Option<Arc<str>>,
    log_schema: &'static LogSchema,
    origin_product_value: u32,
) -> Option<ddmetric_proto::sketch_payload::Sketch> {
    if ddsketch.is_empty() {
        return None;
    }

    let name = get_namespaced_name(metric, default_namespace);
    let ts = encode_timestamp(metric.timestamp());
    let mut tags = metric.tags().cloned().unwrap_or_default();
    let host = log_schema
        .host_key()
        .map(|key| tags.remove(key.to_string().as_str()).unwrap_or_default())
        .unwrap_or_default();
    let tags = encode_tags(&tags);

    let cnt = ddsketch.count() as i64;
    let min = ddsketch
        .min()
        .expect("min should be present for non-empty sketch");
    let max = ddsketch
        .max()
        .expect("max should be present for non-empty sketch");
    let avg = ddsketch
        .avg()
        .expect("avg should be present for non-empty sketch");
    let sum = ddsketch
        .sum()
        .expect("sum should be present for non-empty sketch");

    let (bins, counts) = ddsketch.bin_map().into_parts();
    let k = bins.into_iter().map(Into::into).collect();
    let n = counts.into_iter().map(Into::into).collect();

    let event_metadata = metric.metadata();
    let metadata = generate_proto_metadata(
        event_metadata.datadog_origin_metadata(),
        event_metadata.source_type(),
        origin_product_value,
    );

    trace!(?metadata, "Generated sketch metadata.");

    Some(ddmetric_proto::sketch_payload::Sketch {
        metric: name,
        tags,
        host,
        distributions: Vec::new(),
        dogsketches: vec![ddmetric_proto::sketch_payload::sketch::Dogsketch {
            ts,
            cnt,
            min,
            max,
            avg,
            sum,
            k,
            n,
        }],
        metadata,
    })
}

fn series_to_proto_message(
    metric: &Metric,
    default_namespace: &Option<Arc<str>>,
    log_schema: &'static LogSchema,
    origin_product_value: u32,
) -> Result<ddmetric_proto::metric_payload::MetricSeries, EncoderError> {
    let metric_name = get_namespaced_name(metric, default_namespace);
    let mut tags = metric.tags().cloned().unwrap_or_default();

    let mut resources = vec![];

    if let Some(host) = log_schema
        .host_key()
        .map(|key| tags.remove(key.to_string().as_str()).unwrap_or_default())
    {
        resources.push(ddmetric_proto::metric_payload::Resource {
            r#type: "host".to_string(),
            name: host,
        });
    }

    // In the `datadog_agent` source, the tag is added as `device` for the V1 endpoint
    // and `resource.device` for the V2 endpoint.
    if let Some(device) = tags.remove("device").or(tags.remove("resource.device")) {
        resources.push(ddmetric_proto::metric_payload::Resource {
            r#type: "device".to_string(),
            name: device,
        });
    }

    let source_type_name = tags.remove("source_type_name").unwrap_or_default();

    let tags = encode_tags(&tags);

    let event_metadata = metric.metadata();
    let metadata = generate_proto_metadata(
        event_metadata.datadog_origin_metadata(),
        event_metadata.source_type(),
        origin_product_value,
    );
    trace!(?metadata, "Generated MetricSeries metadata.");

    let timestamp = encode_timestamp(metric.timestamp());

    // our internal representation is in milliseconds but the expected output is in seconds
    let maybe_interval = metric.interval_ms().map(|i| i.get() / 1000);

    let (points, metric_type) = match metric.value() {
        MetricValue::Counter { value } => {
            if let Some(interval) = maybe_interval {
                // When an interval is defined, it implies the value should be in a per-second form,
                // so we need to get back to seconds from our milliseconds-based interval, and then
                // divide our value by that amount as well.
                let value = *value / (interval as f64);
                (
                    vec![ddmetric_proto::metric_payload::MetricPoint { value, timestamp }],
                    ddmetric_proto::metric_payload::MetricType::Rate,
                )
            } else {
                (
                    vec![ddmetric_proto::metric_payload::MetricPoint {
                        value: *value,
                        timestamp,
                    }],
                    ddmetric_proto::metric_payload::MetricType::Count,
                )
            }
        }
        MetricValue::Set { values } => (
            vec![ddmetric_proto::metric_payload::MetricPoint {
                value: values.len() as f64,
                timestamp,
            }],
            ddmetric_proto::metric_payload::MetricType::Gauge,
        ),
        MetricValue::Gauge { value } => (
            vec![ddmetric_proto::metric_payload::MetricPoint {
                value: *value,
                timestamp,
            }],
            ddmetric_proto::metric_payload::MetricType::Gauge,
        ),
        // NOTE: AggregatedSummary will have been previously split into counters and gauges during normalization
        value => {
            // this case should have already been surfaced by encode_single_metric() so this should never be reached
            return Err(EncoderError::InvalidMetric {
                expected: "series",
                metric_value: value.as_name(),
            });
        }
    };

    Ok(ddmetric_proto::metric_payload::MetricSeries {
        resources,
        metric: metric_name,
        tags,
        points,
        r#type: metric_type.into(),
        // unit is omitted
        unit: "".to_string(),
        source_type_name,
        interval: maybe_interval.unwrap_or(0) as i64,
        metadata,
    })
}

// Manually write the field tag and then encode the Message payload directly as a length-delimited message.
fn encode_proto_key_and_message<T, B>(msg: T, tag: u32, buf: &mut B) -> Result<(), EncoderError>
where
    T: prost::Message,
    B: BufMut,
{
    prost::encoding::encode_key(tag, prost::encoding::WireType::LengthDelimited, buf);

    msg.encode_length_delimited(buf)
        .map_err(|_| EncoderError::ProtoEncodingFailed)
}

fn get_namespaced_name(metric: &Metric, default_namespace: &Option<Arc<str>>) -> String {
    encode_namespace(
        metric
            .namespace()
            .or_else(|| default_namespace.as_ref().map(|s| s.as_ref())),
        '.',
        metric.name(),
    )
}

fn encode_tags(tags: &MetricTags) -> Vec<String> {
    let mut pairs: Vec<_> = tags
        .iter_all()
        .map(|(name, value)| match value {
            Some(value) => format!("{}:{}", name, value),
            None => name.into(),
        })
        .collect();
    pairs.sort();
    pairs
}

fn encode_timestamp(timestamp: Option<DateTime<Utc>>) -> i64 {
    if let Some(ts) = timestamp {
        ts.timestamp()
    } else {
        Utc::now().timestamp()
    }
}

// Given the vector source type, return the OriginService value associated with that integration, if any.
fn source_type_to_service(source_type: &str) -> Option<u32> {
    match source_type {
        // In order to preserve consistent behavior, we intentionally don't set origin metadata
        // for the case where the Datadog Agent did not set it.
        "datadog_agent" => None,

        // These are the sources for which metrics truly originated from this Vector instance.
        "apache_metrics" => Some(17),
        "aws_ecs_metrics" => Some(209),
        "eventstoredb_metrics" => Some(210),
        "host_metrics" => Some(211),
        "internal_metrics" => Some(212),
        "mongodb_metrics" => Some(111),
        "nginx_metrics" => Some(117),
        "open_telemetry" => Some(213),
        "postgresql_metrics" => Some(128),
        "prometheus_remote_write" => Some(214),
        "prometheus_scrape" => Some(215),
        "statsd" => Some(153),

        // These sources are only capable of receiving metrics with the `native` or `native_json` codec.
        // Generally that means the Origin Metadata will have been set as a pass through.
        // However, if the upstream Vector instance did not set Origin Metadata (for example if it is an
        // older version version), we will at least set the OriginProduct and OriginCategory.
        "kafka" | "nats" | "redis" | "gcp_pubsub" | "http_client" | "http_server" | "vector" => {
            Some(0)
        }

        // This scenario should not occur- if it does it means we added a source that deals with metrics,
        // and did not update this function.
        // But if it does occur, by setting the Service value to be undefined, we at least populate the
        // OriginProduct and OriginCategory.
        _ => {
            debug!("Source {source_type} OriginService value is undefined! This source needs to be properly mapped to a Service value.");
            Some(0)
        }
    }
}

/// Determine the correct Origin metadata values to use depending on if they have been
/// set already upstream or not. The generalized struct `DatadogMetricOriginMetadata` is
/// utilized in this function, which allows the series and sketch encoding to call and map
/// the result appropriately for the given protocol they operate on.
fn generate_origin_metadata(
    maybe_pass_through: Option<&DatadogMetricOriginMetadata>,
    maybe_source_type: Option<&str>,
    origin_product_value: u32,
) -> Option<DatadogMetricOriginMetadata> {
    let no_value = 0;

    // An upstream vector source or a transform has set the origin metadata already.
    // Currently this is only possible by these scenarios:
    //     - `datadog_agent` source receiving the metadata on ingested metrics
    //     - `vector` source receiving events with EventMetadata that already has the origins set
    //     - A metrics source configured with the `native` or `native_json` codecs, where the upstream
    //       Vector instance enriched the EventMetadata with Origin metadata.
    //     - `log_to_metric` transform set the OriginService in the EventMetadata when it creates
    //        the new metric.
    if let Some(pass_through) = maybe_pass_through {
        Some(DatadogMetricOriginMetadata::new(
            pass_through.product().or(Some(origin_product_value)),
            pass_through.category().or(Some(ORIGIN_CATEGORY_VALUE)),
            pass_through.service().or(Some(no_value)),
        ))

    // No metadata has been set upstream
    } else {
        maybe_source_type.and_then(|source_type| {
            // Only set the metadata if the source is a metric source we should set it for.
            // In order to preserve consistent behavior, we intentionally don't set origin metadata
            // for the case where the Datadog Agent did not set it.
            source_type_to_service(source_type).map(|origin_service_value| {
                DatadogMetricOriginMetadata::new(
                    Some(origin_product_value),
                    Some(ORIGIN_CATEGORY_VALUE),
                    Some(origin_service_value),
                )
            })
        })
    }
}

fn generate_series_metadata(
    maybe_pass_through: Option<&DatadogMetricOriginMetadata>,
    maybe_source_type: Option<&str>,
    origin_product_value: u32,
) -> Option<DatadogSeriesMetricMetadata> {
    generate_origin_metadata(maybe_pass_through, maybe_source_type, origin_product_value).map(
        |origin| DatadogSeriesMetricMetadata {
            origin: Some(origin),
        },
    )
}

fn generate_series_metrics(
    metric: &Metric,
    default_namespace: &Option<Arc<str>>,
    log_schema: &'static LogSchema,
    origin_product_value: u32,
) -> Result<Vec<DatadogSeriesMetric>, EncoderError> {
    let name = get_namespaced_name(metric, default_namespace);

    let mut tags = metric.tags().cloned().unwrap_or_default();
    let host = log_schema
        .host_key()
        .map(|key| tags.remove(key.to_string().as_str()).unwrap_or_default());

    let source_type_name = tags.remove("source_type_name");
    let device = tags.remove("device");
    let ts = encode_timestamp(metric.timestamp());
    let tags = Some(encode_tags(&tags));

    // our internal representation is in milliseconds but the expected output is in seconds
    let maybe_interval = metric.interval_ms().map(|i| i.get() / 1000);

    let event_metadata = metric.metadata();
    let metadata = generate_series_metadata(
        event_metadata.datadog_origin_metadata(),
        event_metadata.source_type(),
        origin_product_value,
    );

    trace!(?metadata, "Generated series metadata.");

    let (points, metric_type) = match metric.value() {
        MetricValue::Counter { value } => {
            if let Some(interval) = maybe_interval {
                // When an interval is defined, it implies the value should be in a per-second form,
                // so we need to get back to seconds from our milliseconds-based interval, and then
                // divide our value by that amount as well.
                let value = *value / (interval as f64);
                (vec![DatadogPoint(ts, value)], DatadogMetricType::Rate)
            } else {
                (vec![DatadogPoint(ts, *value)], DatadogMetricType::Count)
            }
        }
        MetricValue::Set { values } => (
            vec![DatadogPoint(ts, values.len() as f64)],
            DatadogMetricType::Gauge,
        ),
        MetricValue::Gauge { value } => (vec![DatadogPoint(ts, *value)], DatadogMetricType::Gauge),
        // NOTE: AggregatedSummary will have been previously split into counters and gauges during normalization
        value => {
            return Err(EncoderError::InvalidMetric {
                expected: "series",
                metric_value: value.as_name(),
            })
        }
    };

    Ok(vec![DatadogSeriesMetric {
        metric: name,
        r#type: metric_type,
        interval: maybe_interval,
        points,
        tags,
        host,
        source_type_name,
        device,
        metadata,
    }])
}

fn get_compressor() -> Compressor {
    // We use the "zlib default" compressor because it's all Datadog supports, and adding it
    // generically to `Compression` would make things a little weird because of the conversion trait
    // implementations that are also only none vs gzip.
    Compression::zlib_default().into()
}

const fn max_uncompressed_header_len() -> usize {
    SERIES_PAYLOAD_HEADER.len() + SERIES_PAYLOAD_FOOTER.len()
}

// Datadog ingest APIs accept zlib, which is what we're accounting for here. By default, zlib
// has a 2 byte header and 4 byte CRC trailer. [1]
//
// [1] https://www.zlib.net/zlib_tech.html
const ZLIB_HEADER_TRAILER: usize = 6;

const fn max_compression_overhead_len(compressed_limit: usize) -> usize {
    // We calculate the overhead as the zlib header/trailer plus the worst case overhead of
    // compressing `compressed_limit` bytes, such that we assume all of the data we write may not be
    // compressed at all.
    ZLIB_HEADER_TRAILER + max_compressed_overhead_len(compressed_limit)
}

const fn max_compressed_overhead_len(len: usize) -> usize {
    // Datadog ingest APIs accept zlib, which is what we're accounting for here.
    //
    // Deflate, the underlying compression algorithm, has a technique to ensure that input data
    // can't be encoded in such a way where it's expanded by a meaningful amount. This technique
    // allows storing blocks of uncompressed data with only 5 bytes of overhead per block.
    // Technically, the blocks can be up to 65KB in Deflate, but modern zlib implementations use
    // block sizes of 16KB. [1][2]
    //
    // We calculate the overhead of compressing a given `len` bytes as the worst case of that many
    // bytes being written to the compressor and being unable to be compressed at all
    //
    // [1] https://www.zlib.net/zlib_tech.html
    // [2] https://www.bolet.org/~pornin/deflate-flush-fr.html
    const STORED_BLOCK_SIZE: usize = 16384;
    (1 + len.saturating_sub(ZLIB_HEADER_TRAILER) / STORED_BLOCK_SIZE) * 5
}

const fn validate_payload_size_limits(
    endpoint: DatadogMetricsEndpoint,
    uncompressed_limit: usize,
    compressed_limit: usize,
) -> Option<(usize, usize)> {
    if endpoint.is_series() {
        // For series, we need to make sure the uncompressed limit can account for the header/footer
        // we would add that wraps the encoded metrics up in the expected JSON object. This does
        // imply that adding 1 to this limit would be allowed, and obviously we can't encode a
        // series metric in a single byte, but this is just a simple sanity check, not an exhaustive
        // search of the absolute bare minimum size.
        let header_len = max_uncompressed_header_len();
        if uncompressed_limit <= header_len {
            return None;
        }
    }

    // Get the maximum possible overhead of the compression container, based on the incoming
    // _uncompressed_ limit. We use the uncompressed limit because we're calculating the maximum
    // overhead in the case that, theoretically, none of the input data was compressible.  This
    // possibility is essentially impossible, but serves as a proper worst-case-scenario check.
    let max_compression_overhead = max_compression_overhead_len(uncompressed_limit);
    if compressed_limit <= max_compression_overhead {
        return None;
    }

    Some((uncompressed_limit, compressed_limit))
}

fn write_payload_header(
    endpoint: DatadogMetricsEndpoint,
    writer: &mut dyn io::Write,
) -> io::Result<usize> {
    match endpoint {
        DatadogMetricsEndpoint::Series(SeriesApiVersion::V1) => writer
            .write_all(SERIES_PAYLOAD_HEADER)
            .map(|_| SERIES_PAYLOAD_HEADER.len()),
        _ => Ok(0),
    }
}

fn write_payload_delimiter(
    endpoint: DatadogMetricsEndpoint,
    writer: &mut dyn io::Write,
) -> io::Result<usize> {
    match endpoint {
        DatadogMetricsEndpoint::Series(SeriesApiVersion::V1) => writer
            .write_all(SERIES_PAYLOAD_DELIMITER)
            .map(|_| SERIES_PAYLOAD_DELIMITER.len()),
        _ => Ok(0),
    }
}

fn write_payload_footer(
    endpoint: DatadogMetricsEndpoint,
    writer: &mut dyn io::Write,
) -> io::Result<usize> {
    match endpoint {
        DatadogMetricsEndpoint::Series(SeriesApiVersion::V1) => writer
            .write_all(SERIES_PAYLOAD_FOOTER)
            .map(|_| SERIES_PAYLOAD_FOOTER.len()),
        _ => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{self, copy},
        num::NonZeroU32,
        sync::Arc,
    };

    use bytes::{BufMut, Bytes, BytesMut};
    use chrono::{DateTime, TimeZone, Timelike, Utc};
    use flate2::read::ZlibDecoder;
    use proptest::{
        arbitrary::any, collection::btree_map, num::f64::POSITIVE as ARB_POSITIVE_F64, prop_assert,
        proptest, strategy::Strategy, string::string_regex,
    };
    use prost::Message;
    use vector_lib::{
        config::{log_schema, LogSchema},
        event::{
            metric::{MetricSketch, TagValue},
            DatadogMetricOriginMetadata, EventMetadata, Metric, MetricKind, MetricTags,
            MetricValue,
        },
        metric_tags,
        metrics::AgentDDSketch,
    };

    use super::{
        ddmetric_proto, encode_proto_key_and_message, encode_tags, encode_timestamp,
        generate_series_metrics, get_compressor, get_sketch_payload_sketches_field_number,
        max_compression_overhead_len, max_uncompressed_header_len, series_to_proto_message,
        sketch_to_proto_message, validate_payload_size_limits, write_payload_footer,
        write_payload_header, DatadogMetricsEncoder, EncoderError,
    };
    use crate::{
        common::datadog::DatadogMetricType,
        sinks::datadog::metrics::{
            config::{DatadogMetricsEndpoint, SeriesApiVersion},
            encoder::{DEFAULT_DD_ORIGIN_PRODUCT_VALUE, ORIGIN_PRODUCT_VALUE},
        },
    };

    fn get_simple_counter() -> Metric {
        let value = MetricValue::Counter { value: 3.14 };
        Metric::new("basic_counter", MetricKind::Incremental, value).with_timestamp(Some(ts()))
    }

    fn get_simple_counter_with_metadata(metadata: EventMetadata) -> Metric {
        let value = MetricValue::Counter { value: 3.14 };
        Metric::new_with_metadata("basic_counter", MetricKind::Incremental, value, metadata)
            .with_timestamp(Some(ts()))
    }

    fn get_simple_rate_counter(value: f64, interval_ms: u32) -> Metric {
        let value = MetricValue::Counter { value };
        Metric::new("basic_counter", MetricKind::Incremental, value)
            .with_timestamp(Some(ts()))
            .with_interval_ms(NonZeroU32::new(interval_ms))
    }

    fn get_simple_sketch() -> Metric {
        let mut ddsketch = AgentDDSketch::with_agent_defaults();
        ddsketch.insert(3.14);
        Metric::new("basic_counter", MetricKind::Incremental, ddsketch.into())
            .with_timestamp(Some(ts()))
    }

    fn get_compressed_empty_series_payload() -> Bytes {
        let mut compressor = get_compressor();

        _ = write_payload_header(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V1),
            &mut compressor,
        )
        .expect("should not fail");
        _ = write_payload_footer(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V1),
            &mut compressor,
        )
        .expect("should not fail");

        compressor.finish().expect("should not fail").freeze()
    }

    fn get_compressed_empty_sketches_payload() -> Bytes {
        get_compressor().finish().expect("should not fail").freeze()
    }

    fn decompress_payload(payload: Bytes) -> io::Result<Bytes> {
        let mut decompressor = ZlibDecoder::new(&payload[..]);
        let mut decompressed = BytesMut::new().writer();
        let result = copy(&mut decompressor, &mut decompressed);
        result.map(|_| decompressed.into_inner().freeze())
    }

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
            .single()
            .and_then(|t| t.with_nanosecond(11))
            .expect("invalid timestamp")
    }

    fn tags() -> MetricTags {
        metric_tags! {
            "normal_tag" => "value",
            "true_tag" => "true",
            "empty_tag" => TagValue::Bare,
            "multi_value" => "one",
            "multi_value" => "two",
        }
    }

    fn encode_sketches_normal<B>(
        metrics: &[Metric],
        default_namespace: &Option<Arc<str>>,
        log_schema: &'static LogSchema,
        buf: &mut B,
    ) where
        B: BufMut,
    {
        let mut sketches = Vec::new();
        for metric in metrics {
            let MetricValue::Sketch { sketch } = metric.value() else {
                panic!("must be sketch")
            };
            match sketch {
                MetricSketch::AgentDDSketch(ddsketch) => {
                    if let Some(sketch) =
                        sketch_to_proto_message(metric, ddsketch, default_namespace, log_schema, 14)
                    {
                        sketches.push(sketch);
                    }
                }
            }
        }

        let sketch_payload = ddmetric_proto::SketchPayload {
            metadata: None,
            sketches,
        };

        // Now try encoding this sketch payload, and then try to compress it.
        sketch_payload.encode(buf).unwrap()
    }

    #[test]
    fn test_encode_tags() {
        assert_eq!(
            encode_tags(&tags()),
            vec![
                "empty_tag",
                "multi_value:one",
                "multi_value:two",
                "normal_tag:value",
                "true_tag:true",
            ]
        );
    }

    #[test]
    fn test_encode_timestamp() {
        assert_eq!(encode_timestamp(None), Utc::now().timestamp());
        assert_eq!(encode_timestamp(Some(ts())), 1542182950);
    }

    #[test]
    fn incorrect_metric_for_endpoint_causes_error() {
        // Series metrics can't go to the sketches endpoint.
        let mut sketch_encoder = DatadogMetricsEncoder::new(DatadogMetricsEndpoint::Sketches, None)
            .expect("default payload size limits should be valid");
        let series_result = sketch_encoder.try_encode(get_simple_counter());
        assert!(matches!(
            series_result.err(),
            Some(EncoderError::InvalidMetric { .. })
        ));

        // And sketches can't go to the series endpoint.
        let mut series_v1_encoder =
            DatadogMetricsEncoder::new(DatadogMetricsEndpoint::Series(SeriesApiVersion::V1), None)
                .expect("default payload size limits should be valid");
        let sketch_result = series_v1_encoder.try_encode(get_simple_sketch());
        assert!(matches!(
            sketch_result.err(),
            Some(EncoderError::InvalidMetric { .. })
        ));

        let mut series_v2_encoder =
            DatadogMetricsEncoder::new(DatadogMetricsEndpoint::Series(SeriesApiVersion::V2), None)
                .expect("default payload size limits should be valid");
        let sketch_result = series_v2_encoder.try_encode(get_simple_sketch());
        assert!(matches!(
            sketch_result.err(),
            Some(EncoderError::InvalidMetric { .. })
        ));
    }

    #[test]
    fn encode_counter_with_interval_as_rate() {
        // When a counter explicitly has an interval, we need to encode it as a rate. This means
        // dividing the value by the interval (in seconds) and setting the metric type so that when
        // it lands on the DD side, they can multiply the value by the interval (in seconds) and get
        // back the correct total value for that time period.

        let value = 423.1331;
        let interval_ms = 10000;
        let rate_counter = get_simple_rate_counter(value, interval_ms);
        let expected_value = value / (interval_ms / 1000) as f64;
        let expected_interval = interval_ms / 1000;

        // series v1
        {
            // Encode the metric and make sure we did the rate conversion correctly.
            let result = generate_series_metrics(
                &rate_counter,
                &None,
                log_schema(),
                DEFAULT_DD_ORIGIN_PRODUCT_VALUE,
            );
            assert!(result.is_ok());

            let metrics = result.unwrap();
            assert_eq!(metrics.len(), 1);

            let actual = &metrics[0];
            assert_eq!(actual.r#type, DatadogMetricType::Rate);
            assert_eq!(actual.interval, Some(expected_interval));
            assert_eq!(actual.points.len(), 1);
            assert_eq!(actual.points[0].1, expected_value);
        }

        // series v2
        {
            let series_proto = series_to_proto_message(
                &rate_counter,
                &None,
                log_schema(),
                DEFAULT_DD_ORIGIN_PRODUCT_VALUE,
            )
            .unwrap();
            assert_eq!(series_proto.r#type, 2);
            assert_eq!(series_proto.interval, expected_interval as i64);
            assert_eq!(series_proto.points.len(), 1);
            assert_eq!(series_proto.points[0].value, expected_value);
        }
    }

    #[test]
    fn encode_non_rate_metric_with_interval() {
        // It is possible that the Agent sends Gauges with an interval set. This
        // Occurs when the origin of the metric is Dogstatsd, where the interval
        // is set to 10.

        let value = 423.1331;
        let interval_ms = 10000;

        let gauge = Metric::new(
            "basic_gauge",
            MetricKind::Incremental,
            MetricValue::Gauge { value },
        )
        .with_timestamp(Some(ts()))
        .with_interval_ms(NonZeroU32::new(interval_ms));

        let expected_value = value; // For gauge, the value should not be modified by interval
        let expected_interval = interval_ms / 1000;

        // series v1
        {
            // Encode the metric and make sure we did the rate conversion correctly.
            let result = generate_series_metrics(
                &gauge,
                &None,
                log_schema(),
                DEFAULT_DD_ORIGIN_PRODUCT_VALUE,
            );
            assert!(result.is_ok());

            let metrics = result.unwrap();
            assert_eq!(metrics.len(), 1);

            let actual = &metrics[0];
            assert_eq!(actual.r#type, DatadogMetricType::Gauge);
            assert_eq!(actual.interval, Some(expected_interval));
            assert_eq!(actual.points.len(), 1);
            assert_eq!(actual.points[0].1, expected_value);
        }

        // series v2
        {
            let series_proto = series_to_proto_message(
                &gauge,
                &None,
                log_schema(),
                DEFAULT_DD_ORIGIN_PRODUCT_VALUE,
            )
            .unwrap();
            assert_eq!(series_proto.r#type, 3);
            assert_eq!(series_proto.interval, expected_interval as i64);
            assert_eq!(series_proto.points.len(), 1);
            assert_eq!(series_proto.points[0].value, expected_value);
        }
    }

    #[test]
    fn encode_origin_metadata_pass_through() {
        let product = 10;
        let category = 11;
        let service = 9;

        let event_metadata = EventMetadata::default().with_origin_metadata(
            DatadogMetricOriginMetadata::new(Some(product), Some(category), Some(service)),
        );
        let counter = get_simple_counter_with_metadata(event_metadata);

        // series v1
        {
            let result = generate_series_metrics(
                &counter,
                &None,
                log_schema(),
                DEFAULT_DD_ORIGIN_PRODUCT_VALUE,
            );
            assert!(result.is_ok());

            let metrics = result.unwrap();
            assert_eq!(metrics.len(), 1);

            let actual = &metrics[0];
            let generated_origin = actual.metadata.as_ref().unwrap().origin.as_ref().unwrap();

            assert_eq!(generated_origin.product().unwrap(), product);
            assert_eq!(generated_origin.category().unwrap(), category);
            assert_eq!(generated_origin.service().unwrap(), service);
        }
        // series v2
        {
            let series_proto = series_to_proto_message(
                &counter,
                &None,
                log_schema(),
                DEFAULT_DD_ORIGIN_PRODUCT_VALUE,
            )
            .unwrap();

            let generated_origin = series_proto.metadata.unwrap().origin.unwrap();
            assert_eq!(generated_origin.origin_product, product);
            assert_eq!(generated_origin.origin_category, category);
            assert_eq!(generated_origin.origin_service, service);
        }
    }

    #[test]
    fn encode_origin_metadata_vector_sourced() {
        let product = *ORIGIN_PRODUCT_VALUE;

        let category = 11;
        let service = 153;

        let mut counter = get_simple_counter();

        counter.metadata_mut().set_source_type("statsd");

        // series v1
        {
            let result = generate_series_metrics(&counter, &None, log_schema(), product);
            assert!(result.is_ok());

            let metrics = result.unwrap();
            assert_eq!(metrics.len(), 1);

            let actual = &metrics[0];
            let generated_origin = actual.metadata.as_ref().unwrap().origin.as_ref().unwrap();

            assert_eq!(generated_origin.product().unwrap(), product);
            assert_eq!(generated_origin.category().unwrap(), category);
            assert_eq!(generated_origin.service().unwrap(), service);
        }
        // series v2
        {
            let series_proto = series_to_proto_message(
                &counter,
                &None,
                log_schema(),
                DEFAULT_DD_ORIGIN_PRODUCT_VALUE,
            )
            .unwrap();

            let generated_origin = series_proto.metadata.unwrap().origin.unwrap();
            assert_eq!(generated_origin.origin_product, product);
            assert_eq!(generated_origin.origin_category, category);
            assert_eq!(generated_origin.origin_service, service);
        }
    }

    #[test]
    fn encode_single_series_v1_metric_with_default_limits() {
        // This is a simple test where we ensure that a single metric, with the default limits, can
        // be encoded without hitting any errors.
        let mut encoder =
            DatadogMetricsEncoder::new(DatadogMetricsEndpoint::Series(SeriesApiVersion::V1), None)
                .expect("default payload size limits should be valid");
        let counter = get_simple_counter();
        let expected = counter.clone();

        // Encode the counter.
        let result = encoder.try_encode(counter);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);

        // Finish the payload, make sure we got what we came for.
        let result = encoder.finish();
        assert!(result.is_ok());

        let (_payload, mut processed) = result.unwrap();
        assert_eq!(processed.len(), 1);
        assert_eq!(expected, processed.pop().unwrap());
    }

    #[test]
    fn encode_single_series_v2_metric_with_default_limits() {
        // This is a simple test where we ensure that a single metric, with the default limits, can
        // be encoded without hitting any errors.
        let mut encoder =
            DatadogMetricsEncoder::new(DatadogMetricsEndpoint::Series(SeriesApiVersion::V2), None)
                .expect("default payload size limits should be valid");
        let counter = get_simple_counter();
        let expected = counter.clone();

        // Encode the counter.
        let result = encoder.try_encode(counter);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);

        // Finish the payload, make sure we got what we came for.
        let result = encoder.finish();
        assert!(result.is_ok());

        let (_payload, mut processed) = result.unwrap();
        assert_eq!(processed.len(), 1);
        assert_eq!(expected, processed.pop().unwrap());
    }

    #[test]
    fn encode_single_sketch_metric_with_default_limits() {
        // This is a simple test where we ensure that a single metric, with the default limits, can
        // be encoded without hitting any errors.
        let mut encoder = DatadogMetricsEncoder::new(DatadogMetricsEndpoint::Sketches, None)
            .expect("default payload size limits should be valid");
        let sketch = get_simple_sketch();
        let expected = sketch.clone();

        // Encode the sketch.
        let result = encoder.try_encode(sketch);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);

        // Finish the payload, make sure we got what we came for.
        let result = encoder.finish();
        assert!(result.is_ok());

        let (_payload, mut processed) = result.unwrap();
        assert_eq!(processed.len(), 1);
        assert_eq!(expected, processed.pop().unwrap());
    }

    #[test]
    fn encode_empty_sketch() {
        // This is a simple test where we ensure that a single metric, with the default limits, can
        // be encoded without hitting any errors.
        let mut encoder = DatadogMetricsEncoder::new(DatadogMetricsEndpoint::Sketches, None)
            .expect("default payload size limits should be valid");
        let sketch = Metric::new(
            "empty",
            MetricKind::Incremental,
            AgentDDSketch::with_agent_defaults().into(),
        )
        .with_timestamp(Some(ts()));
        let expected = sketch.clone();

        // Encode the sketch.
        let result = encoder.try_encode(sketch);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);

        // Finish the payload, make sure we got what we came for.
        let result = encoder.finish();
        assert!(result.is_ok());

        let (_payload, mut processed) = result.unwrap();
        assert_eq!(processed.len(), 1);
        assert_eq!(expected, processed.pop().unwrap());
    }

    #[test]
    fn encode_multiple_sketch_metrics_normal_vs_incremental() {
        // This tests our incremental sketch encoding against the more straightforward approach of
        // just building/encoding a full `SketchPayload` message.
        let metrics = vec![
            get_simple_sketch(),
            get_simple_sketch(),
            get_simple_sketch(),
        ];

        let mut normal_buf = Vec::new();
        encode_sketches_normal(&metrics, &None, log_schema(), &mut normal_buf);

        let mut incremental_buf = Vec::new();
        for metric in &metrics {
            match metric.value() {
                MetricValue::Sketch { sketch } => match sketch {
                    MetricSketch::AgentDDSketch(ddsketch) => {
                        if let Some(sketch_proto) =
                            sketch_to_proto_message(metric, ddsketch, &None, log_schema(), 14)
                        {
                            encode_proto_key_and_message(
                                sketch_proto,
                                get_sketch_payload_sketches_field_number(),
                                &mut incremental_buf,
                            )
                            .unwrap();
                        }
                    }
                },
                _ => panic!("should be a sketch"),
            }
        }

        assert_eq!(normal_buf, incremental_buf);
    }

    #[test]
    fn payload_size_limits_series() {
        // Get the maximum length of the header/trailer data.
        let header_len = max_uncompressed_header_len();

        // This is too small.
        let result = validate_payload_size_limits(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V2),
            header_len,
            usize::MAX,
        );
        assert_eq!(result, None);

        // This is just right.
        let result = validate_payload_size_limits(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V2),
            header_len + 1,
            usize::MAX,
        );
        assert_eq!(result, Some((header_len + 1, usize::MAX)));

        // Get the maximum compressed overhead length, based on our input uncompressed size.  This
        // represents the worst case overhead based on the input data (of length usize::MAX, in this
        // case) being entirely incompressible.
        let compression_overhead_len = max_compression_overhead_len(usize::MAX);

        // This is too small.
        let result = validate_payload_size_limits(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V2),
            usize::MAX,
            compression_overhead_len,
        );
        assert_eq!(result, None);

        // This is just right.
        let result = validate_payload_size_limits(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V2),
            usize::MAX,
            compression_overhead_len + 1,
        );
        assert_eq!(result, Some((usize::MAX, compression_overhead_len + 1)));
    }

    #[test]
    fn payload_size_limits_sketches() {
        // There's no lower bound on uncompressed size for the sketches payload.
        let result = validate_payload_size_limits(DatadogMetricsEndpoint::Sketches, 0, usize::MAX);
        assert_eq!(result, Some((0, usize::MAX)));

        // Get the maximum compressed overhead length, based on our input uncompressed size.  This
        // represents the worst case overhead based on the input data (of length usize::MAX, in this
        // case) being entirely incompressible.
        let compression_overhead_len = max_compression_overhead_len(usize::MAX);

        // This is too small.
        let result = validate_payload_size_limits(
            DatadogMetricsEndpoint::Sketches,
            usize::MAX,
            compression_overhead_len,
        );
        assert_eq!(result, None);

        // This is just right.
        let result = validate_payload_size_limits(
            DatadogMetricsEndpoint::Sketches,
            usize::MAX,
            compression_overhead_len + 1,
        );
        assert_eq!(result, Some((usize::MAX, compression_overhead_len + 1)));
    }

    #[test]
    fn encode_series_breaks_out_when_limit_reached_uncompressed() {
        // We manually create the encoder with an arbitrarily low "uncompressed" limit but high
        // "compressed" limit to exercise the codepath that should avoid encoding a metric when the
        // uncompressed payload would exceed the limit.
        let header_len = max_uncompressed_header_len();
        let mut encoder = DatadogMetricsEncoder::with_payload_limits(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V1),
            None,
            header_len + 1,
            usize::MAX,
        )
        .expect("payload size limits should be valid");

        // Trying to encode a metric that would cause us to exceed our uncompressed limits will
        // _not_ return an error from `try_encode`, but instead will simply return back the metric
        // as it could not be added.
        let counter = get_simple_counter();
        let result = encoder.try_encode(counter.clone());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(counter));

        // And similarly, since we didn't actually encode a metric, we _should_ be able to finish
        // this payload, but it will be empty (effectively, the header/footer will exist) and no
        // processed metrics should be returned.
        let result = encoder.finish();
        assert!(result.is_ok());

        let (payload, processed) = result.unwrap();
        assert_eq!(
            payload.uncompressed_byte_size,
            max_uncompressed_header_len()
        );
        assert_eq!(
            payload.into_payload(),
            get_compressed_empty_series_payload()
        );
        assert_eq!(processed.len(), 0);
    }

    #[test]
    fn encode_sketches_breaks_out_when_limit_reached_uncompressed() {
        // We manually create the encoder with an arbitrarily low "uncompressed" limit but high
        // "compressed" limit to exercise the codepath that should avoid encoding a metric when the
        // uncompressed payload would exceed the limit.
        let mut encoder = DatadogMetricsEncoder::with_payload_limits(
            DatadogMetricsEndpoint::Sketches,
            None,
            1,
            usize::MAX,
        )
        .expect("payload size limits should be valid");

        // Trying to encode a metric that would cause us to exceed our uncompressed limits will
        // _not_ return an error from `try_encode`, but instead will simply return back the metric
        // as it could not be added.
        let sketch = get_simple_sketch();
        let result = encoder.try_encode(sketch.clone());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(sketch));

        // And similarly, since we didn't actually encode a metric, we _should_ be able to finish
        // this payload, but it will be empty and no processed metrics should be returned.
        let result = encoder.finish();
        assert!(result.is_ok());

        let (payload, processed) = result.unwrap();
        assert_eq!(payload.uncompressed_byte_size, 0);
        assert_eq!(
            payload.into_payload(),
            get_compressed_empty_sketches_payload()
        );
        assert_eq!(processed.len(), 0);
    }

    #[test]
    fn encode_series_breaks_out_when_limit_reached_compressed() {
        // We manually create the encoder with an arbitrarily low "compressed" limit but high
        // "uncompressed" limit to exercise the codepath that should avoid encoding a metric when the
        // compressed payload would exceed the limit.
        let uncompressed_limit = 128;
        let compressed_limit = 32;
        let mut encoder = DatadogMetricsEncoder::with_payload_limits(
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V1),
            None,
            uncompressed_limit,
            compressed_limit,
        )
        .expect("payload size limits should be valid");

        // Trying to encode a metric that would cause us to exceed our compressed limits will
        // _not_ return an error from `try_encode`, but instead will simply return back the metric
        // as it could not be added.
        let counter = get_simple_counter();
        let result = encoder.try_encode(counter.clone());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(counter));

        // And similarly, since we didn't actually encode a metric, we _should_ be able to finish
        // this payload, but it will be empty (effectively, the header/footer will exist) and no
        // processed metrics should be returned.
        let result = encoder.finish();
        assert!(result.is_ok());

        let (payload, processed) = result.unwrap();
        assert_eq!(
            payload.uncompressed_byte_size,
            max_uncompressed_header_len()
        );
        assert_eq!(
            payload.into_payload(),
            get_compressed_empty_series_payload()
        );
        assert_eq!(processed.len(), 0);
    }

    #[test]
    fn encode_sketches_breaks_out_when_limit_reached_compressed() {
        // We manually create the encoder with an arbitrarily low "compressed" limit but high
        // "uncompressed" limit to exercise the codepath that should avoid encoding a metric when the
        // compressed payload would exceed the limit.
        let uncompressed_limit = 128;
        let compressed_limit = 16;
        let mut encoder = DatadogMetricsEncoder::with_payload_limits(
            DatadogMetricsEndpoint::Sketches,
            None,
            uncompressed_limit,
            compressed_limit,
        )
        .expect("payload size limits should be valid");

        // Trying to encode a metric that would cause us to exceed our compressed limits will
        // _not_ return an error from `try_encode`, but instead will simply return back the metric
        // as it could not be added.
        let sketch = get_simple_sketch();
        let result = encoder.try_encode(sketch.clone());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(sketch));

        // And similarly, since we didn't actually encode a metric, we _should_ be able to finish
        // this payload, but it will be empty (effectively, the header/footer will exist) and no
        // processed metrics should be returned.
        let result = encoder.finish();
        assert!(result.is_ok());

        let (payload, processed) = result.unwrap();
        assert_eq!(payload.uncompressed_byte_size, 0);
        assert_eq!(
            payload.into_payload(),
            get_compressed_empty_sketches_payload()
        );
        assert_eq!(processed.len(), 0);
    }

    fn arb_counter_metric() -> impl Strategy<Value = Metric> {
        let name = string_regex("[a-zA-Z][a-zA-Z0-9_]{8,96}").expect("regex should not be invalid");
        let value = ARB_POSITIVE_F64;
        let tags = btree_map(
            any::<u64>().prop_map(|v| v.to_string()),
            any::<u64>().prop_map(|v| v.to_string()),
            0..64,
        )
        .prop_map(|tags| (!tags.is_empty()).then(|| MetricTags::from(tags)));

        (name, value, tags).prop_map(|(metric_name, metric_value, metric_tags)| {
            let metric_value = MetricValue::Counter {
                value: metric_value,
            };
            Metric::new(metric_name, MetricKind::Incremental, metric_value).with_tags(metric_tags)
        })
    }

    proptest! {
        #[test]
        fn encoding_check_for_payload_limit_edge_cases(
            uncompressed_limit in 0..64_000_000usize,
            compressed_limit in 0..10_000_000usize,
            metric in arb_counter_metric(),
        ) {
            // We simply try to encode a single metric into an encoder, and make sure that when we
            // finish the payload, if it didn't result in an error, that the payload was under the
            // configured limits.
            //
            // We check this with targeted unit tests as well but this is some cheap insurance to
            // show that we're hopefully not missing any particular corner cases.
            let result = DatadogMetricsEncoder::with_payload_limits(
                DatadogMetricsEndpoint::Series(SeriesApiVersion::V2),
                None,
                uncompressed_limit,
                compressed_limit,
            );
            if let Ok(mut encoder) = result {
                _ = encoder.try_encode(metric);

                if let Ok((payload, _processed)) = encoder.finish() {
                    let payload = payload.into_payload();
                    prop_assert!(payload.len() <= compressed_limit);

                    let result = decompress_payload(payload);
                    prop_assert!(result.is_ok());

                    let decompressed = result.unwrap();
                    prop_assert!(decompressed.len() <= uncompressed_limit);
                }
            }
        }
    }
}
