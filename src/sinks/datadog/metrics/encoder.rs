use std::{
    collections::BTreeMap,
    io::{self, Write},
};

use chrono::{DateTime, Utc};
use prost::Message;
use serde::Serialize;
use snafu::{ResultExt, Snafu};
use vector_core::event::{metric::MetricSketch, Metric, MetricValue};

use crate::sinks::util::{encode_namespace, encoding::as_tracked_write, Compressor};

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

    #[snafu(display("Finished payload exceeded the (un)compressed size limits"))]
    TooLarge { metrics: Vec<Metric> },
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

pub struct DatadogMetricsEncoder {
    endpoint: DatadogMetricsEndpoint,
    default_namespace: Option<String>,
    uncompressed_limit: Option<usize>,
    compressed_limit: Option<usize>,

    writer: Compressor,
    written: usize,
    buf: Vec<u8>,

    processed: Vec<Metric>,
}

impl DatadogMetricsEncoder {
    /// Creates a new `DatadogMetricsEncoder` for the given endpoint.
    ///
    /// Depending on the endpoint, different payload size limitations will be applied.
    pub fn new(
        endpoint: DatadogMetricsEndpoint,
        default_namespace: Option<String>,
    ) -> io::Result<Self> {
        // Calculate the payload size limits for the given endpoint.
        let (uncompressed_limit, compressed_limit) = get_endpoint_payload_size_limits(endpoint);

        // Create our compressor and get the header in place.  We use the "zlib default" compressor
        // because it's all Datadog supports, and adding it generically to `Compression` would make
        // things a little weird because of the conversion trait implementations that are also only
        // none vs gzip.
        let mut writer = Compressor::zlib_default();
        let _ = write_payload_header(endpoint, &mut writer)?;

        Ok(Self {
            endpoint,
            default_namespace,
            writer,
            written: 0,
            buf: Vec::new(),
            uncompressed_limit,
            compressed_limit,
            processed: Vec::new(),
        })
    }
}

impl DatadogMetricsEncoder {
    fn get_namespaced_name(&self, metric: &Metric) -> String {
        encode_namespace(
            metric
                .namespace()
                .or(self.default_namespace.as_ref().map(|s| s.as_str())),
            '.',
            metric.name(),
        )
    }

    fn encode_metric_for_endpoint(
        &mut self,
        metric: &Metric,
    ) -> Result<usize, EncoderError> {
        match self.endpoint {
            DatadogMetricsEndpoint::Series => self.encode_series_metric(metric),
            DatadogMetricsEndpoint::Distribution => self.encode_distribution_metric(metric),
            DatadogMetricsEndpoint::Sketch => self.encode_sketch_metric(metric),
        }
    }

    fn encode_series_metric(
        &mut self,
        metric: &Metric,
    ) -> Result<usize, EncoderError> {
        let namespaced_name = self.get_namespaced_name(metric);
        let ts = encode_timestamp(metric.timestamp());
        let tags = metric.tags().map(encode_tags);
        let series = match metric.value() {
            MetricValue::Counter { value } => DatadogSeriesMetric {
                metric: namespaced_name,
                r#type: DatadogMetricType::Count,
                // TODO: how tf do we shuttle the interval in here? need the actual batch
                // time out, like how long actually elapsed since the previous batch for this endpoint,
                // not the maximum allowed batch timeout value
                interval: None,
                points: vec![DatadogPoint(ts, *value)],
                tags,
            },
            MetricValue::Set { values } => DatadogSeriesMetric {
                metric: namespaced_name,
                r#type: DatadogMetricType::Gauge,
                interval: None,
                points: vec![DatadogPoint(ts, values.len() as f64)],
                tags,
            },
            MetricValue::Gauge { value } => DatadogSeriesMetric {
                metric: namespaced_name,
                r#type: DatadogMetricType::Gauge,
                interval: None,
                points: vec![DatadogPoint(ts, *value)],
                tags,
            },
            value => {
                return Err(EncoderError::InvalidMetric {
                    endpoint: self.endpoint,
                    metric_value: value.as_name(),
                })
            }
        };

        let result = as_tracked_write(&mut self.buf, &series, |writer, item| {
            serde_json::to_writer(writer, item)
        });
        result.map_err(Into::into).context(Io)
    }

    fn encode_distribution_metric(
        &self,
        _metric: &Metric,
    ) -> Result<usize, EncoderError> {
        todo!()
    }

    fn encode_sketch_metric(
        &self,
        _metric: &Metric,
    ) -> Result<usize, EncoderError> {
        // We don't write anything here because sketches are encoded in `DatadogMetricsEncoder::pre_finish`.
        Ok(0)
    }

