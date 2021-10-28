use bytes::Bytes;
use serde_json::error::Category;
use snafu::Snafu;
use vector_core::event::{EventFinalizers, Finalizable, Metric};

use super::{
    config::{DatadogMetricsEndpoint, DatadogMetricsEndpointConfiguration},
    encoder::{CreateError, DatadogMetricsEncoder, EncoderError, FinishError},
    service::DatadogMetricsRequest,
};
use crate::sinks::util::IncrementalRequestBuilder;

#[derive(Debug, Snafu)]
pub enum RequestBuilderError {
    #[snafu(display("Failed to build the request builder: {}", error_type))]
    FailedToBuild { error_type: &'static str },

    #[snafu(display("Encoding of a metric failed ({})", reason))]
    FailedToEncode {
        reason: &'static str,
        dropped_events: u64,
    },

    #[snafu(display("A split payload was still too big to encode/compress within size limits"))]
    FailedToSplit { dropped_events: u64 },

    #[snafu(display("An unexpected error occurred"))]
    Unexpected {
        error_type: &'static str,
        dropped_events: u64,
    },
}

impl RequestBuilderError {
    /// Converts this error into its constituent parts: the error reason, and how many events were
    /// dropped as a result.
    pub const fn into_parts(self) -> (&'static str, u64) {
        match self {
            Self::FailedToBuild { error_type } => (error_type, 0),
            Self::FailedToEncode {
                reason,
                dropped_events,
            } => (reason, dropped_events),
            Self::FailedToSplit { dropped_events } => ("split_failed", dropped_events),
            Self::Unexpected {
                error_type,
                dropped_events,
            } => (error_type, dropped_events),
        }
    }
}

impl From<CreateError> for RequestBuilderError {
    fn from(e: CreateError) -> Self {
        match e {
            CreateError::InvalidLimits => Self::FailedToBuild {
                error_type: "invalid_payload_limits",
            },
        }
    }
}

impl From<EncoderError> for RequestBuilderError {
    fn from(e: EncoderError) -> Self {
        match e {
            // Series metrics (JSON) are encoded incrementally, so we can only ever lose a single
            // metric for a JSON encoding failure.
            EncoderError::JsonEncodingFailed { source } => Self::FailedToEncode {
                reason: match source.classify() {
                    Category::Io => "json_io",
                    Category::Syntax => "json_syntax",
                    Category::Data => "json_data",
                    Category::Eof => "json_eof",
                },
                dropped_events: 1,
            },
            // Sketch metrics (Protocol Buffers) are encoded in a single shot, so naturally we would
            // expect `dropped_events` to be 1-N, instead of always 1.  We should never emit this
            // metric when calling `try_encode`, which is where we'd see the JSON variant of it.
            // This is because sketch encoding happens at the end.
            //
            // Thus, we default `dropped_events` to 1, and if we actually hit this error when
            // finishing up a payload, we'll fix up the true number of dropped events at that point.
            EncoderError::ProtoEncodingFailed { .. } => Self::FailedToEncode {
                // `prost` states that for an encoding error specifically, it can only ever fail due
                // to insufficient capacity in the encoding buffer.
                reason: "protobuf_insufficient_buf_capacity",
                dropped_events: 1,
            },
            // Not all metric types for valid depending on the configured endpoint of the encoder.
            EncoderError::InvalidMetric { metric_value, .. } => Self::FailedToEncode {
                // TODO: At some point, it would be nice to use `const_format` to build the reason
                // as "<invalid metric_type> _via_<endpoint>" to better understand in what context
                // metric X is being considered as invalid.  Practically it's not a huge issue,
                // because the number of metric types are fixed and we should be able to inspect the
                // code for issues, or if it became a big problem, we could just go ahead and do the
                // `const_format` work... but it'd be nice to be ahead of curve when trivially possible.
                reason: metric_value,
                dropped_events: 1,
            },
        }
    }
}

/// Incremental request builder specific to Datadog metrics.
pub struct DatadogMetricsRequestBuilder {
    endpoint_configuration: DatadogMetricsEndpointConfiguration,
    series_encoder: DatadogMetricsEncoder,
    sketches_encoder: DatadogMetricsEncoder,
}

impl DatadogMetricsRequestBuilder {
    pub fn new(
        endpoint_configuration: DatadogMetricsEndpointConfiguration,
        default_namespace: Option<String>,
    ) -> Result<Self, RequestBuilderError> {
        Ok(Self {
            endpoint_configuration,
            series_encoder: DatadogMetricsEncoder::new(
                DatadogMetricsEndpoint::Series,
                default_namespace.clone(),
            )?,
            sketches_encoder: DatadogMetricsEncoder::new(
                DatadogMetricsEndpoint::Sketches,
                default_namespace,
            )?,
        })
    }

