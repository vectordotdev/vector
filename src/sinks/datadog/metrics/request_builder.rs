use std::{num::NonZeroU64, sync::Arc};

use bytes::Bytes;
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
pub struct ShadowBuilderConfig {
    /// The URI for the V3 shadow series endpoint (e.g. `/api/intake/metrics/v3/series`).
    pub series_uri: Uri,
    /// The `SeriesApiVersion` variant matching the shadow series endpoint.
    /// Used to set the correct payload limits and compression on the shadow encoder.
    pub series_api_version: SeriesApiVersion,
    /// The URI for the V3 shadow sketches endpoint (e.g. `/api/intake/metrics/v3/sketches`).
    pub sketches_uri: Uri,
    /// Default metric namespace for the shadow encoders.
    pub default_namespace: Option<String>,
    /// Send a V3 shadow once per this many legacy (V1/V2 series, or non-V3 sketches) flushes.
    pub shadow_every: NonZeroU64,
}

/// V3 shadow-write encoder, present only when `DualWriteConfig` is set on the sink.
/// Bundles the encoder with its target URI and sampling cadence so the three can't
/// drift out of sync with each other.
struct ShadowEncoder {
    encoder: DatadogMetricsV3Encoder,
    uri: Uri,
    every: NonZeroU64,
    /// Running count of legacy flushes seen since sink startup.
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
    /// Present only when `DualWriteConfig` is set on the sink.
    shadow: Option<ShadowEncoder>,
    /// Present only when `DualWriteConfig` is set on the sink.
    sketches_shadow: Option<ShadowEncoder>,
    /// True when `sketches_api_version` is the legacy (non-V3) format, i.e. when a V3
    /// sketches shadow write is meaningful. `DatadogMetricsEndpoint::Sketches` doesn't carry
    /// the api version the way `DatadogMetricsEndpoint::Series` does, so this has to be
    /// tracked separately.
    sketches_is_legacy: bool,
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

        let (shadow, sketches_shadow) = match shadow_config {
            Some(config) => (
                Some(ShadowEncoder::new(
                    DatadogMetricsEndpoint::Series(config.series_api_version),
                    config.series_uri,
                    config.shadow_every,
                    config.default_namespace.clone(),
                )),
                Some(ShadowEncoder::new(
                    DatadogMetricsEndpoint::Sketches,
                    config.sketches_uri,
                    config.shadow_every,
                    config.default_namespace,
                )),
            ),
            None => (None, None),
        };

        Self {
            endpoint_configuration,
            series_encoder,
            sketches_encoder,
            shadow,
            sketches_shadow,
            sketches_is_legacy: !sketches_api_version.is_v3_format(),
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

        // Determine whether this flush triggers a shadow. Only legacy (non-V3) batches are
        // counted — V3 series and V3 sketches are already on the target wire format, so
        // shadowing them would be redundant.
        let is_v1v2_series = matches!(
            endpoint,
            DatadogMetricsEndpoint::Series(SeriesApiVersion::V1 | SeriesApiVersion::V2)
        );
        let is_legacy_sketches =
            matches!(endpoint, DatadogMetricsEndpoint::Sketches) && self.sketches_is_legacy;
        let is_shadow_flush = if is_v1v2_series {
            self.shadow
                .as_mut()
                .is_some_and(ShadowEncoder::should_flush)
        } else if is_legacy_sketches {
            self.sketches_shadow
                .as_mut()
                .is_some_and(ShadowEncoder::should_flush)
        } else {
            false
        };

        // UUIDv7 generated once per shadow flush; shared across primary + shadow requests.
        let batch_id: Option<Arc<str>> =
            is_shadow_flush.then(|| Arc::from(Uuid::now_v7().to_string().as_str()));

        // Clone metrics before primary encoding consumes them, if we need a shadow copy.
        let shadow_metrics = is_shadow_flush.then(|| metrics.clone());

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

        // ── Shadow encode (V3) ────────────────────────────────────────────────
        let shadow_target = if is_v1v2_series {
            self.shadow
                .as_mut()
                .map(|shadow| (shadow, DatadogMetricsEndpoint::Series(SeriesApiVersion::V3)))
        } else {
            self.sketches_shadow
                .as_mut()
                .map(|shadow| (shadow, DatadogMetricsEndpoint::Sketches))
        };

        if let (Some(shadow_m), Some((shadow, shadow_endpoint))) = (shadow_metrics, shadow_target) {
            let mut shadow_results =
                encode_batch(&mut shadow.encoder, api_key, shadow_endpoint, shadow_m);

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
                    mut recommended_splits,
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
                    let mut split_idx = metrics.len();
                    let stride = split_idx / recommended_splits;

                    while recommended_splits > 1 {
                        split_idx -= stride;
                        let chunk = metrics.split_off(split_idx);
                        results.push(encode_chunk(encoder, api_key.clone(), endpoint, chunk));
                        recommended_splits -= 1;
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
    use vector_lib::request_metadata::GroupedCountByteSize;

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
}
