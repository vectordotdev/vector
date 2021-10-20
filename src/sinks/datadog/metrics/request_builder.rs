use bytes::Bytes;
use vector_core::event::{EventFinalizers, Finalizable, Metric};

use super::{
    config::{DatadogMetricsEndpoint, DatadogMetricsEndpointConfiguration},
    encoder::{DatadogMetricsEncoder, EncoderError},
    service::DatadogMetricsRequest,
};
use crate::sinks::util::IncrementalRequestBuilder;

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
    ) -> Self {
        Self {
            endpoint_configuration,
            series_encoder: DatadogMetricsEncoder::new(
                DatadogMetricsEndpoint::Series,
                default_namespace.clone(),
            ),
            sketches_encoder: DatadogMetricsEncoder::new(
                DatadogMetricsEndpoint::Sketches,
                default_namespace,
            ),
        }
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
    type Error = EncoderError;

    fn encode_events_incremental(
        &mut self,
        input: (DatadogMetricsEndpoint, Vec<Metric>),
    ) -> Result<Vec<(Self::Metadata, Self::Payload)>, Self::Error> {
        let (endpoint, mut metrics) = input;
        let encoder = self.get_encoder(endpoint);
        let mut metric_drain = metrics.drain(..);

        let mut results = Vec::new();
        let mut pending = None;
        while metric_drain.len() != 0 {
            let mut n = 0;

            loop {
                // Grab the previously pending metric, or the next metric from the drain.
                let metric = if let Some(metric) = pending.take() {
                    metric
                } else {
                    match metric_drain.next() {
                        Some(metric) => metric,
                        None => break,
                    }
                };

                // Try encoding the metric.
                match encoder.try_encode(metric)? {
                    // We encoded the metric successfully, so update our metadata and continue.
                    None => n += 1,
                    Some(metric) => {
                        // The encoded metric would not fit within the configured limits, so we need
                        // to finish the current encoder and generate our payload, and keep going.
                        pending = Some(metric);
                        break;
                    }
                }
            }

            // If we encoded one or more metrics this pass, finalize the payload.
            if n > 0 {
                match encoder.finish() {
                    Ok((payload, mut metrics)) => {
                        let finalizers = metrics.take_finalizers();
                        results.push(((endpoint, n, finalizers), payload.into()));
                    }
                    Err(err) => match err {
                        // The encoder informed us that the resulting payload was too big, so we're
                        // being given a chance here to split it into smaller input batches in the
                        // hopes of generating a smaller payload that _isn't_ too big.  We only
                        // attempt this once.
                        //
                        // TODO: In the future, when we have a way to incrementally write out
                        // Protocol Buffers data, similiar to how the Datadog Agent does it with
                        // `molecule`, we can wrap all of the sketch encoding into the same
                        // incremental encoding paradigm and avoid this.
                        EncoderError::TooLarge {
                            mut metrics,
                            recommended_splits,
                        } => {
                            // The encoder recommends how many splits we should do based on how much
                            // the set of processed metrics exceeded the payload size limits, so we
                            // do those splits here.
                            let mut chunks = Vec::new();
                            let mut n = recommended_splits;
                            let mut remaining = metrics.len();
                            let stride = remaining / n;

                            while n > 1 {
                                remaining -= stride;
                                let chunk = metrics.split_off(remaining);
                                chunks.push(chunk);
                                n -= 1;
                            }
                            chunks.push(metrics);

                            // Now encode them.
                            for chunk in chunks {
                                let _chunk_size = chunk.len();
                                match encode_now_or_never(encoder, endpoint, chunk) {
                                    Ok(result) => results.push(result),
                                    Err(_) => {
                                        // We failed to encode the chunk, consider it lost.
                                        // TODO: emit metrics here to signal that we lost metrics
                                        // due to an encoder failure.
                                    }
                                }
                            }
                        }
                        // Not an error we can do anything about, so just forward it on.
                        suberr => {
                            // TODO: emit metrics here to signal that we lost metrics due to an
                            // encoder failure.
                            return Err(suberr);
                        }
                    },
                }
            }
        }

        Ok(results)
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

/// Simple encoder implementation that doesn't try to split.
///
/// This is required because the issues mentioned in `DatadogMetricsRequestBuilder` with not being
/// able to _actually_ encode Protocol Buffers data incrementally.  We split batches that end up
/// being too large, and simply try to encode them here: in isolation, without further splitting.
fn encode_now_or_never(
    encoder: &mut DatadogMetricsEncoder,
    endpoint: DatadogMetricsEndpoint,
    mut metrics: Vec<Metric>,
) -> Result<((DatadogMetricsEndpoint, usize, EventFinalizers), Bytes), EncoderError> {
    let mut n = 0;

    // We don't return `EncoderError::TooLarge` during a `try_encode` call, so if we get an error
    // here, it's unrecoverable.  If we failed to encode all of the metrics, we just break from the
    // loop early and return `EncoderError::TooLarge` ourselves.  Since we used the recommended
    // number of splits directly from the encoder to get here, if we're hitting the limits before
    // encoding all the metrics in the batch, then something is super wrong and, again, essentially
    // unrecoverable.
    while let Some(metric) = metrics.pop() {
        match encoder.try_encode(metric)? {
            Some(metric) => {
                metrics.push(metric);
                return Err(EncoderError::TooLarge {
                    metrics: Vec::new(),
                    recommended_splits: 0,
                });
            }
            None => {}
        }
        n += 1;
    }

    encoder.finish().map(|(payload, mut processed)| {
        let finalizers = processed.take_finalizers();
        ((endpoint, n, finalizers), payload.into())
    })
}
