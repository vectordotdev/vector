use std::sync::Arc;

use bytes::Bytes;
use chrono::Utc;
use snafu::Snafu;
use vector_lib::{
    event::{EventFinalizers, Finalizable, Metric},
    request_metadata::RequestMetadata,
};

use super::{
    config::{
        DatadogMetricsEndpoint, DatadogMetricsEndpointConfiguration, SeriesApiVersion,
        SketchesApiVersion,
    },
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

/// Encoder dispatch: either V1/V2 incremental or V3 batch. Used uniformly for both
/// the series and sketches encoders — which variant is picked depends only on the
/// configured `SeriesApiVersion`, not on the endpoint.
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
    sketches_encoder: EncoderKind,
}

impl DatadogMetricsRequestBuilder {
    pub fn new(
        endpoint_configuration: DatadogMetricsEndpointConfiguration,
        default_namespace: Option<String>,
        series_api_version: SeriesApiVersion,
        sketches_api_version: SketchesApiVersion,
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

        // Independent of `series_api_version`: Datadog's intake gates V3 series and V3 sketches
        // separately, so the sketches wire format must be chosen by its own setting.
        let sketches_encoder = if sketches_api_version.is_v3_format() {
            EncoderKind::V3(Box::new(DatadogMetricsV3Encoder::new(
                DatadogMetricsEndpoint::Sketches,
                default_namespace,
            )))
        } else {
            EncoderKind::V1V2(Box::new(DatadogMetricsEncoder::new(
                DatadogMetricsEndpoint::Sketches,
                default_namespace,
            )))
        };

        Self {
            endpoint_configuration,
            series_encoder,
            sketches_encoder,
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

        let encoder = match endpoint {
            DatadogMetricsEndpoint::Series(_) => &mut self.series_encoder,
            DatadogMetricsEndpoint::Sketches => &mut self.sketches_encoder,
        };

        encode_batch(encoder, api_key, endpoint, metrics)
    }

    fn build_request(&mut self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (ddmetrics_metadata, request_metadata) = metadata;

        let uri = self
            .endpoint_configuration
            .get_uri_for_endpoint(ddmetrics_metadata.endpoint);

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

fn encode_batch<E: MetricsEncoder>(
    encoder: &mut E,
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
                    mut metrics,
                    recommended_splits,
                }) => {
                    // The encoder informed us that the resulting payload was too big, so we're
                    // being given a chance here to split it into smaller input batches in the
                    // hopes of generating a smaller payload that _isn't_ too big.
                    //
                    // The encoder instructs us on how many subchunks it thinks we need to split
                    // these metrics up into in order to successfully encode them without error,
                    // based on the resulting size of the previous attempt compared to the
                    // payload size limits.
                    //
                    // In order to avoid a pathological case from causing us to
                    // recursively/endlessly attempt encoding smaller and smaller batches, we
                    // only do this split/encode operation once.  If any of the chunks fail for
                    // any reason, we fail that chunk entirely.
                    //
                    // TODO: In the future, when we have a way to incrementally write out
                    // Protocol Buffers data, similar to how the Datadog Agent does it with
                    // `molecule`, we can wrap all of the sketch encoding into the same
                    // incremental encoding paradigm and avoid this.
                    //
                    // `recommended_splits` is derived from a *byte-size* ratio and is unbounded
                    // by the number of metrics actually in this batch: a single metric whose
                    // encoded size alone exceeds the limit (e.g. one very high-cardinality
                    // sketch) can report a `recommended_splits` far larger than `metrics.len()`.
                    // Without capping it, `stride = metrics.len() / recommended_splits`
                    // truncates to `0`, so every iteration of the loop below calls
                    // `metrics.split_off(split_idx)` with an unchanged `split_idx` — producing
                    // `recommended_splits - 1` *empty* chunks that each "succeed" as a
                    // zero-metric request, while the real oversized chunk is pushed unchanged
                    // at the end and fails again. Capping to `metrics.len()` guarantees each
                    // chunk gets at least one metric; when there's only one metric to begin
                    // with, the cap collapses the loop entirely and that single unsplittable
                    // metric is sent through `encode_chunk` on its own, where it fails cleanly
                    // as `FailedToSplit` instead of spawning empty requests first.
                    let recommended_splits = recommended_splits.min(metrics.len());
                    let mut split_idx = metrics.len();
                    let stride = split_idx / recommended_splits;

                    let mut remaining_splits = recommended_splits;
                    while remaining_splits > 1 {
                        split_idx -= stride;
                        let chunk = metrics.split_off(split_idx);
                        results.push(encode_chunk(encoder, api_key.clone(), endpoint, chunk));
                        remaining_splits -= 1;
                    }
                    results.push(encode_chunk(encoder, api_key.clone(), endpoint, metrics));
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

/// Encodes one chunk in a single shot, treating any error as unrecoverable. Used for
/// split-retry after a `FinishError::TooLarge`.
fn encode_chunk<E: MetricsEncoder>(
    encoder: &mut E,
    api_key: Option<Arc<str>>,
    endpoint: DatadogMetricsEndpoint,
    metrics: Vec<Metric>,
) -> Result<((DDMetricsMetadata, RequestMetadata), Bytes), RequestBuilderError> {
    let metrics_len = metrics.len();

    metrics
        .into_iter()
        .try_fold(0, |n, metric| match encoder.try_encode(metric) {
            Ok(None) => Ok(n + 1),
            _ => Err(RequestBuilderError::FailedToSplit {
                dropped_events: metrics_len as u64,
            }),
        })?;

    encoder
        .finish()
        .map(|(encode_result, mut processed)| {
            let finalizers = processed.take_finalizers();
            let ddmetrics_metadata = DDMetricsMetadata {
                api_key,
                endpoint,
                finalizers,
            };

            let request_metadata =
                RequestMetadataBuilder::from_events(&processed).build(&encode_result);

            (
                (ddmetrics_metadata, request_metadata),
                encode_result.into_payload(),
            )
        })
        .map_err(|_| RequestBuilderError::FailedToSplit {
            dropped_events: metrics_len as u64,
        })
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

    /// Test double for [`MetricsEncoder`] whose `finish()` reports `TooLarge` with an
    /// arbitrary, caller-chosen `recommended_splits` for any non-empty batch — regardless of
    /// how many metrics are actually pending. This lets us exercise `encode_batch`'s split
    /// arithmetic in isolation, including the case a real encoder hits when a *single* metric
    /// (e.g. one huge sketch) alone exceeds the size limit: `recommended_splits` is derived
    /// from a byte-size ratio and can be larger than the metric count.
    struct AlwaysTooLargeEncoder {
        pending: Vec<Metric>,
        recommended_splits: usize,
    }

    impl MetricsEncoder for AlwaysTooLargeEncoder {
        fn try_encode(&mut self, metric: Metric) -> Result<Option<Metric>, EncoderError> {
            self.pending.push(metric);
            Ok(None)
        }

        fn finish(&mut self) -> Result<(EncodeResult<Bytes>, Vec<Metric>), FinishError> {
            let metrics = std::mem::take(&mut self.pending);
            if metrics.is_empty() {
                // Matches every real encoder's behavior: finishing an empty batch always
                // succeeds trivially, producing an empty payload.
                return Ok((
                    EncodeResult::compressed(Bytes::new(), 0, GroupedCountByteSize::new_untagged()),
                    Vec::new(),
                ));
            }
            Err(FinishError::TooLarge {
                metrics,
                recommended_splits: self.recommended_splits,
            })
        }
    }

    /// A single metric that's too large on its own can report a `recommended_splits` far
    /// larger than the metric count (it's derived from a byte-size ratio, not from counting
    /// metrics). Splitting must never emit more chunks than there are metrics to put in them:
    /// this metric must come out as exactly one failed result, not four phantom "successful"
    /// empty requests followed by one failure.
    #[test]
    fn too_large_split_never_exceeds_the_metric_count() {
        let mut encoder = AlwaysTooLargeEncoder {
            pending: Vec::new(),
            recommended_splits: 5,
        };

        let results = encode_batch(
            &mut encoder,
            None,
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            vec![counter_metric()],
        );

        assert_eq!(
            results.len(),
            1,
            "an unsplittable single metric must yield exactly one result, not {} \
             (a fixed `recommended_splits` split count would otherwise emit \
             `recommended_splits - 1` empty successes before the real failure)",
            results.len()
        );
        assert!(
            results[0].is_err(),
            "the single oversized metric must be reported as failed, not silently dropped \
             behind a successful empty payload"
        );
    }

    /// With more metrics than the recommended split count, splitting proceeds exactly as
    /// before: each of the `recommended_splits` chunks gets a non-empty share of the metrics.
    #[test]
    fn too_large_split_with_enough_metrics_produces_no_empty_chunks() {
        let mut encoder = AlwaysTooLargeEncoder {
            pending: Vec::new(),
            recommended_splits: 3,
        };

        let metrics: Vec<Metric> = (0..3).map(|_| counter_metric()).collect();
        let results = encode_batch(
            &mut encoder,
            None,
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            metrics,
        );

        assert_eq!(
            results.len(),
            3,
            "3 metrics split 3 ways must produce exactly 3 results, one per metric"
        );
        assert!(
            results.iter().all(Result::is_err),
            "this encoder always reports TooLarge for non-empty input, so every \
             single-metric chunk must fail as unsplittable, not succeed"
        );
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