    fn get_encoder(&mut self, endpoint: DatadogMetricsEndpoint) -> &mut DatadogMetricsEncoder {
        match endpoint {
            DatadogMetricsEndpoint::Series => &mut self.series_encoder,
            DatadogMetricsEndpoint::Sketches => &mut self.sketches_encoder,
        }
    }
}

impl IncrementalRequestBuilder<(DatadogMetricsEndpoint, Vec<Metric>)>
    for DatadogMetricsRequestBuilder
{
    type Metadata = (DatadogMetricsEndpoint, usize, EventFinalizers);
    type Payload = Bytes;
    type Request = DatadogMetricsRequest;
    type Error = RequestBuilderError;

    fn encode_events_incremental(
        &mut self,
        input: (DatadogMetricsEndpoint, Vec<Metric>),
    ) -> Vec<Result<(Self::Metadata, Self::Payload), Self::Error>> {
        let (endpoint, mut metrics) = input;
        let encoder = self.get_encoder(endpoint);
        let mut metric_drain = metrics.drain(..);

        let mut results = Vec::new();
        let mut pending = None;
        while metric_drain.len() != 0 {
            let mut n = 0;

            loop {
                // Grab the previously pending metric, or the next metric from the drain.
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
                    Ok((payload, mut metrics)) => {
                        let finalizers = metrics.take_finalizers();
                        results.push(Ok(((endpoint, n, finalizers), payload.into())));
                    }
                    Err(err) => match err {
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
                        // Protocol Buffers data, similiar to how the Datadog Agent does it with
                        // `molecule`, we can wrap all of the sketch encoding into the same
                        // incremental encoding paradigm and avoid this.
                        FinishError::TooLarge {
                            mut metrics,
                            mut recommended_splits,
                        } => {
                            let mut split_idx = metrics.len();
                            let stride = split_idx / recommended_splits;

                            while recommended_splits > 1 {
                                split_idx -= stride;
                                let chunk = metrics.split_off(split_idx);
                                results.push(encode_now_or_never(encoder, endpoint, chunk));
                                recommended_splits -= 1;
                            }
                            results.push(encode_now_or_never(encoder, endpoint, metrics));
                        }
                        // Not an error we can do anything about, so just forward it on.
                        suberr => results.push(Err(RequestBuilderError::Unexpected {
                            error_type: suberr.as_error_type(),
                            dropped_events: n as u64,
                        })),
                    },
                }
            }
        }

        results
    }

    fn build_request(&mut self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (endpoint, batch_size, finalizers) = metadata;
        let uri = self.endpoint_configuration.get_uri_for_endpoint(endpoint);

        DatadogMetricsRequest {
            payload,
            uri,
            content_type: endpoint.content_type(),
            finalizers,
            batch_size,
        }
    }
}

/// Simple encoder implementation that treats any error during encoding or finishing as unrecoverable.
///
/// We only call this method when our main encoding loop tried to finish a payload and was told
/// that the payload was too large compared to the payload size limits.  That error gives back any
/// metrics that were correctly encoded so that we can attempt to encode them again in smaller
/// chunks.  However, rather than continually trying smaller and smaller chunks, which could be
/// caused by a pathological error, we only attempt that operation once.  This method facilitates
/// the "only try it once" aspect by treating all errors as unrecoverable.
fn encode_now_or_never(
    encoder: &mut DatadogMetricsEncoder,
    endpoint: DatadogMetricsEndpoint,
    metrics: Vec<Metric>,
) -> Result<((DatadogMetricsEndpoint, usize, EventFinalizers), Bytes), RequestBuilderError> {
    let metrics_len = metrics.len() as u64;

    let n = metrics
        .into_iter()
        .try_fold(0, |n, metric| match encoder.try_encode(metric) {
            Ok(None) => Ok(n + 1),
            _ => Err(RequestBuilderError::FailedToSplit {
                dropped_events: metrics_len,
            }),
        })?;

    encoder
        .finish()
        .map(|(payload, mut processed)| {
            let finalizers = processed.take_finalizers();
            ((endpoint, n, finalizers), payload.into())
        })
        .map_err(|_| RequestBuilderError::FailedToSplit {
            dropped_events: metrics_len,
        })
}