    pub fn try_encode(&mut self, metric: Metric) -> Result<Option<Metric>, EncoderError> {
        // Start out by encoding the metric into our temporary buffer.  We additionally write a
        // payload delimiter before the metric if we've already written at least one metric so far.
        // This ensures that we don't leave a dangling delimiter if we had to back out a metric from
        // encoding.
        //
        // I also realize that we're grabbing the length of the temporary buffer even though we got
        // back the number of bytes written by the actual encode function, but this is purely to
        // ensure that the number we're considering is the true buffer length, rather than any
        // intermediate number that may have unintentionally been passed back.
        self.buf.clear();
        if !self.processed.is_empty() {
            let _ = write_payload_delimiter(self.endpoint, &mut self.buf).context(Io)?;
        }
        let _ = self.encode_metric_for_endpoint(&metric)?;
        let n = self.buf.len();

        // If we're over our uncompressed size limit with this metric, inform the caller.
        if let Some(uncompressed_limit) = self.uncompressed_limit {
            if self.written + n > uncompressed_limit {
                return Ok(Some(metric));
            }
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
        if let Some(compressed_limit) = self.compressed_limit {
            let compressed_len = self.writer.get_ref().len();
            if compressed_len + n > compressed_limit {
                return Ok(Some(metric));
            }
        }

        // We should be safe to write our holding buffer to the compressor and store the metric.
        let _ = self.writer.write_all(&self.buf).context(Io)?;
        self.written += n;
        self.processed.push(metric);

        Ok(None)
    }

    fn pre_finish(&mut self) -> Result<(), EncoderError> {
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
        
        // Only go through this if we're targeting the sketch endpoint.
        if !(matches!(self.endpoint, DatadogMetricsEndpoint::Sketch)) {
            return Ok(())
        }

        let mut sketches = Vec::new();
        for metric in &self.processed {
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

        // Now try encoding this sketch payload, and then write it to the compressor.
        self.buf.clear();
        let _ = sketch_payload
            .encode(&mut self.buf)
            .expect("encoding sketches into Vec<u8> should be infallible");

        self.writer.write_all(&self.buf).context(Io)
    }

    pub fn finish(mut self) -> Result<(Vec<u8>, Vec<Metric>), EncoderError> {
        let _ = self.pre_finish()?;

        let _ = write_payload_footer(self.endpoint, &mut self.writer).context(Io)?;
        let payload = self.writer.finish().context(Io)?;

        info!(message = "finished encoding/compression request",
            uncompressed_len = self.written, compressed_len = payload.len(),
            endpoint = ?self.endpoint, processed_len = self.processed.len());

        // A compressed limit is only set if we're actually compressing, so we check for that, and
        // then the uncompressed size, and if neither are set, we default to returning the payload.
        let within_limit = self
            .compressed_limit
            .or(self.uncompressed_limit)
            .map(|limit| payload.len() <= limit)
            .unwrap_or(true);

        if within_limit {
            Ok((payload, self.processed))
        } else {
            Err(EncoderError::TooLarge {
                metrics: self.processed,
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

fn get_endpoint_payload_size_limits(
    _endpoint: DatadogMetricsEndpoint,
) -> (Option<usize>, Option<usize>) {
    // Estimate the potential overhead of the compression container itself.
    //
    // TODO: We're estimating the expected size of the compressor overhead -- file header,
    // checksum, etc -- and computing our actual compressed limit from that.  This should be
    // reasonably accurate for gzip, but might not be for other compression algorithms.
    //
    // flate2 does not expose a way for us to get the exact numbers, but it would be nice if
    // it did.
    let estimated_compressed_header_len = 24;

    // According to the datadog-agent code, sketches use the same payload size limits as series
    // data. We're just gonna go with that for now.
    let uncompressed_limit = MAXIMUM_SERIES_PAYLOAD_SIZE;
    let compressed_limit = MAXIMUM_SERIES_PAYLOAD_COMPRESSED_SIZE;

    // We already know we'll have to write the header/footer for the series payload by hand
    // to allow encoding incrementally, so figure out the size of that so we can remove it.
    let header_len = SERIES_PAYLOAD_HEADER.len() + SERIES_PAYLOAD_FOOTER.len();

    // This is a sanity check to ensure that our chosen limits are reasonable.
    assert!(uncompressed_limit > header_len);
    assert!(compressed_limit > header_len + estimated_compressed_header_len);

    // Adjust for the known/estimated sizes of headers, footers, compression container
    // overhead, etc.
    let uncompressed_limit = uncompressed_limit - header_len;
    let compressed_limit = compressed_limit - header_len + estimated_compressed_header_len;

    (Some(uncompressed_limit), Some(compressed_limit))
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
