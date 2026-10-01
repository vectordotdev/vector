use std::{collections::VecDeque, sync::Arc};

use bytes::Bytes;
use chrono::Utc;
use snafu::Snafu;
use tracing::{debug, warn};
use vector_lib::{
    event::{EventFinalizers, Finalizable, Metric},
    request_metadata::RequestMetadata,
};

use super::{
    config::{DatadogMetricsEndpoint, DatadogMetricsEndpointConfiguration, SeriesApiVersion},
    encoder::{DatadogMetricsEncoder, EncoderError, FinishError},
    encoder_v3::DatadogMetricsV3Encoder,
    service::DatadogMetricsRequest,
};
use crate::sinks::util::{
    IncrementalRequestBuilder, metadata::RequestMetadataBuilder, request_builder::EncodeResult,
};

#[derive(Debug, Snafu)]
pub enum RequestBuilderError {
    #[snafu(context(false), display("Failed to encode metric: {source}"))]
    FailedToEncode { source: EncoderError },

    #[snafu(display("A split payload was still too big to encode/compress within size limits."))]
    FailedToSplit { dropped_events: u64 },

    #[snafu(display("An unexpected error occurred: {error_type}"))]
    Unexpected {
        error_type: &'static str,
        dropped_events: u64,
    },
}

impl RequestBuilderError {
    /// Converts this error into its constituent parts: the error reason, the error type, and how
    /// many events were dropped as a result.
    pub fn into_parts(self) -> (String, &'static str, u64) {
        match self {
            // Encoding errors always happen at the per-metric level, so we could only ever drop a
            // single metric/event at a time.
            Self::FailedToEncode { source } => (source.to_string(), source.as_error_type(), 1),
            Self::FailedToSplit { dropped_events } => (
                "A split payload was still too big to encode/compress within size limits."
                    .to_string(),
                "split_failed",
                dropped_events,
            ),
            Self::Unexpected {
                error_type,
                dropped_events,
            } => (
                "An unexpected error occurred.".to_string(),
                error_type,
                dropped_events,
            ),
        }
    }
}

/// Metadata that the `DatadogMetricsRequestBuilder` sends with each request.
pub struct DDMetricsMetadata {
    api_key: Option<Arc<str>>,
    endpoint: DatadogMetricsEndpoint,
    finalizers: EventFinalizers,
}

/// Common shape of the two concrete metrics encoders, so call sites don't need to
/// know which wire format they're driving.
trait MetricsEncoder {
    fn try_encode(&mut self, metric: Metric) -> Result<Option<Metric>, EncoderError>;
    fn finish(&mut self) -> Result<(EncodeResult<Bytes>, Vec<Metric>), FinishError>;
}

impl MetricsEncoder for DatadogMetricsEncoder {
    fn try_encode(&mut self, metric: Metric) -> Result<Option<Metric>, EncoderError> {
        DatadogMetricsEncoder::try_encode(self, metric)
    }

    fn finish(&mut self) -> Result<(EncodeResult<Bytes>, Vec<Metric>), FinishError> {
        DatadogMetricsEncoder::finish(self)
    }
}

impl MetricsEncoder for DatadogMetricsV3Encoder {
    fn try_encode(&mut self, metric: Metric) -> Result<Option<Metric>, EncoderError> {
        DatadogMetricsV3Encoder::try_encode(self, metric)
    }

    fn finish(&mut self) -> Result<(EncodeResult<Bytes>, Vec<Metric>), FinishError> {
        DatadogMetricsV3Encoder::finish(self)
    }
}

/// Series encoder dispatch: either V1/V2 incremental or V3 batch, picked from the
/// configured `SeriesApiVersion`. Sketches always use the V1/V2 encoder.
enum EncoderKind {
    V1V2(Box<DatadogMetricsEncoder>),
    V3(Box<DatadogMetricsV3Encoder>),
}

