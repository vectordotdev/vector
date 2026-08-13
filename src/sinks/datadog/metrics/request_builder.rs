use std::{num::NonZeroU64, sync::Arc};

use bytes::Bytes;
use chrono::Utc;
use http::Uri;
use snafu::Snafu;
use uuid::Uuid;
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
    /// Shared transaction ID linking the V2 and V3 shadow payload from the same flush.
    /// None on non-shadow flushes.
    batch_id: Option<Arc<str>>,
    /// 0-based index within this flush (for split payloads). Stamped after encoding.
    batch_seq: usize,
    /// Total requests produced by this flush. Stamped after encoding.
    batch_len: usize,
    /// Overrides the URI from `endpoint_configuration`. Used for shadow V3 requests
    /// that target a different path than the primary encoder.
    target_uri: Option<Uri>,
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

/// Shadow write configuration passed from `DatadogMetricsConfig::build_sink`.
///
/// Series only. Sketches are deliberately never dual-written: the V3 sketches intake routes
/// don't exist (they 404, and a 404 maps to a *retriable* `ClientError`, so a sketches
/// shadow retried forever without ever delivering).
pub struct ShadowBuilderConfig {
    /// The URI for the V3 shadow series endpoint (e.g. `/api/intake/metrics/v3/series`).
    pub series_uri: Uri,
    /// The `SeriesApiVersion` variant matching the shadow series endpoint.
    /// Used to set the correct payload limits and compression on the shadow encoder.
    pub series_api_version: SeriesApiVersion,
    /// Default metric namespace for the shadow encoder.
    pub default_namespace: Option<String>,
    /// Send a V3 shadow once per this many legacy (V1/V2) series flushes.
    pub shadow_every: NonZeroU64,
}

/// V3 shadow-write encoder, present only when `DualWriteConfig` is set on the sink.
/// Bundles the encoder with its target URI and sampling cadence so the three can't
/// drift out of sync with each other.
struct ShadowEncoder {
    encoder: DatadogMetricsV3Encoder,
    uri: Uri,
    every: NonZeroU64,
    /// Running count of legacy series flushes seen since sink startup.
    flush_count: u64,
}

impl ShadowEncoder {
    fn new(
        endpoint: DatadogMetricsEndpoint,
        uri: Uri,
        every: NonZeroU64,
        default_namespace: Option<String>,
    ) -> Self {
        Self {
            encoder: DatadogMetricsV3Encoder::new(endpoint, default_namespace),
            uri,
            every,
            flush_count: 0,
        }
    }

    /// Advances the flush counter and reports whether this flush should also produce a
    /// shadow write.
    const fn should_flush(&mut self) -> bool {
        self.flush_count = self.flush_count.wrapping_add(1);
        self.flush_count.is_multiple_of(self.every.get())
    }
}

/// Incremental request builder specific to Datadog metrics.
pub struct DatadogMetricsRequestBuilder {
    endpoint_configuration: DatadogMetricsEndpointConfiguration,
    series_encoder: EncoderKind,
    sketches_encoder: EncoderKind,
    /// Series-only V3 shadow encoder, present only when `dual_write` is enabled.
    /// There is deliberately no sketches equivalent; see `ShadowBuilderConfig`.
    shadow: Option<ShadowEncoder>,
}

