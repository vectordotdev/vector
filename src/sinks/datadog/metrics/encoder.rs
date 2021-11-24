use std::{
    cmp,
    collections::BTreeMap,
    convert::TryInto,
    io::{self, Write},
    mem,
    sync::Arc,
    time::Instant,
};

use bytes::BufMut;
use chrono::{DateTime, Utc};
use prost::Message;
use snafu::{ResultExt, Snafu};
use vector_core::{
    config::{log_schema, LogSchema},
    event::{metric::MetricSketch, Metric, MetricValue},
};

use crate::{
    common::datadog::{DatadogMetricType, DatadogPoint, DatadogSeriesMetric},
    sinks::util::{encode_namespace, Compressor},
};

use super::config::{
    DatadogMetricsEndpoint, MAXIMUM_PAYLOAD_COMPRESSED_SIZE, MAXIMUM_PAYLOAD_SIZE,
};

const SERIES_PAYLOAD_HEADER: &[u8] = b"{\"series\":[";
const SERIES_PAYLOAD_FOOTER: &[u8] = b"]}";
const SERIES_PAYLOAD_DELIMITER: &[u8] = b",";

mod ddsketch_proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

#[derive(Debug, Snafu)]
pub enum CreateError {
    #[snafu(display("Invalid compressed/uncompressed payload size limits were given"))]
    InvalidLimits,
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

    #[snafu(display("Failed to encode series metrics to JSON: {}", source))]
    JsonEncodingFailed { source: serde_json::Error },

    #[snafu(display("Failed to encode sketch metrics to Protocol Buffers: {}", source))]
    ProtoEncodingFailed { source: prost::EncodeError },
}

#[derive(Debug, Snafu)]
pub enum FinishError {
    #[snafu(display(
        "Failure occurred during writing to or finalizing the compressor: {}",
        source
    ))]
    CompressionFailed { source: io::Error },

    #[snafu(display("Failed to encode pending metrics: {}", source))]
    PendingEncodeFailed { source: EncoderError },

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
            Self::PendingEncodeFailed { .. } => "pendiong_encode_failed",
            Self::TooLarge { .. } => "too_large",
        }
    }
}

struct EncoderState {
    writer: Compressor,
    written: usize,
    buf: Vec<u8>,

    pending: Vec<Metric>,
    processed: Vec<Metric>,
}

impl Default for EncoderState {
    fn default() -> Self {
        EncoderState {
            // We use the "zlib default" compressor because it's all Datadog supports, and adding it
            // generically to `Compression` would make things a little weird because of the
            // conversion trait implementations that are also only none vs gzip.
            writer: get_compressor(),
            written: 0,
            buf: Vec::with_capacity(1024),
            pending: Vec::new(),
            processed: Vec::new(),
        }
    }
}

pub struct DatadogMetricsEncoder {
    endpoint: DatadogMetricsEndpoint,
    default_namespace: Option<Arc<str>>,
    uncompressed_limit: usize,
    compressed_limit: usize,

    state: EncoderState,
    last_sent: Option<Instant>,
    log_schema: &'static LogSchema,
}