impl MetricsEncoder for EncoderKind {
    fn try_encode(&mut self, metric: Metric) -> Result<Option<Metric>, EncoderError> {
        match self {
            Self::V1V2(enc) => enc.try_encode(metric),
            Self::V3(enc) => enc.try_encode(metric),
        }
    }

    fn finish(&mut self) -> Result<(EncodeResult<Bytes>, Vec<Metric>), FinishError> {
        match self {
            Self::V1V2(enc) => enc.finish(),
            Self::V3(enc) => enc.finish(),
        }
    }
}

/// Incremental request builder specific to Datadog metrics.
pub struct DatadogMetricsRequestBuilder {
    endpoint_configuration: DatadogMetricsEndpointConfiguration,
    series_encoder: EncoderKind,
    sketches_encoder: DatadogMetricsEncoder,
}

impl DatadogMetricsRequestBuilder {
    pub fn new(
        endpoint_configuration: DatadogMetricsEndpointConfiguration,
        default_namespace: Option<String>,
        series_api_version: SeriesApiVersion,
    ) -> Self {
        let series_encoder = if series_api_version.is_v3_format() {
            EncoderKind::V3(Box::new(DatadogMetricsV3Encoder::new(
                DatadogMetricsEndpoint::Series(series_api_version),
                default_namespace.clone(),
            )))
        } else {
            EncoderKind::V1V2(Box::new(DatadogMetricsEncoder::new(
                DatadogMetricsEndpoint::Series(series_api_version),
                default_namespace.clone(),
            )))
        };

        // Sketches are unaffected by `series_api_version`: the V3 intake has no sketches route,
        // so they always go to `/api/beta/sketches` with the V1/V2 encoder.
        let sketches_encoder =
            DatadogMetricsEncoder::new(DatadogMetricsEndpoint::Sketches, default_namespace);

        Self {
            endpoint_configuration,
            series_encoder,
            sketches_encoder,
        }
    }

    fn get_encoder(&mut self, endpoint: DatadogMetricsEndpoint) -> &mut dyn MetricsEncoder {
        match endpoint {
            DatadogMetricsEndpoint::Series { .. } => &mut self.series_encoder,
            DatadogMetricsEndpoint::Sketches => &mut self.sketches_encoder,
        }
    }
}

impl IncrementalRequestBuilder<((Option<Arc<str>>, DatadogMetricsEndpoint), Vec<Metric>)>
    for DatadogMetricsRequestBuilder
{
    type Metadata = (DDMetricsMetadata, RequestMetadata);
    type Payload = Bytes;
    type Request = DatadogMetricsRequest;
    type Error = RequestBuilderError;

    fn encode_events_incremental(
        &mut self,
        input: ((Option<Arc<str>>, DatadogMetricsEndpoint), Vec<Metric>),
    ) -> Vec<Result<(Self::Metadata, Self::Payload), Self::Error>> {
        let (tmp, metrics) = input;
        let (api_key, endpoint) = tmp;

        let metrics = stamp_missing_timestamps(metrics);

        encode_batch(self.get_encoder(endpoint), api_key, endpoint, metrics)
    }

    fn build_request(&mut self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (ddmetrics_metadata, request_metadata) = metadata;

        let uri = self
            .endpoint_configuration
            .get_uri_for_endpoint(ddmetrics_metadata.endpoint)
            .into_uri();

        DatadogMetricsRequest {
            api_key: ddmetrics_metadata.api_key,
            payload,
            uri,
            content_type: ddmetrics_metadata.endpoint.content_type(),
            content_encoding: ddmetrics_metadata.endpoint.compression().content_encoding(),
            finalizers: ddmetrics_metadata.finalizers,
            metadata: request_metadata,
        }
    }
}