impl DatadogMetricsRequestBuilder {
    pub fn new(
        endpoint_configuration: DatadogMetricsEndpointConfiguration,
        default_namespace: Option<String>,
        series_api_version: SeriesApiVersion,
        sketches_api_version: SketchesApiVersion,
        shadow_config: Option<ShadowBuilderConfig>,
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

        // Series only: no sketches shadow encoder is ever constructed, so an enabled
        // `dual_write` cannot produce sketches traffic.
        let shadow = shadow_config.map(|config| {
            ShadowEncoder::new(
                DatadogMetricsEndpoint::Series(config.series_api_version),
                config.series_uri,
                config.shadow_every,
                config.default_namespace,
            )
        });

        Self {
            endpoint_configuration,
            series_encoder,
            sketches_encoder,
            shadow,
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

        // Determine whether this flush triggers a shadow. Only legacy (V1/V2) *series*
        // batches count: V3 series is already on the target wire format, and sketches are
        // never shadowed at all because the V3 sketches intake routes don't exist.
        let is_v1v2_series = matches!(
            endpoint,
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V1 | SeriesApiVersion::V2)
        );
        let is_shadow_flush = is_v1v2_series
            && self
                .shadow
                .as_mut()
                .is_some_and(ShadowEncoder::should_flush);

        // UUIDv7 generated once per shadow flush; shared across primary + shadow requests.
        let batch_id: Option<Arc<str>> =
            is_shadow_flush.then(|| Arc::from(Uuid::now_v7().to_string().as_str()));

        // Clone metrics before primary encoding consumes them, if we need a shadow copy.
        // The shadow copy must not carry the production `EventFinalizers`: cloning a `Metric`
        // clones its `Arc<EventFinalizer>` pointers, so without stripping them here, the
        // validation-only shadow batch would share finalizers with the primary batch. A
        // shadow-only failure could then reject (or indefinitely delay) acknowledgement for
        // events whose primary V1/V2 request already succeeded. Dropping the taken finalizers
        // detaches the shadow copy from acknowledgement entirely.
        let shadow_metrics = is_shadow_flush.then(|| {
            let mut shadow_m = metrics.clone();
            for metric in &mut shadow_m {
                drop(metric.take_finalizers());
            }
            shadow_m
        });

        // ── Primary encode ────────────────────────────────────────────────────
        // V3Beta uses the same columnar encoder path as V3; only the
        // URI differs (set in endpoint_configuration at build time).
        let encoder = match endpoint {
            DatadogMetricsEndpoint::Series(_) => &mut self.series_encoder,
            DatadogMetricsEndpoint::Sketches => &mut self.sketches_encoder,
        };
        let mut results = encode_batch(encoder, api_key.clone(), endpoint, metrics);

        // Stamp batch ID and independent seq/len on primary results before merging.
        stamp_batch_id(batch_id.as_ref(), &mut results);
        stamp_sequence(&mut results);

        // ── Shadow encode (V3 series only) ────────────────────────────────────
        // `shadow_metrics` is populated only on a shadow flush, which already implies a
        // legacy series batch with the sampling cadence satisfied, so this can never emit
        // sketches traffic.
        if let (Some(shadow_m), Some(shadow)) = (shadow_metrics, self.shadow.as_mut()) {
            let mut shadow_results = encode_batch(
                &mut shadow.encoder,
                api_key,
                DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
                shadow_m,
            );

            // Override the URI so these requests go to the shadow endpoint, not V3 public API.
            for ((meta, _), _) in shadow_results.iter_mut().flatten() {
                meta.target_uri = Some(shadow.uri.clone());
            }
            // Shadow seq/len is independent of primary: the intake uses request ID +
            // seq/len to reassemble split payloads within one encoder's output.
            stamp_batch_id(batch_id.as_ref(), &mut shadow_results);
            stamp_sequence(&mut shadow_results);
            results.extend(shadow_results);
        }

        results
    }

    fn build_request(&mut self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (ddmetrics_metadata, request_metadata) = metadata;

        let uri = ddmetrics_metadata.target_uri.unwrap_or_else(|| {
            self.endpoint_configuration
                .get_uri_for_endpoint(ddmetrics_metadata.endpoint)
        });

        DatadogMetricsRequest {
            api_key: ddmetrics_metadata.api_key,
            payload,
            uri,
            content_type: ddmetrics_metadata.endpoint.content_type(),
            content_encoding: ddmetrics_metadata.endpoint.compression().content_encoding(),
            finalizers: ddmetrics_metadata.finalizers,
            metadata: request_metadata,
            batch_id: ddmetrics_metadata.batch_id,
            batch_seq: ddmetrics_metadata.batch_seq,
            batch_len: ddmetrics_metadata.batch_len,
        }
    }
}

