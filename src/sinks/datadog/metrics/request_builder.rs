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
    default_namespace: Option<String>,
}

impl DatadogMetricsRequestBuilder {
    pub fn new(
        endpoint_configuration: DatadogMetricsEndpointConfiguration,
        default_namespace: Option<String>,
    ) -> Self {
        Self {
            endpoint_configuration,
            default_namespace,
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
        &self,
        input: (DatadogMetricsEndpoint, Vec<Metric>),
    ) -> Result<Vec<(Self::Metadata, Self::Payload)>, Self::Error> {
        let (endpoint, mut metrics) = input;
        let mut metric_drain = metrics.drain(..);

        let mut results = Vec::new();
        let mut pending = None;
        while metric_drain.len() != 0 {
            let mut n = 0;
            let mut encoder = DatadogMetricsEncoder::new(endpoint, self.default_namespace.clone())?;

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
                        EncoderError::TooLarge { metrics } => {
                            let mut first = metrics;
                            let second = first.split_off(first.len() / 2);

                            let result = encode_now_or_never(
                                first,
                                endpoint,
                                self.default_namespace.clone(),
                            )?;
                            results.push(result);

                            let result = encode_now_or_never(
                                second,
                                endpoint,
                                self.default_namespace.clone(),
                            )?;
                            results.push(result);
                        }
                        // Not an error we can do anything about, so just forward it on.
                        suberr => return Err(suberr),
                    },
                }
            }
        }

        Ok(results)
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
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
    metrics: Vec<Metric>,
    endpoint: DatadogMetricsEndpoint,
    default_namespace: Option<String>,
) -> Result<((DatadogMetricsEndpoint, usize, EventFinalizers), Bytes), EncoderError> {
    let mut n = 0;
    let mut encoder = DatadogMetricsEncoder::new(endpoint, default_namespace)?;
    for metric in metrics {
        if let Some(_) = encoder.try_encode(metric)? {
            // We're co-opting the TooLarge error here, but any error that a caller receives from
            // this method implies we failed, and that they need to tpack up and go home.
            return Err(EncoderError::TooLarge {
                metrics: Vec::new(),
            });
        }
        n += 1;
    }

    encoder.finish().map(|(payload, mut processed)| {
        let finalizers = processed.take_finalizers();
        ((endpoint, n, finalizers), payload.into())
    })
}