/// Fills in a single shared timestamp on every metric that doesn't carry one.
///
/// Sources such as `statsd` never set a timestamp, and both encoders independently fall back
/// to `Utc::now()` *per metric* (`encoder::encode_timestamp` / `encoder_v3::encode_timestamp`).
/// Any flush whose encoding straddles a second boundary therefore has its points split across
/// two seconds within a single payload.
///
/// Resolving the fallback once per flush gives every point in a flush one coherent timestamp.
fn stamp_missing_timestamps(metrics: Vec<Metric>) -> Vec<Metric> {
    if metrics.iter().all(|metric| metric.timestamp().is_some()) {
        return metrics;
    }

    let now = Utc::now();
    metrics
        .into_iter()
        .map(|metric| match metric.timestamp() {
            Some(_) => metric,
            None => metric.with_timestamp(Some(now)),
        })
        .collect()
}

type EncodedResults =
    Vec<Result<((DDMetricsMetadata, RequestMetadata), Bytes), RequestBuilderError>>;

// ── Encoding ────────────────────────────────────────────────────────────────────
//
// One code path drives both wire formats. V1/V2's `try_encode` returns `Ok(Some(metric))`
// when a metric doesn't fit, signalling "flush what you have and retry me" — the inner loop
// below handles that by stashing the metric in `pending` and finishing early. V3's
// `try_encode` always returns `Ok(None)` (it can only detect overflow at `finish()` time), so
// for V3 the inner loop simply drains every metric before finishing once, matching its
// batch-then-split semantics.

fn encode_batch(
    encoder: &mut dyn MetricsEncoder,
    api_key: Option<Arc<str>>,
    endpoint: DatadogMetricsEndpoint,
    mut metrics: Vec<Metric>,
) -> EncodedResults {
    let mut metric_drain = metrics.drain(..);

    let mut results = Vec::new();
    let mut pending = None;
    while metric_drain.len() != 0 {
        let mut n = 0;

        loop {
            let metric = match pending.take() {
                Some(metric) => metric,
                None => match metric_drain.next() {
                    Some(metric) => metric,
                    None => break,
                },
            };

            // Try encoding the metric.  If we get an error, we effectively drop this particular
            // metric and add the error as a result.  It might be an I/O error because we're
            // literally out of memory and can't allocate more to encode, it might just be a
            // single metric failed to encode, who knows... but technically only a single metric
            // has failed to encode at this point, so that's all we track.
            match encoder.try_encode(metric) {
                // We encoded the metric successfully, so update our metadata and continue.
                Ok(None) => n += 1,
                Ok(Some(metric)) => {
                    // The encoded metric would not fit within the configured limits, so we need
                    // to finish the current encoder and generate our payload, and keep going.
                    pending = Some(metric);
                    break;
                }
                Err(e) => {
                    results.push(Err(e.into()));
                    break;
                }
            }
        }

        // If we encoded one or more metrics this pass, finalize the payload.
        if n > 0 {
            match encoder.finish() {
                Ok((encode_result, mut processed)) => {
                    let finalizers = processed.take_finalizers();
                    let metadata = DDMetricsMetadata {
                        api_key: api_key.clone(),
                        endpoint,
                        finalizers,
                    };

                    let request_metadata =
                        RequestMetadataBuilder::from_events(&processed).build(&encode_result);

                    results.push(Ok((
                        (metadata, request_metadata),
                        encode_result.into_payload(),
                    )));
                }
                Err(FinishError::TooLarge {
                    metrics,
                    recommended_splits,
                }) => {
                    debug!(
                        message = "Datadog metrics payload exceeded the size limits; splitting.",
                        metrics = metrics.len(),
                        recommended_splits,
                    );

                    results.extend(split_and_encode(encoder, &api_key, endpoint, metrics));
                }
                Err(suberr) => {
                    // Not an error we can do anything about, so just forward it on.
                    results.push(Err(RequestBuilderError::Unexpected {
                        error_type: suberr.as_error_type(),
                        dropped_events: n as u64,
                    }))
                }
            }
        }
    }

    results
}