/// Fills in a single shared timestamp on every metric that doesn't carry one.
///
/// Sources such as `statsd` never set a timestamp, and both encoders independently fall back
/// to `Utc::now()` *per metric* (`encoder::encode_timestamp` / `encoder_v3::encode_timestamp`).
/// Because the primary and V3 shadow payloads are encoded sequentially from the same batch,
/// any flush whose encoding straddles a second boundary ends up with different timestamps in
/// each payload, which the intake's V2/V3 comparison reports as a large set of
/// present-in-one-side-only series. It also means a single flush's points can be split across
/// two seconds within the V2 payload alone.
///
/// Resolving the fallback once per flush makes the primary and shadow payloads agree exactly,
/// and gives every point in a flush one coherent timestamp.
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

// ── Batch ID and sequence stamping ────────────────────────────────────────────

type EncodedResults =
    Vec<Result<((DDMetricsMetadata, RequestMetadata), Bytes), RequestBuilderError>>;

/// Sets `batch_id` on every successful result in `results`.
fn stamp_batch_id(batch_id: Option<&Arc<str>>, results: &mut EncodedResults) {
    if let Some(id) = batch_id {
        for ((meta, _), _) in results.iter_mut().flatten() {
            meta.batch_id = Some(Arc::clone(id));
        }
    }
}

