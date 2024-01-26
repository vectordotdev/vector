use bytes::Bytes;
use snafu::Snafu;
use std::sync::Arc;
use vector_lib::event::{EventFinalizers, Finalizable, Metric};
use vector_lib::request_metadata::RequestMetadata;

use super::{
    config::{DatadogMetricsEndpoint, DatadogMetricsEndpointConfiguration},
    encoder::{CreateError, DatadogMetricsEncoder, EncoderError, FinishError},
    service::DatadogMetricsRequest,
};
use crate::sinks::util::{metadata::RequestMetadataBuilder, IncrementalRequestBuilder};

#[derive(Debug, Snafu)]
pub enum RequestBuilderError {
    #[snafu(
        context(false),
        display("Failed to build the request builder: {source}")
    )]
    FailedToBuild { source: CreateError },

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
            Self::FailedToBuild { source } => (source.to_string(), source.as_error_type(), 0),
            // Encoding errors always happen at the per-metric level, so we could only ever drop a
            // single metric/event at a time.
            Self::FailedToEncode { source } => (source.to_string(), source.as_error_type(), 1),
            Self::FailedToSplit { dropped_events } => (
                "A split payload was still too big to encode/compress withing size limits."
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
                DatadogMetricsEndpoint::series(),
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
        let (tmp, mut metrics) = input;
        let (api_key, endpoint) = tmp;

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
                    Ok((encode_result, mut metrics)) => {
                        let finalizers = metrics.take_finalizers();
                        let metadata = DDMetricsMetadata {
                            api_key: api_key.as_ref().map(Arc::clone),
                            endpoint,
                            finalizers,
                        };

                        let request_metadata =
                            RequestMetadataBuilder::from_events(&metrics).build(&encode_result);

                        results.push(Ok((
                            (metadata, request_metadata),
                            encode_result.into_payload(),
                        )));
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
                        // Protocol Buffers data, similar to how the Datadog Agent does it with
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
                                results.push(encode_now_or_never(
                                    encoder,
                                    api_key.as_ref().map(Arc::clone),
                                    endpoint,
                                    chunk,
                                ));
                                recommended_splits -= 1;
                            }
                            results.push(encode_now_or_never(
                                encoder,
                                api_key.as_ref().map(Arc::clone),
                                endpoint,
                                metrics,
                            ));
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
        let (ddmetrics_metadata, request_metadata) = metadata;
        let uri = self
            .endpoint_configuration
            .get_uri_for_endpoint(ddmetrics_metadata.endpoint);

        DatadogMetricsRequest {
            api_key: ddmetrics_metadata.api_key,
            payload,
            uri,
            content_type: ddmetrics_metadata.endpoint.content_type(),
            finalizers: ddmetrics_metadata.finalizers,
            metadata: request_metadata,
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
