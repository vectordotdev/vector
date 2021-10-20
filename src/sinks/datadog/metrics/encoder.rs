use std::{
    cmp,
    collections::BTreeMap,
    convert::TryInto,
    io::{self, Write},
    mem,
    time::Instant,
};

use chrono::{DateTime, Utc};
use prost::Message;
use serde::Serialize;
use snafu::{ResultExt, Snafu};
use vector_core::event::{metric::MetricSketch, Metric, MetricValue};

use crate::sinks::util::{encode_namespace, Compressor};

use super::config::{
    DatadogMetricsEndpoint, MAXIMUM_SERIES_PAYLOAD_COMPRESSED_SIZE, MAXIMUM_SERIES_PAYLOAD_SIZE,
};

const SERIES_PAYLOAD_HEADER: &[u8] = b"{\"series\":[";
const SERIES_PAYLOAD_FOOTER: &[u8] = b"]}";
const SERIES_PAYLOAD_DELIMITER: &[u8] = b",";

mod ddsketch_proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

#[derive(Debug, Snafu)]
pub enum EncoderError {
    #[snafu(display(
        "Invalid metric value '{}' was given for the configured endpoint '{:?}'",
        metric_value,
        endpoint
    ))]
    InvalidMetric {
        endpoint: DatadogMetricsEndpoint,
        metric_value: &'static str,
    },

    #[snafu(display("I/O error encountered during encoding/finishing: {}", source))]
    Io { source: io::Error },

    #[snafu(display("Failed to encode series metrics to JSON: {}", source))]
    JsonEncodingFailed { source: serde_json::Error },

    #[snafu(display("Failed to encode sketch metrics to Protocol Buffers: {}", source))]
    ProtoEncodingFailed { source: prost::EncodeError },

    #[snafu(display("Finished payload exceeded the (un)compressed size limits"))]
    TooLarge {
        metrics: Vec<Metric>,
        recommended_splits: usize,
    },
}

impl From<io::Error> for EncoderError {
    fn from(source: io::Error) -> Self {
        EncoderError::Io { source }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct DatadogSeriesMetric {
    metric: String,
    r#type: DatadogMetricType,
    interval: Option<i64>,
    points: Vec<DatadogPoint<f64>>,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatadogMetricType {
    Gauge,
    Count,
    Rate,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct DatadogPoint<T>(i64, T);

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
            writer: Compressor::zlib_default(),
            written: 0,
            buf: Vec::with_capacity(1024),
            pending: Vec::new(),
            processed: Vec::new(),
        }
    }
}

pub struct DatadogMetricsEncoder {
    endpoint: DatadogMetricsEndpoint,
    default_namespace: Option<String>,
    uncompressed_limit: usize,
    compressed_limit: usize,

    state: EncoderState,
    last_sent: Option<Instant>,
}

impl DatadogMetricsEncoder {
    /// Creates a new `DatadogMetricsEncoder` for the given endpoint.
    pub fn new(endpoint: DatadogMetricsEndpoint, default_namespace: Option<String>) -> Self {
        // Calculate the payload size limits for the given endpoint.
        let (uncompressed_limit, compressed_limit) = get_endpoint_payload_size_limits();

        Self {
            endpoint,
            default_namespace,
            uncompressed_limit,
            compressed_limit,
            state: EncoderState::default(),
            last_sent: None,
        }
    }
}

impl DatadogMetricsEncoder {
    fn reset_state(&mut self) -> EncoderState {
        self.last_sent = Some(Instant::now());
        mem::take(&mut self.state)
    }

    fn get_namespaced_name(&self, metric: &Metric) -> String {
        encode_namespace(
            metric
                .namespace()
                .or(self.default_namespace.as_ref().map(|s| s.as_str())),
            '.',
            metric.name(),
        )
    }