impl DatadogMetricsEncoder {
    /// Creates a new `DatadogMetricsEncoder` for the given endpoint.
    pub fn new(
        endpoint: DatadogMetricsEndpoint,
        default_namespace: Option<String>,
    ) -> Result<Self, CreateError> {
        // According to the datadog-agent code, sketches use the same payload size limits as series
        // data. We're just gonna go with that for now.
        Self::with_payload_limits(
            endpoint,
            default_namespace,
            MAXIMUM_PAYLOAD_SIZE,
            MAXIMUM_PAYLOAD_COMPRESSED_SIZE,
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
            validate_payload_size_limits(uncompressed_limit, compressed_limit)
                .ok_or(CreateError::InvalidLimits)?;

        Ok(Self {
            endpoint,
            default_namespace: default_namespace.map(Arc::from),
            uncompressed_limit,
            compressed_limit,
            state: EncoderState::default(),
            last_sent: None,
            log_schema: log_schema(),
        })
    }
}

impl DatadogMetricsEncoder {
    fn reset_state(&mut self) -> EncoderState {
        self.last_sent = Some(Instant::now());
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

        match self.endpoint {
            // Series metrics are encoded via JSON, in an incremental fashion.
            DatadogMetricsEndpoint::Series => {
                // A single `Metric` might generate multiple Datadog series metrics.
                let all_series = generate_series_metrics(
                    &metric,
                    &self.default_namespace,
                    self.log_schema,
                    self.last_sent,
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
                    let _ = serde_json::to_writer(&mut self.state.buf, series)
                        .context(JsonEncodingFailed)?;
                }
            }
            // We can't encode sketches incrementally (yet), so we don't do any encoding here.  We
            // simply store it for later, and in `try_encode_pending`, any such pending metrics will be
            // encoded in a single operation.
            DatadogMetricsEndpoint::Sketches => match metric.value() {
                MetricValue::Sketch { .. } => {}
                value => {
                    return Err(EncoderError::InvalidMetric {
                        expected: "sketches",
                        metric_value: value.as_name(),
                    })
                }
            },
        }

        // If we actually encoded a metric, we try to see if our temporary buffer can be compressed
        // and added to the overall payload.  Otherwise, it means we're deferring the metric for
        // later encoding, so we store it off to the side.
        if !self.state.buf.is_empty() {
            match self.try_compress_buffer() {
                Err(_) | Ok(false) => return Ok(Some(metric)),
                Ok(true) => {}
            }

            self.state.processed.push(metric);
        } else {
            self.state.pending.push(metric);
        }

        Ok(None)
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
        if compressed_len + n > self.compressed_limit {
            return Ok(false);
        }

        // We should be safe to write our holding buffer to the compressor and store the metric.
        let _ = self.state.writer.write_all(&self.state.buf)?;
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

    fn try_encode_pending(&mut self) -> Result<(), FinishError> {
        // The Datadog Agent uses a particular Protocol Buffers library to incrementally encode the
        // DDSketch structures into a payload, similar to how we incrementally encode the series
        // metrics.  Unfortunately, there's no existing Rust crate that allows writing out Protocol
        // Buffers payloads by hand, so we have to cheat a little and buffer up the metrics until
        // the very end.
        //
        // `try_encode`, and thus `encode_single_metric`, specifically store sketch-oriented metrics
        // off to the side for this very purpose, letting us gather them all here, encoding them
        // into a single Protocol Buffers payload.
        //
        // Naturally, this means we might actually generate a payload that's too big.  This is a
        // problem for the caller to figure out.  Presently, the only usage of this encoder will
        // naively attempt to split the batch into two and try again.

        // Only go through this if we're targeting the sketch endpoint.
        if !(matches!(self.endpoint, DatadogMetricsEndpoint::Sketches)) {
            return Ok(());
        }

        // Consume of all of the "pending" metrics and try to write them out as sketches.
        let pending = mem::take(&mut self.state.pending);
        let _ = write_sketches(
            &pending,
            &self.default_namespace,
            self.log_schema,
            &mut self.state.buf,
        )
        .context(PendingEncodeFailed)?;

        if self.try_compress_buffer().context(CompressionFailed)? {
            // Since we encoded and compressed them successfully, add them to the "processed" list.
            self.state.processed.extend(pending);
            Ok(())
        } else {
            // The payload was too big overall, which we can't do anything about.  Up to the caller
            // now to try to encode them again after splitting the batch.
            Err(FinishError::TooLarge {
                metrics: pending,
                // TODO: Hard-coded split code for now because we need to hoist up the logic for
                // calculating the recommended splits to an instance method or something.
                recommended_splits: 2,
            })
        }
    }

    pub fn finish(&mut self) -> Result<(Vec<u8>, Vec<Metric>), FinishError> {
        // Try to encode any pending metrics we had stored up.
        let _ = self.try_encode_pending()?;

        // Write any payload footer necessary for the configured endpoint.
        let n = write_payload_footer(self.endpoint, &mut self.state.writer)
            .context(CompressionFailed)?;
        self.state.written += n;

        // Consume the encoder state so we can do our final checks and return the necessary data.
        let state = self.reset_state();
        let payload = state.writer.finish().context(CompressionFailed)?;
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
            Ok((payload, processed))
        } else {
            Err(FinishError::TooLarge {
                metrics: processed,
                recommended_splits,
            })
        }
    }
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

fn encode_tags(tags: &BTreeMap<String, String>) -> Vec<String> {
    let mut pairs: Vec<_> = tags
        .iter()
        .map(|(name, value)| format!("{}:{}", name, value))
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

fn generate_series_metrics(
    metric: &Metric,
    default_namespace: &Option<Arc<str>>,
    log_schema: &'static LogSchema,
    last_sent: Option<Instant>,
) -> Result<Vec<DatadogSeriesMetric>, EncoderError> {
    let name = get_namespaced_name(metric, default_namespace);

    let mut tags = metric.tags().cloned().unwrap_or_default();
    let host = tags.remove(log_schema.host_key());
    let source_type_name = tags.remove("source_type_name");
    let device = tags.remove("device");
    let ts = encode_timestamp(metric.timestamp());
    let tags = Some(encode_tags(&tags));
    let interval = last_sent
        .map(|then| then.elapsed())
        .map(|d| d.as_secs().try_into().unwrap_or(i64::MAX));

    let results = match metric.value() {
        MetricValue::Counter { value } => vec![DatadogSeriesMetric {
            metric: name,
            r#type: DatadogMetricType::Count,
            interval,
            points: vec![DatadogPoint(ts, *value)],
            tags,
            host,
            source_type_name,
            device,
        }],
        MetricValue::Set { values } => vec![DatadogSeriesMetric {
            metric: name,
            r#type: DatadogMetricType::Gauge,
            interval: None,
            points: vec![DatadogPoint(ts, values.len() as f64)],
            tags,
            host,
            source_type_name,
            device,
        }],
        MetricValue::Gauge { value } => vec![DatadogSeriesMetric {
            metric: name,
            r#type: DatadogMetricType::Gauge,
            interval: None,
            points: vec![DatadogPoint(ts, *value)],
            tags,
            host,
            source_type_name,
            device,
        }],
        MetricValue::AggregatedSummary {
            quantiles,
            count,
            sum,
        } => {
            let mut results = vec![
                DatadogSeriesMetric {
                    metric: format!("{}.count", &name),
                    r#type: DatadogMetricType::Rate,
                    interval,
                    points: vec![DatadogPoint(ts, f64::from(*count))],
                    tags: tags.clone(),
                    host: host.clone(),
                    source_type_name: source_type_name.clone(),
                    device: device.clone(),
                },
                DatadogSeriesMetric {
                    metric: format!("{}.sum", &name),
                    r#type: DatadogMetricType::Gauge,
                    interval: None,
                    points: vec![DatadogPoint(ts, *sum)],
                    tags: tags.clone(),
                    host: host.clone(),
                    source_type_name: source_type_name.clone(),
                    device: device.clone(),
                },
            ];

            for quantile in quantiles {
                results.push(DatadogSeriesMetric {
                    metric: format!("{}.{}percentile", &name, quantile.as_percentile()),
                    r#type: DatadogMetricType::Gauge,
                    interval: None,
                    points: vec![DatadogPoint(ts, quantile.value)],
                    tags: tags.clone(),
                    host: host.clone(),
                    source_type_name: source_type_name.clone(),
                    device: device.clone(),
                })
            }
            results
        }
        value => {
            return Err(EncoderError::InvalidMetric {
                expected: "series",
                metric_value: value.as_name(),
            })
        }
    };

    Ok(results)
}

fn write_sketches<B>(
    metrics: &[Metric],
    default_namespace: &Option<Arc<str>>,
    log_schema: &'static LogSchema,
    buf: &mut B,
) -> Result<(), EncoderError>
where
    B: BufMut,
{
    let mut sketches = Vec::new();
    for metric in metrics {
        match metric.value() {
            MetricValue::Sketch { sketch } => match sketch {
                MetricSketch::AgentDDSketch(ddsketch) => {
                    // Don't encode any empty sketches.
                    if ddsketch.is_empty() {
                        continue;
                    }

                    let name = get_namespaced_name(metric, default_namespace);
                    let ts = encode_timestamp(metric.timestamp());
                    let mut tags = metric.tags().cloned().unwrap_or_default();
                    let host = tags.remove(log_schema.host_key()).unwrap_or_default();
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

                    let sketch = ddsketch_proto::sketch_payload::Sketch {
                        metric: name,
                        tags,
                        host,
                        distributions: Vec::new(),
                        dogsketches: vec![ddsketch_proto::sketch_payload::sketch::Dogsketch {
                            ts,
                            cnt,
                            min,
                            max,
                            avg,
                            sum,
                            k,
                            n,
                        }],
                    };

                    sketches.push(sketch);
                }
            },
            // We filter out non-sketch metrics during `encode_single_metric` if we're targeting
            // the sketches endpoint.
            _ => unreachable!(),
        }
    }

    let sketch_payload = ddsketch_proto::SketchPayload {
        // TODO: The "common metadata" fields are things that only very loosely apply to Vector, or
        // are hard to characterize -- for example, what's the API key for a sketch that didn't originate
        // from the Datadog Agent? -- so we're just omitting it here in the hopes it doesn't
        // actually matter.
        metadata: None,
        sketches,
    };

    // Now try encoding this sketch payload, and then try to compress it.
    sketch_payload.encode(buf).context(ProtoEncodingFailed)
}

fn get_compressor() -> Compressor {
    Compressor::zlib_default()
}

const fn max_uncompressed_header_len() -> usize {
    SERIES_PAYLOAD_HEADER.len() + SERIES_PAYLOAD_FOOTER.len()
}

const fn max_compression_overhead_len(compressed_limit: usize) -> usize {
    // Datadog ingest APIs accept zlib, which is what we're accounting for here. By default, zlib
    // has a 2 byte header and 4 byte CRC trailer. Additionally, Deflate, the underlying
    // compression algorithm, has a technique to ensure that input data can't be encoded in such a
    // way where it's expanded by a meaningful amount.
    //
    // This technique allows storing blocks of uncompressed data with only 5 bytes of overhead per
    // block. Technically, the blocks can be up to 65KB in Deflate, but modern zlib implementations
    // use block sizes of 16KB. [1][2]
    //
    // With all of that said, we calculate the overhead as the header plus trailer plus the given
    // compressed size limit, minus the known overhead, multiplied such that it accounts for the
    // worse case of entirely uncompressed data.
    //
    // [1] https://www.zlib.net/zlib_tech.html
    // [2] https://www.bolet.org/~pornin/deflate-flush-fr.html
    const HEADER_TRAILER: usize = 6;
    const STORED_BLOCK_SIZE: usize = 16384;
    HEADER_TRAILER + (1 + compressed_limit.saturating_sub(HEADER_TRAILER) / STORED_BLOCK_SIZE) * 5
}

const fn validate_payload_size_limits(
    uncompressed_limit: usize,
    compressed_limit: usize,
) -> Option<(usize, usize)> {
    // Get the maximum possible length of the header/footer combined.
    //
    // This only matters for series metrics at the moment, since sketches are encoded in a single
    // shot to their Protocol Buffers representation.  We're "wasting" `header_len` bytes in the
    // case of sketches, but we're alsdo talking about like 10 bytes: not enough to care about.
    let header_len = max_uncompressed_header_len();
    if uncompressed_limit <= header_len {
        return None;
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
        DatadogMetricsEndpoint::Series => writer
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
        DatadogMetricsEndpoint::Series => writer
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
        DatadogMetricsEndpoint::Series => writer
            .write_all(SERIES_PAYLOAD_FOOTER)
            .map(|_| SERIES_PAYLOAD_FOOTER.len()),
        _ => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        io::{self, copy},
    };

    use chrono::{DateTime, TimeZone, Utc};
    use flate2::read::ZlibDecoder;
    use proptest::{
        arbitrary::any, collection::btree_map, num::f64::POSITIVE as ARB_POSITIVE_F64, prop_assert,
        proptest, strategy::Strategy, string::string_regex,
    };
    use vector_core::{
        event::{Metric, MetricKind, MetricValue},
        metrics::AgentDDSketch,
    };

    use crate::sinks::datadog::metrics::{config::DatadogMetricsEndpoint, encoder::EncoderError};

    use super::{
        encode_tags, encode_timestamp, get_compressor, max_compression_overhead_len,
        max_uncompressed_header_len, validate_payload_size_limits, write_payload_footer,
        write_payload_header, DatadogMetricsEncoder,
    };

    fn get_simple_counter() -> Metric {
        let value = MetricValue::Counter { value: 3.14 };
        Metric::new("basic_counter", MetricKind::Incremental, value)
    }

    fn get_simple_sketch() -> Metric {
        let mut ddsketch = AgentDDSketch::with_agent_defaults();
        ddsketch.insert(3.14);
        Metric::new("basic_counter", MetricKind::Incremental, ddsketch.into())
    }

    fn get_compressed_empty_series_payload() -> Vec<u8> {
        let mut compressor = get_compressor();

        let _ = write_payload_header(DatadogMetricsEndpoint::Series, &mut compressor)
            .expect("should not fail");
        let _ = write_payload_footer(DatadogMetricsEndpoint::Series, &mut compressor)
            .expect("should not fail");

        compressor.finish().expect("should not fail")
    }

    fn decompress_payload(payload: Vec<u8>) -> io::Result<Vec<u8>> {
        let mut decompressor = ZlibDecoder::new(&payload[..]);
        let mut decompressed = Vec::new();
        let result = copy(&mut decompressor, &mut decompressed);
        result.map(|_| decompressed)
    }

    fn ts() -> DateTime<Utc> {
        Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)
    }

    fn tags() -> BTreeMap<String, String> {
        vec![
            ("normal_tag".to_owned(), "value".to_owned()),
            ("true_tag".to_owned(), "true".to_owned()),
            ("empty_tag".to_owned(), "".to_owned()),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn test_encode_tags() {
        assert_eq!(
            encode_tags(&tags()),
            vec!["empty_tag:", "normal_tag:value", "true_tag:true"]
        );
    }

    #[test]
    fn test_encode_timestamp() {
        assert_eq!(encode_timestamp(None), Utc::now().timestamp());
        assert_eq!(encode_timestamp(Some(ts())), 1542182950);
    }

    #[test]
    fn incorrect_metric_for_endpoint_causes_error() {
        // Series metrics can't gbo to the sketches endpoint.
        let mut sketch_encoder = DatadogMetricsEncoder::new(DatadogMetricsEndpoint::Sketches, None)
            .expect("default payload size limits should be valid");
        let series_result = sketch_encoder.try_encode(get_simple_counter());
        assert!(matches!(
            series_result.err(),
            Some(EncoderError::InvalidMetric { .. })
        ));

        // And sketches can't go to the series endpoint.
        // Series metrics can't gbo to the sketches endpoint.
        let mut series_encoder = DatadogMetricsEncoder::new(DatadogMetricsEndpoint::Series, None)
            .expect("default payload size limits should be valid");
        let sketch_result = series_encoder.try_encode(get_simple_sketch());
        assert!(matches!(
            sketch_result.err(),
            Some(EncoderError::InvalidMetric { .. })
        ));
    }

    #[test]
    fn encode_single_series_metric_with_default_limits() {
        // This is a simple test where we ensure that a single metric, with the default limits, can
        // be encoded without hitting any errors.
        let mut encoder = DatadogMetricsEncoder::new(DatadogMetricsEndpoint::Series, None)
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
    fn payload_size_limits() {
        // Get the maximum length of the header/trailer data.
        let header_len = max_uncompressed_header_len();

        // This is too small.
        let result = validate_payload_size_limits(header_len, usize::MAX);
        assert_eq!(result, None);

        // This is just right.
        let result = validate_payload_size_limits(header_len + 1, usize::MAX);
        assert_eq!(result, Some((header_len + 1, usize::MAX)));

        // Get the maximum compressed overhead length, based on our input uncompressed size.  This
        // represents the worst case overhead based on the input data (of length usize::MAX, in this
        // case) being entirely incompressible.
        let compression_overhead_len = max_compression_overhead_len(usize::MAX);

        // This is too small.
        let result = validate_payload_size_limits(usize::MAX, compression_overhead_len);
        assert_eq!(result, None);

        // This is just right.
        let result = validate_payload_size_limits(usize::MAX, compression_overhead_len + 1);
        assert_eq!(result, Some((usize::MAX, compression_overhead_len + 1)));
    }

    #[test]
    fn encode_breaks_out_when_limit_reached_uncompressed() {
        // We manually create the encoder with an arbitrarily low "uncompressed" limit but high
        // "compressed" limit to exercise the codepath that should avoid encoding a metric when the
        // uncompressed payload would exceed the limit.
        let header_len = max_uncompressed_header_len();
        let mut encoder = DatadogMetricsEncoder::with_payload_limits(
            DatadogMetricsEndpoint::Series,
            None,
            header_len + 1,
            usize::MAX,
        )
        .expect("payload size limits should be valid");

        // Trying to encode a metric that would cause us to exceed our uncompressed limits will
        // _not_ return an error from `try_encode`.
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
        let empty_payload = get_compressed_empty_series_payload();
        assert_eq!(payload, empty_payload);
        assert_eq!(processed.len(), 0);
    }

    #[test]
    fn encode_breaks_out_when_limit_reached_compressed() {
        // We manually create the encoder with an arbitrarily low "compressed" limit but high
        // "uncompressed" limit to exercise the codepath that should avoid encoding a metric when the
        // compressed payload would exceed the limit.
        let uncompressed_limit = 128;
        let compressed_limit = 32;
        let mut encoder = DatadogMetricsEncoder::with_payload_limits(
            DatadogMetricsEndpoint::Series,
            None,
            uncompressed_limit,
            compressed_limit,
        )
        .expect("payload size limits should be valid");

        // Trying to encode a metric that would cause us to exceed our compressed limits will
        // _not_ return an error from `try_encode`.
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
        let empty_payload = get_compressed_empty_series_payload();
        assert_eq!(payload, empty_payload);
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
        .prop_map(|tags| if tags.is_empty() { None } else { Some(tags) });

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
                DatadogMetricsEndpoint::Series,
                None,
                uncompressed_limit,
                compressed_limit,
            );
            if let Ok(mut encoder) = result {
                let _ = encoder.try_encode(metric);

                if let Ok((payload, _processed)) = encoder.finish() {
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
