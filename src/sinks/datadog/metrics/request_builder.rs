use std::io;

use bytes::Bytes;
use http::Uri;
use vector_core::event::{EventFinalizers, Finalizable, Metric};

use crate::sinks::util::{Compression, IncrementalRequestBuilder, encoding::StatefulEncoder};
use super::{
    config::DatadogMetricsEndpoint, encoder::DatadogMetricsEncoder, service::DatadogMetricsRequest,
};

pub struct DatadogMetricsRequestBuilder {
    compression: Compression,
    endpoint_uri_mappings: Vec<(DatadogMetricsEndpoint, Uri)>,
}

impl DatadogMetricsRequestBuilder {
    pub fn new(compression: Compression, endpoint_uri_mappings: Vec<(DatadogMetricsEndpoint, Uri)>) -> Self {
        Self {
            compression,
            endpoint_uri_mappings,
        }
    }

    fn get_endpoint_uri(&self, endpoint: DatadogMetricsEndpoint) -> Option<Uri> {
        self.endpoint_uri_mappings.iter()
            .find(|e| e.0 == endpoint)
            .map(|e| e.1.clone())
    }
}

impl IncrementalRequestBuilder<(DatadogMetricsEndpoint, Vec<Metric>)> for DatadogMetricsRequestBuilder {
    type Metadata = (DatadogMetricsEndpoint, usize, EventFinalizers);
    type Payload = Bytes;
    type Request = DatadogMetricsRequest;
    type Error = io::Error;

    fn encode_events_incremental(&self, input: (DatadogMetricsEndpoint, Vec<Metric>))
        -> Result<Vec<(Self::Metadata, Self::Payload)>, Self::Error>
    {
        let (endpoint, mut metrics) = input;
        let mut metric_drain = metrics.drain(..);

        let mut results = Vec::new();
        let mut pending = None;
        while metric_drain.len() != 0 {
            let mut n = 0;
            let mut finalizers = EventFinalizers::default();
            let mut encoder = DatadogMetricsEncoder::new(endpoint, self.compression)?;

            loop {
                // Grab the previously pending metric, or the next metric from the drain.
                let mut metric = if let Some(metric) = pending.take() {
                    metric
                } else {
                    match metric_drain.next() {
                        Some(metric) => metric,
                        None => break,
                    }
                };

                // Try encoding the metric.
                let subfinalizers = metric.take_finalizers();
                match encoder.try_encode(metric)? {
                    None => {
                        // We encoded the metric successfully, so update our metadata and continue.
                        finalizers.merge(subfinalizers);
                        n += 1;
                    },
                    Some(mut metric) => {
                        // The encoded metric would not fit within the configured limits, so we need
                        // to finish the current encoder and generate our payload, and keep going.
                        metric.metadata_mut().merge_finalizers(subfinalizers);
                        pending = Some(metric);
                        break
                    }
                }
            }

            // If we encoded one or more metrics this pass, finalize the payload.
            if n > 0 {
                if let Some(payload) = encoder.finish()? {
                    results.push(((endpoint, n, finalizers), payload.into()));
                } else {
                    error!("Finalized payload was over the configured limits; {} event dropped as a result", n);
                }
            }
        }

        Ok(results)
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (endpoint, batch_size, finalizers) = metadata;
        let uri = self.get_endpoint_uri(endpoint)
            .expect("unable to find URI for metric endpoint");

        DatadogMetricsRequest {
            payload,
            uri,
            content_type: endpoint.content_type(),
            compression: self.compression,
            finalizers,
            batch_size,
        }
    }
}