    fn encode_single_metric(&mut self, metric: Metric) -> Result<Option<Metric>, EncoderError> {
        // Clear our temporary buffer before any encoding.
        self.state.buf.clear();

        match self.endpoint {
            // Series metrics are encoded via JSON, in an incremental fashion.
            DatadogMetricsEndpoint::Series => {
                // A single `Metric` might generate multiple Datadog series metrics.
                let all_series = self.generate_series_metrics(&metric)?;

                // We handle adding the JSON array separator (comma) manually since the encoding is
                // happening incrementally.
                let has_processed = !self.state.processed.is_empty();
                for (i, series) in all_series.iter().enumerate() {
                    // Add a array delimiter if we already have other metrics encoded.
                    if has_processed || i > 0 {
                        let _ = write_payload_delimiter(self.endpoint, &mut self.state.buf)
                            .context(Io)?;
                    }
                    let _ = serde_json::to_writer(&mut self.state.buf, series)
                        .context(JsonEncodingFailed)?;
                }
            }
            // We can't encode sketches incrementally (yet), so we don't do any encoding here.  We
            // simply store it for later, and in `pre_finish`, any such pending metrics will be
            // encoded in a single operation.
            DatadogMetricsEndpoint::Sketches => {}
        }

        // If we actually encoded a metric, we try to see if our temporary buffer can be compressed
        // and added to the overall payload.  Otherwise, it means we're deferring the metric for
        // later encoding, so we store it off to the side.
        if !self.state.buf.is_empty() {
            // Compressing the temporary buffer would violate our payload size limits, so we give
            // the metric back to the caller.
            if !self.try_compress_buffer().context(Io)? {
                return Ok(Some(metric));
            }

            self.state.processed.push(metric);
        } else {
            self.state.pending.push(metric);
        }

        Ok(None)
    }

    fn generate_series_metrics(
        &mut self,
        metric: &Metric,
    ) -> Result<Vec<DatadogSeriesMetric>, EncoderError> {
        let namespaced_name = self.get_namespaced_name(metric);
        let ts = encode_timestamp(metric.timestamp());
        let tags = metric.tags().map(encode_tags);
        let interval = self
            .last_sent
            .map(|then| then.elapsed())
            .map(|d| d.as_secs().try_into().unwrap_or(i64::MAX));

        let results = match metric.value() {
            MetricValue::Counter { value } => vec![DatadogSeriesMetric {
                metric: namespaced_name,
                r#type: DatadogMetricType::Count,
                interval,
                points: vec![DatadogPoint(ts, *value)],
                tags,
            }],
            MetricValue::Set { values } => vec![DatadogSeriesMetric {
                metric: namespaced_name,
                r#type: DatadogMetricType::Gauge,
                interval: None,
                points: vec![DatadogPoint(ts, values.len() as f64)],
                tags,
            }],
            MetricValue::Gauge { value } => vec![DatadogSeriesMetric {
                metric: namespaced_name,
                r#type: DatadogMetricType::Gauge,
                interval: None,
                points: vec![DatadogPoint(ts, *value)],
                tags,
            }],
            MetricValue::AggregatedSummary {
                quantiles,
                count,
                sum,
            } => {
                let mut results = vec![
                    DatadogSeriesMetric {
                        metric: format!("{}.count", &namespaced_name),
                        r#type: DatadogMetricType::Rate,
                        interval,
                        points: vec![DatadogPoint(ts, f64::from(*count))],
                        tags: tags.clone(),
                    },
                    DatadogSeriesMetric {
                        metric: format!("{}.sum", &namespaced_name),
                        r#type: DatadogMetricType::Gauge,
                        interval: None,
                        points: vec![DatadogPoint(ts, *sum)],
                        tags: tags.clone(),
                    },
                ];

                for quantile in quantiles {
                    results.push(DatadogSeriesMetric {
                        metric: format!(
                            "{}.{}percentile",
                            &namespaced_name,
                            quantile.as_percentile()
                        ),
                        r#type: DatadogMetricType::Gauge,
                        interval: None,
                        points: vec![DatadogPoint(ts, quantile.value)],
                        tags: tags.clone(),
                    })
                }
                results
            }
            value => {
                return Err(EncoderError::InvalidMetric {
                    endpoint: self.endpoint,
                    metric_value: value.as_name(),
                })
            }
        };

        Ok(results)
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
        // fit within the compressed limit.  The worst case scenario should be when the input is
        // incompressible, so while not optimal, this also ensures we wouldn't exceed the compressed
        // limit.
        //
        // TODO: Could we track the ratio between uncompressed/compressed metrics as we write
        // them, in order to be able to estimate the size of a metric once written to the
        // compressor? This would mean that we would potentially ruin an entire batch if we wrote to the
        // compressor and our estimate was too low, which would make the type signature ugly and
        // also mean that the caller would have to track that fact to avoid hitting it again.
        //
        // Might be more easily achieved if we could write to the compressor knowing that it would
        // calculate the CRC at the very end, giving us a chance to back out a compressed write if
        // it would indeed overflow, but flate2 does not currently have a way to let us do that.
        //
        // Alternatively, we could store all of the `Metric` objects after we successfully encode
        // them.  By doing so, it would give us the ability to back out of an encode operation that
        // exceeds the compressed size limit by forcefully dropping the compressed-so-far buffer,
        // and reencoding/recompressing the events we successfully processed so far.  We could then
        // correctly return the current metric to the caller, signaling them to finish this encoder,
        // which would give them back a compressed payload identical to the one that existed before
        // they called `try_encode`.
        //
        // We eat a little bit of memory usage holding on to the metrics, but not a ton.  It's much
        // better than continually cloning the compression buffer in order to roll it back if we
        // exceed the compressed size limit.
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
    /// If the metric could not be encoded into this encoder due to hitting the limits on the sizer
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
            let _ = write_payload_header(self.endpoint, &mut self.state.writer).context(Io)?;
        }