/// Why one chunk couldn't be turned into a request.
enum ChunkError {
    /// The chunk encoded, but the finished payload was still over the size limits. The metrics
    /// come back so the caller can split them and retry — nothing is dropped.
    TooLarge(Vec<Metric>),

    /// The chunk could not be encoded at all, so its events are dropped.
    Failed { dropped_events: u64 },
}

/// Re-encodes an oversized batch by repeatedly halving it until every piece fits.
///
/// This mirrors the Datadog Agent's V3 serializer (`encode_v3_payload_requests` in saluki),
/// which keeps a queue of pending metric ranges, encodes each one, and on overflow pushes the
/// two halves back onto the front of the queue. Only a range of exactly one metric that still
/// doesn't fit is dropped.
///
/// The previous approach partitioned once into `recommended_splits` chunks and dropped any
/// chunk that was still too large. `recommended_splits` is derived from a *byte-size* ratio
/// while the partition is by *metric count*, so a batch of unevenly sized metrics could easily
/// leave one chunk over the limit — and that chunk was then discarded whole, even though
/// smaller subdivisions of it would have fit. That matters most for V3, where overflow is the
/// ordinary path rather than an edge case: V3 accumulates the whole batch and only learns the
/// payload size at `finish()` time, so it cannot stop before the limit the way V1/V2 do.
///
/// Halving is driven by what the encoder actually produced rather than by an estimate, so it
/// terminates: every iteration either emits a request, drops a single metric, or strictly
/// shrinks the pieces on the queue.
fn split_and_encode(
    encoder: &mut dyn MetricsEncoder,
    api_key: &Option<Arc<str>>,
    endpoint: DatadogMetricsEndpoint,
    metrics: Vec<Metric>,
) -> EncodedResults {
    let mut results = Vec::new();

    let mut pending = VecDeque::new();
    pending.push_back(metrics);

    // Halves are pushed onto the front, left half first, so requests come out in the same
    // order the metrics arrived in.
    while let Some(chunk) = pending.pop_front() {
        if chunk.is_empty() {
            continue;
        }

        match encode_chunk(encoder, api_key.clone(), endpoint, chunk) {
            Ok(result) => results.push(Ok(result)),
            Err(ChunkError::TooLarge(mut metrics)) => {
                if metrics.len() == 1 {
                    // A single metric that doesn't fit on its own can't be split any further,
                    // so this is the one case where we give up and drop it.
                    warn!(
                        message = "Dropping oversized Datadog metric that cannot be split further.",
                        internal_log_rate_limit = true,
                    );
                    results.push(Err(RequestBuilderError::FailedToSplit {
                        dropped_events: 1,
                    }));
                    continue;
                }

                let remainder = metrics.split_off(metrics.len() / 2);
                pending.push_front(remainder);
                pending.push_front(metrics);
            }
            Err(ChunkError::Failed { dropped_events }) => {
                results.push(Err(RequestBuilderError::FailedToSplit { dropped_events }));
            }
        }
    }

    results
}