/// Sets `batch_seq` and `batch_len` on every successful result based on their
/// position among the *successful* results.  Called after all encoding (primary +
/// shadow) is done so the total count is known.
///
/// Must count and number `Ok` results only: a split chunk that fails to encode
/// (`Err`) is dropped and never becomes a request (see `builder.rs`'s `Ok`-only
/// filtering), so including it in `len` or letting it consume a `seq` value would
/// advertise a part that will never be sent — the intake then waits for that
/// missing sequence number forever, timing out the whole reassembly.
fn stamp_sequence(results: &mut EncodedResults) {
    let len = results.iter().filter(|result| result.is_ok()).count();
    for (seq, ((meta, _), _)) in results.iter_mut().flatten().enumerate() {
        meta.batch_seq = seq;
        meta.batch_len = len;
    }
}

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
                        batch_id: None,
                        batch_seq: 0,
                        batch_len: 1,
                        target_uri: None,
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
                batch_id: None,
                batch_seq: 0,
                batch_len: 1,
                target_uri: None,
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
        event::{MetricKind, MetricValue, metric::MetricSketch},
        metrics::AgentDDSketch,
        request_metadata::GroupedCountByteSize,
    };

    use super::*;

    fn ok_result() -> Result<((DDMetricsMetadata, RequestMetadata), Bytes), RequestBuilderError> {
        let metadata = DDMetricsMetadata {
            api_key: None,
            endpoint: DatadogMetricsEndpoint::Series(SeriesApiVersion::V3),
            finalizers: EventFinalizers::default(),
            batch_id: None,
            batch_seq: 0,
            batch_len: 0,
            target_uri: None,
        };
        let request_metadata =
            RequestMetadata::new(0, 0, 0, 0, GroupedCountByteSize::new_untagged());
        Ok(((metadata, request_metadata), Bytes::new()))
    }

    fn err_result() -> Result<((DDMetricsMetadata, RequestMetadata), Bytes), RequestBuilderError> {
        Err(RequestBuilderError::FailedToSplit { dropped_events: 1 })
    }

    /// A split chunk that fails to encode must not consume a `batch_seq` slot or inflate
    /// `batch_len` — since it's dropped and never sent, doing so leaves the successfully
    /// encoded parts advertising a `seq`/`len` that doesn't match what's actually
    /// transmitted, causing the intake to wait forever for a part that will never arrive.
    #[test]
    fn stamp_sequence_numbers_only_ok_results() {
        let mut results: EncodedResults = vec![
            ok_result(),
            err_result(),
            ok_result(),
            err_result(),
            ok_result(),
        ];

        stamp_sequence(&mut results);

        let stamped: Vec<(usize, usize)> = results
            .iter()
            .flatten()
            .map(|((meta, _), _)| (meta.batch_seq, meta.batch_len))
            .collect();

        assert_eq!(stamped, vec![(0, 3), (1, 3), (2, 3)]);
    }

    #[test]
    fn stamp_sequence_with_no_failures_is_unchanged() {
        let mut results: EncodedResults = vec![ok_result(), ok_result()];

        stamp_sequence(&mut results);

        let stamped: Vec<(usize, usize)> = results
            .iter()
            .flatten()
            .map(|((meta, _), _)| (meta.batch_seq, meta.batch_len))
            .collect();

        assert_eq!(stamped, vec![(0, 2), (1, 2)]);
    }

    // ── Shadow dual-write is series-only ────────────────────────────────────────

    fn builder_with_shadow_every(every: u64) -> DatadogMetricsRequestBuilder {
        let endpoint_configuration = DatadogMetricsEndpointConfiguration::new(
            "https://example.com/api/v2/series".parse().unwrap(),
            "https://example.com/api/beta/sketches".parse().unwrap(),
        );

        DatadogMetricsRequestBuilder::new(
            endpoint_configuration,
            None,
            SeriesApiVersion::V2,
            SketchesApiVersion::V2,
            Some(ShadowBuilderConfig {
                series_uri: "https://example.com/api/intake/metrics/v3beta/series"
                    .parse()
                    .unwrap(),
                series_api_version: SeriesApiVersion::V3Beta,
                default_namespace: None,
                shadow_every: NonZeroU64::new(every).unwrap(),
            }),
        )
    }

    fn counter_metric() -> Metric {
        Metric::new(
            "test.counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        )
    }

    fn sketch_metric() -> Metric {
        let mut sketch = AgentDDSketch::with_agent_defaults();
        sketch.insert(1.0);
        Metric::new(
            "test.sketch",
            MetricKind::Incremental,
            MetricValue::Sketch {
                sketch: MetricSketch::AgentDDSketch(sketch),
            },
        )
    }

    fn encode(
        builder: &mut DatadogMetricsRequestBuilder,
        endpoint: DatadogMetricsEndpoint,
        metrics: Vec<Metric>,
    ) -> Vec<(DDMetricsMetadata, RequestMetadata)> {
        builder
            .encode_events_incremental(((None, endpoint), metrics))
            .into_iter()
            .filter_map(Result::ok)
            .map(|(metadata, _payload)| metadata)
            .collect()
    }

    /// The V3 sketches intake routes don't exist (404 -> retriable `ClientError` -> infinite
    /// retry loop), so a sketches flush must never produce a shadow request even when
    /// `dual_write` is fully enabled and sampling every flush.
    #[test]
    fn sketches_are_never_shadowed_even_with_dual_write_enabled() {
        let mut builder = builder_with_shadow_every(1);

        let encoded = encode(
            &mut builder,
            DatadogMetricsEndpoint::Sketches,
            vec![sketch_metric()],
        );

        assert_eq!(
            encoded.len(),
            1,
            "a sketches flush must yield only the primary request, got {} requests",
            encoded.len()
        );
        let (meta, _) = &encoded[0];
        assert_eq!(meta.endpoint, DatadogMetricsEndpoint::Sketches);
        assert!(
            meta.batch_id.is_none(),
            "sketches must not be stamped with a shadow X-Metrics-Request-ID"
        );
        assert!(
            meta.target_uri.is_none(),
            "sketches must never be retargeted at a V3 shadow endpoint"
        );
    }

    /// Series shadowing is unaffected by the sketches removal: a legacy series flush still
    /// emits a V2 primary plus a V3 shadow sharing one request ID.
    #[test]
    fn legacy_series_is_still_shadowed() {
        let mut builder = builder_with_shadow_every(1);

        let encoded = encode(
            &mut builder,
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V2),
            vec![counter_metric()],
        );

        assert_eq!(encoded.len(), 2, "expected a V2 primary and a V3 shadow");
        let ids: Vec<Option<Arc<str>>> = encoded
            .iter()
            .map(|(meta, _)| meta.batch_id.clone())
            .collect();
        assert!(
            ids.iter().all(Option::is_some),
            "both halves of the pair must carry a batch id"
        );
        assert_eq!(ids[0], ids[1], "the pair must share one request ID");
        assert_eq!(
            encoded
                .iter()
                .filter(|(meta, _)| meta.target_uri.is_some())
                .count(),
            1,
            "exactly one half of the pair is retargeted to the shadow URI"
        );
    }

    /// The shadow copy of a series flush must not carry the production `EventFinalizers`.
    /// If it did, a shadow-only failure (e.g. the validation-only V3beta endpoint rejecting
    /// or erroring on a request whose V1/V2 twin was delivered fine) would update the same
    /// shared `EventFinalizer`s as the primary and could reject or indefinitely delay
    /// acknowledgement for events that were already successfully delivered.
    #[test]
    fn shadow_copy_does_not_carry_production_finalizers() {
        use vector_lib::event::{BatchNotifier, BatchStatus};

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let finalized_metric = counter_metric().with_batch_notifier(&batch);
        drop(batch);

        let mut builder = builder_with_shadow_every(1);
        let mut encoded = builder.encode_events_incremental((
            (None, DatadogMetricsEndpoint::Series(SeriesApiVersion::V2)),
            vec![finalized_metric],
        ));

        assert_eq!(encoded.len(), 2, "expected a V2 primary and a V3 shadow");

        let metas: Vec<DDMetricsMetadata> = encoded
            .drain(..)
            .filter_map(Result::ok)
            .map(|((meta, _), _)| meta)
            .collect();

        let with_finalizers = metas.iter().filter(|m| !m.finalizers.is_empty()).count();
        let without_finalizers = metas.iter().filter(|m| m.finalizers.is_empty()).count();
        assert_eq!(
            with_finalizers, 1,
            "exactly the primary request should carry the production finalizers"
        );
        assert_eq!(
            without_finalizers, 1,
            "the shadow request must not carry any finalizers"
        );

        // Dropping every `DDMetricsMetadata` (and therefore every retained `EventFinalizers`)
        // must resolve the batch exactly once, with its untouched default status of
        // `Delivered` — proving the shadow copy held no live reference into the same
        // finalizer (an extra live reference would keep the batch notifier alive and this
        // `try_recv` would still return `Empty`).
        drop(metas);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
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
    /// `Utc::now()` per metric. Encoding the primary and the shadow sequentially therefore
    /// produced different timestamps whenever the flush straddled a second boundary, which
    /// the intake's V2/V3 comparison reported as thousands of one-sided series. Every
    /// timestamp-less metric in a flush must come out with one identical timestamp.
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

    /// The shadow cadence counter must only advance on series flushes. If sketches flushes
    /// still ticked it, the sampling rate would drift with the sketch/series mix.
    #[test]
    fn sketches_flushes_do_not_advance_the_series_shadow_cadence() {
        let mut builder = builder_with_shadow_every(2);

        for _ in 0..5 {
            let encoded = encode(
                &mut builder,
                DatadogMetricsEndpoint::Sketches,
                vec![sketch_metric()],
            );
            assert!(
                encoded.iter().all(|(meta, _)| meta.batch_id.is_none()),
                "sketches flush produced shadow traffic"
            );
        }

        // With `shadow_every: 2`, the first series flush is #1 and must not shadow...
        let first = encode(
            &mut builder,
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V2),
            vec![counter_metric()],
        );
        assert_eq!(
            first.len(),
            1,
            "series flush #1 should not shadow; the 5 sketches flushes must not have \
             advanced the counter"
        );

        // ...and the second is #2, which does.
        let second = encode(
            &mut builder,
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V2),
            vec![counter_metric()],
        );
        assert_eq!(second.len(), 2, "series flush #2 should shadow");
    }
}