        self.encode_single_metric(metric)
    }

    fn try_encode_pending(&mut self) -> Result<(), EncoderError> {
        // This function allows us to co-opt the "processed" metrics for another purpose: generating
        // sketch payloads.  Since there's no readily-available Protocol Buffer incremental encoder
        // that I can find, we can't truly emulate what the Datadog Agent does, which is encode
        // their sketch payloads one sketch at a time, similar to the actual logic of `try_encode`.
        //
        // Since we can't do that, we simply collect all the metrics given to us, without even
        // complaining that they don't fit, and we wait until `finish` is called to do the actual
        // encoding.
        //
        // By doing so, we aren't encoding incrementally, but we are able to figure out if the
        // sketches would overflow our limits, and if so, we pass back all of the metrics.  This
        // allows the caller to try and split the batch the split and try to run through the
        // encoding process again.

        // The Datadog Agent uses a particular Protocol Buffers library to incrementally encode the
        // DDSketch structures into a payload, similiar to how we incrementally encode the series
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

        // We consume all the pending metrics, since we need to add them to the "processed" vector
        // if we successfully encode them and can compress the payload.
        let pending = mem::replace(&mut self.state.pending, Vec::new());

        let mut sketches = Vec::new();
        for metric in &pending {
            match metric.value() {
                MetricValue::Sketch { sketch } => match sketch {
                    MetricSketch::AgentDDSketch(ddsketch) => {
                        // Don't encode any empty sketches.
                        if ddsketch.is_empty() {
                            trace!(message = "sketch was empty", name = metric.name());
                            continue;
                        }

                        trace!(message = "about to encode sketch", ?ddsketch);

                        let namespaced_name = self.get_namespaced_name(metric);
                        let ts = encode_timestamp(metric.timestamp());
                        let tags = metric.tags().map(encode_tags).unwrap_or_default();

                        let (bins, counts) = ddsketch.bin_map().into_parts();
                        let k = bins.into_iter().map(Into::into).collect();
                        let n = counts.into_iter().map(Into::into).collect();

                        let sketch = ddsketch_proto::sketch_payload::Sketch {
                            metric: namespaced_name,
                            tags,
                            host: "dummy value".to_string(),
                            distributions: Vec::new(),
                            dogsketches: vec![ddsketch_proto::sketch_payload::sketch::Dogsketch {
                                ts,
                                cnt: ddsketch.count() as i64,
                                min: ddsketch
                                    .min()
                                    .expect("min should be present for non-empty sketch"),
                                max: ddsketch
                                    .max()
                                    .expect("max should be present for non-empty sketch"),
                                avg: ddsketch
                                    .avg()
                                    .expect("avg should be present for non-empty sketch"),
                                sum: ddsketch
                                    .avg()
                                    .expect("avg should be present for non-empty sketch"),
                                k,
                                n,
                            }],
                        };

                        sketches.push(sketch);
                    }
                },
                value => {
                    return Err(EncoderError::InvalidMetric {
                        endpoint: self.endpoint,
                        metric_value: value.as_name(),
                    })
                }
            }
        }

        let sketch_payload = ddsketch_proto::SketchPayload {
            // The "common metadata" fields are things that only very loosely apply to Vector, or
            // are hard to characterize -- for example, what's the API key for a sketch that didn't originate
            // from the Datadog Agent? -- so we're just omitting it here in the hopes it doesn't
            // actually matter.
            metadata: None,
            sketches,
        };

        // Now try encoding this sketch payload, and then try to compress it.
        self.state.buf.clear();
        let _ = sketch_payload
            .encode(&mut self.state.buf)
            .context(ProtoEncodingFailed)?;

        if self.try_compress_buffer()? {
            self.state.processed.extend(pending);
            Ok(())
        } else {
            // The payload was too big overall, which we can't do anything about.  Up to the caller
            // now to split the batch or something else.
            Err(EncoderError::TooLarge {
                metrics: pending,
                // Hard-coded split code for now because we need to hoist up the logic for
                // calculating the recommended splits to an instance method or something.
                recommended_splits: 2,
            })
        }
    }