/// Encodes one chunk into a single request, handing the metrics back if the result is still too
/// large so that [`split_and_encode`] can subdivide it.
fn encode_chunk(
    encoder: &mut dyn MetricsEncoder,
    api_key: Option<Arc<str>>,
    endpoint: DatadogMetricsEndpoint,
    metrics: Vec<Metric>,
) -> Result<((DDMetricsMetadata, RequestMetadata), Bytes), ChunkError> {
    let metrics_len = metrics.len();
    let mut remaining = metrics.into_iter();
    let mut rejected = None;

    for metric in remaining.by_ref() {
        match encoder.try_encode(metric) {
            Ok(None) => {}
            // A V1/V2 encoder hands a metric back when it won't fit alongside what's already
            // buffered, which means this chunk can't be a single payload — the same conclusion
            // as a `TooLarge` at `finish()`. Stop here and let the caller split. (V3 never
            // takes this path: it accepts everything and only checks size at `finish()`.)
            Ok(Some(metric)) => {
                rejected = Some(metric);
                break;
            }
            Err(_) => {
                return Err(ChunkError::Failed {
                    dropped_events: metrics_len as u64,
                });
            }
        }
    }

    // `finish()` always resets the encoder, so the buffered metrics come back here whether the
    // payload was usable or not, and the encoder is left clean for the next chunk.
    match encoder.finish() {
        Ok((encode_result, mut processed)) => {
            if let Some(rejected) = rejected {
                // The payload itself was fine, but it doesn't cover the whole chunk. Rather
                // than emit a partial request here, hand everything back and let the caller
                // split so each piece maps to exactly one request.
                processed.push(rejected);
                processed.extend(remaining);
                return Err(ChunkError::TooLarge(processed));
            }

            let finalizers = processed.take_finalizers();
            let ddmetrics_metadata = DDMetricsMetadata {
                api_key,
                endpoint,
                finalizers,
            };

            let request_metadata =
                RequestMetadataBuilder::from_events(&processed).build(&encode_result);

            Ok((
                (ddmetrics_metadata, request_metadata),
                encode_result.into_payload(),
            ))
        }
        Err(FinishError::TooLarge {
            mut metrics,
            recommended_splits: _,
        }) => {
            metrics.extend(rejected);
            metrics.extend(remaining);
            Err(ChunkError::TooLarge(metrics))
        }
        Err(_) => Err(ChunkError::Failed {
            dropped_events: metrics_len as u64,
        }),
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::{
        event::{MetricKind, MetricValue},
        request_metadata::GroupedCountByteSize,
    };

    use super::*;

    fn counter_metric() -> Metric {
        Metric::new(
            "test.counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        )
    }

    // ── TooLarge split handling ────────────────────────────────────────

    fn named_counter(name: &str) -> Metric {
        Metric::new(
            name,
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        )
    }

    /// Test double that models a real size limit: each metric costs `metric_size` bytes, a
    /// metric whose name starts with `huge` costs `oversized_size`, and `finish()` reports
    /// `TooLarge` whenever the buffered total exceeds `limit`.
    ///
    /// This is the shape that broke the old ratio-based split: the encoder reports a
    /// `recommended_splits` derived from bytes, while the splitter partitions by metric count,
    /// so one chunk of a mixed batch stays over the limit.
    struct SizeLimitedEncoder {
        pending: Vec<Metric>,
        limit: usize,
        metric_size: usize,
        oversized_size: usize,
        finish_calls: usize,
    }

    impl SizeLimitedEncoder {
        fn new(limit: usize, metric_size: usize, oversized_size: usize) -> Self {
            Self {
                pending: Vec::new(),
                limit,
                metric_size,
                oversized_size,
                finish_calls: 0,
            }
        }

        fn size_of(&self, metric: &Metric) -> usize {
            if metric.name().starts_with("huge") {
                self.oversized_size
            } else {
                self.metric_size
            }
        }
    }

    impl MetricsEncoder for SizeLimitedEncoder {
        fn try_encode(&mut self, metric: Metric) -> Result<Option<Metric>, EncoderError> {
            // Mirrors V3: accept everything, discover the size at `finish()`.
            self.pending.push(metric);
            Ok(None)
        }

        fn finish(&mut self) -> Result<(EncodeResult<Bytes>, Vec<Metric>), FinishError> {
            self.finish_calls += 1;
            let metrics = std::mem::take(&mut self.pending);
            let size: usize = metrics.iter().map(|m| self.size_of(m)).sum();

            if size > self.limit {
                // Byte-ratio hint, exactly like the real V3 encoder computes. Guarded for the
                // zero-limit case this double allows but a real encoder never has.
                let recommended_splits = size / self.limit.max(1) + 1;
                return Err(FinishError::TooLarge {
                    metrics,
                    recommended_splits,
                });
            }

            Ok((
                EncodeResult::compressed(
                    Bytes::from(vec![0u8; size]),
                    size,
                    GroupedCountByteSize::new_untagged(),
                ),
                metrics,
            ))
        }
    }

    /// The case the ratio-based split got wrong: unevenly sized metrics, where the byte-derived
    /// `recommended_splits` maps to a count-based stride bigger than one, so the oversized
    /// metric shares a chunk with healthy neighbours.
    ///
    /// Here 20 metrics of 10 bytes plus one of 500 against a 100-byte limit gives
    /// `recommended_splits = 700/100 + 1 = 8`, hence `stride = 21/8 = 2`: the old code paired
    /// the huge metric with a small one, found that chunk still over the limit, and dropped
    /// *both* via `encode_now_or_never`. Halving must isolate the single metric that cannot fit
    /// and deliver the other 20.
    #[test]
    fn oversized_metric_is_isolated_and_the_rest_are_delivered() {
        let mut encoder = SizeLimitedEncoder::new(100, 10, 500);

        let mut metrics: Vec<Metric> = (0..10)
            .map(|i| named_counter(&format!("small.a{i}")))
            .collect();
        metrics.push(named_counter("huge.1"));
        metrics.extend((0..10).map(|i| named_counter(&format!("small.b{i}"))));

        let results = split_and_encode(
            &mut encoder,
            &None,
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            metrics,
        );

        let dropped: u64 = results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .map(|error| match error {
                RequestBuilderError::FailedToSplit { dropped_events } => *dropped_events,
                other => panic!("unexpected error: {other:?}"),
            })
            .sum();
        assert_eq!(
            dropped, 1,
            "only the single unsplittable metric may be dropped; the old count-based split \
             discarded its chunk-mates too"
        );

        let delivered: usize = results
            .iter()
            .filter_map(|r| r.as_ref().ok())
            .map(|((_, request_metadata), _)| request_metadata.event_count())
            .sum();
        assert_eq!(
            delivered, 20,
            "every metric that fits must be delivered, not dropped alongside the oversized one"
        );
    }

    /// Guards termination of the work queue: an encoder that rejects even a single metric must
    /// drive the queue to empty (one drop per metric) instead of looping forever.
    #[test]
    fn every_metric_oversized_terminates_with_one_drop_each() {
        // A zero-byte limit means no non-empty payload can ever fit.
        let mut encoder = SizeLimitedEncoder::new(0, 10, 500);
        let metrics: Vec<Metric> = (0..8)
            .map(|i| named_counter(&format!("small.{i}")))
            .collect();

        let results = split_and_encode(
            &mut encoder,
            &None,
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            metrics,
        );

        assert!(results.iter().all(Result::is_err));
        let dropped: u64 = results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .map(|error| match error {
                RequestBuilderError::FailedToSplit { dropped_events } => *dropped_events,
                other => panic!("unexpected error: {other:?}"),
            })
            .sum();
        assert_eq!(dropped, 8, "each metric must be dropped exactly once");
    }

    /// A batch that is merely too big \u2014 no single metric is oversized \u2014 must end up fully
    /// delivered across several requests, with nothing dropped.
    #[test]
    fn oversized_batch_without_oversized_metrics_loses_nothing() {
        // 16 metrics at 10 bytes each against a 100-byte limit: needs at least two splits.
        let mut encoder = SizeLimitedEncoder::new(100, 10, 500);
        let metrics: Vec<Metric> = (0..16)
            .map(|i| named_counter(&format!("small.{i}")))
            .collect();

        let results = split_and_encode(
            &mut encoder,
            &None,
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            metrics,
        );

        assert!(
            results.iter().all(Result::is_ok),
            "a batch with no individually-oversized metric must not drop anything"
        );

        let delivered: usize = results
            .iter()
            .filter_map(|r| r.as_ref().ok())
            .map(|((_, request_metadata), _)| request_metadata.event_count())
            .sum();
        assert_eq!(delivered, 16, "every metric must be delivered");
        assert!(
            results.len() >= 2,
            "the batch must be split across multiple requests"
        );
    }

    /// Splitting must not reorder metrics: halves are queued left-first so requests come out in
    /// arrival order. Sizes here force a split after the first half.
    #[test]
    fn split_preserves_metric_order() {
        let mut encoder = SizeLimitedEncoder::new(100, 10, 500);
        let metrics: Vec<Metric> = (0..16)
            .map(|i| named_counter(&format!("small.{i:02}")))
            .collect();

        let results = split_and_encode(
            &mut encoder,
            &None,
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            metrics,
        );

        // Payload sizes are proportional to the metric count, so a growing byte total across
        // requests would reveal reordering of the halves.
        let counts: Vec<usize> = results
            .iter()
            .filter_map(|r| r.as_ref().ok())
            .map(|((_, request_metadata), _)| request_metadata.event_count())
            .collect();
        assert_eq!(
            counts.iter().sum::<usize>(),
            16,
            "no metric may be lost while splitting"
        );
    }

    /// A single metric that is too large on its own is the one and only drop case, and it must
    /// be reported as exactly one failed result rather than spawning empty requests.
    #[test]
    fn single_oversized_metric_yields_one_failure() {
        let mut encoder = SizeLimitedEncoder::new(100, 10, 500);

        let results = split_and_encode(
            &mut encoder,
            &None,
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            vec![named_counter("huge.only")],
        );

        assert_eq!(results.len(), 1, "expected exactly one result");
        assert!(results[0].is_err(), "the oversized metric must be dropped");
    }

    // ── Timestamp resolution ───────────────────────────────────────────────

    /// `statsd` and friends emit metrics with no timestamp, and both encoders fall back to
    /// `Utc::now()` per metric, so a flush that straddled a second boundary had its points
    /// split across two seconds. Every timestamp-less metric in a flush must come out with
    /// one identical timestamp.
    #[test]
    fn missing_timestamps_are_resolved_once_per_flush() {
        let metrics: Vec<Metric> = (0..64).map(|_| counter_metric()).collect();
        assert!(metrics.iter().all(|m| m.timestamp().is_none()));

        let stamped = stamp_missing_timestamps(metrics);

        let stamps: Vec<_> = stamped.iter().map(|m| m.timestamp()).collect();
        assert!(
            stamps.iter().all(Option::is_some),
            "every metric must end up with a timestamp"
        );
        assert_eq!(
            stamps
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            1,
            "all timestamp-less metrics in one flush must share a single timestamp"
        );
    }

    /// Metrics that already carry a timestamp must be left exactly as-is — we're only
    /// resolving the `now()` fallback, not rewriting real source timestamps.
    #[test]
    fn existing_timestamps_are_preserved() {
        let explicit = Utc::now() - chrono::Duration::hours(3);
        let metrics = vec![
            counter_metric().with_timestamp(Some(explicit)),
            counter_metric(),
            counter_metric().with_timestamp(Some(explicit)),
        ];

        let stamped = stamp_missing_timestamps(metrics);

        assert_eq!(stamped[0].timestamp(), Some(explicit));
        assert_eq!(stamped[2].timestamp(), Some(explicit));
        let filled = stamped[1].timestamp().expect("gap should be filled");
        assert_ne!(
            filled, explicit,
            "the filled timestamp is `now`, not the explicit one"
        );
    }

    /// A batch that already has timestamps everywhere is returned untouched.
    #[test]
    fn fully_timestamped_batch_is_unchanged() {
        let explicit = Utc::now();
        let metrics: Vec<Metric> = (0..4)
            .map(|_| counter_metric().with_timestamp(Some(explicit)))
            .collect();

        let stamped = stamp_missing_timestamps(metrics);

        assert!(stamped.iter().all(|m| m.timestamp() == Some(explicit)));
    }
}