    pub fn finish(&mut self) -> Result<(Vec<u8>, Vec<Metric>), EncoderError> {
        // Try to encode any pending metrics we had stored up.
        let _ = self.try_encode_pending()?;

        // Write any payload footer necessary for the configured endpoint.
        let _ = write_payload_footer(self.endpoint, &mut self.state.writer).context(Io)?;

        // Consume the encoder state so we can do our final checks and return the necessary data.
        let state = self.reset_state();
        let payload = state.writer.finish().context(Io)?;
        let processed = state.processed;

        info!(message = "finished encoding/compression request",
            uncompressed_len = state.written, compressed_len = payload.len(),
            endpoint = ?self.endpoint, processed_len = processed.len());

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
        let target_splits = cmp::max(compressed_splits, uncompressed_splits);

        if target_splits == 0 {
            // No splits means our payload didn't exceed either of the limits.
            Ok((payload, processed))
        } else {
            Err(EncoderError::TooLarge {
                metrics: processed,
                recommended_splits: target_splits + 1,
            })
        }
    }
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

fn get_endpoint_payload_size_limits() -> (usize, usize) {
    // According to the datadog-agent code, sketches use the same payload size limits as series
    // data. We're just gonna go with that for now.
    let uncompressed_limit = MAXIMUM_SERIES_PAYLOAD_SIZE;
    let compressed_limit = MAXIMUM_SERIES_PAYLOAD_COMPRESSED_SIZE;

    // Estimate the potential overhead of the compression container itself.
    //
    // We use zlib, which uses Deflate under the hood.  zlib itself can take up to 6 bytes. For
    // Deflate, the output can only ever be marginally larger than the input as Deflate will resort
    // to storing uncompressed blocks if the compressed versions were somehow larger. Thus, we can
    // expect "an overhead of five bytes per 16 KB block (about 0.03%)"[1] in overhead.
    //
    // Thus, we take 1/32nd of the compressed limit (about 0.03%) and the zlib header length as our
    // estimated maximum overhead.
    //
    // [1] - https://www.zlib.net/zlib_tech.html
    let estimated_max_compression_overhead = 6 + (compressed_limit - 6) / 32;

    // We already know we'll have to write the header/footer for the series payload by hand
    // to allow encoding incrementally, so figure out the size of that so we can remove it.
    let header_len = SERIES_PAYLOAD_HEADER.len() + SERIES_PAYLOAD_FOOTER.len();

    // This is a sanity check to ensure that our chosen limits are reasonable.
    assert!(uncompressed_limit > header_len);
    assert!(compressed_limit > header_len + estimated_max_compression_overhead);

    // Adjust for the known/estimated sizes of headers, footers, compression container
    // overhead, etc.
    let uncompressed_limit = uncompressed_limit - header_len;
    let compressed_limit = compressed_limit - header_len - estimated_max_compression_overhead;

    (uncompressed_limit, compressed_limit)
}

fn write_payload_header(
    endpoint: DatadogMetricsEndpoint,
    writer: &mut dyn io::Write,
) -> io::Result<()> {
    match endpoint {
        DatadogMetricsEndpoint::Series => writer.write_all(SERIES_PAYLOAD_HEADER),
        _ => Ok(()),
    }
}

fn write_payload_delimiter(
    endpoint: DatadogMetricsEndpoint,
    writer: &mut dyn io::Write,
) -> io::Result<()> {
    match endpoint {
        DatadogMetricsEndpoint::Series => writer.write_all(SERIES_PAYLOAD_DELIMITER),
        _ => Ok(()),
    }
}

fn write_payload_footer(
    endpoint: DatadogMetricsEndpoint,
    writer: &mut dyn io::Write,
) -> io::Result<()> {
    match endpoint {
        DatadogMetricsEndpoint::Series => writer.write_all(SERIES_PAYLOAD_FOOTER),
        _ => Ok(()),
    }
}
