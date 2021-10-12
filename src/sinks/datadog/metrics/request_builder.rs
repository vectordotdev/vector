use std::io;

use bytes::Bytes;
use vector_core::event::{Event, EventFinalizers, Finalizable, Metric};

use crate::sinks::util::{encoding::Encoder, Compression, Compressor, RequestBuilder};

use super::{
    config::DatadogMetricsEndpoint, encoder::DatadogMetricsEncoder, service::DatadogMetricsRequest,
};

struct DatadogMetricsRequestBuilder {
    compression: Compression,
    encoder: DatadogMetricsEncoder,
}

impl RequestBuilder<(DatadogMetricsEndpoint, Vec<Event>)> for DatadogMetricsRequestBuilder {
    type Metadata = (DatadogMetricsEndpoint, usize, EventFinalizers);

    type Events = Vec<Metric>;

    type Encoder = DatadogMetricsEncoder;

    type Payload = Bytes;

    type Request = DatadogMetricsRequest;

    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (DatadogMetricsEndpoint, Vec<Event>),
    ) -> (Self::Metadata, Self::Events) {
        let (endpoint, mut events) = input;
        let finalizers = events.take_finalizers();
        let batch_size = events.len();

        let metrics = events.into_iter().map(|e| e.into_metric()).collect();

        ((endpoint, batch_size, finalizers), metrics)
    }

    fn encode_events(&self, events: Self::Events) -> Result<Self::Payload, Self::Error> {
        let mut compressor = Compressor::from(self.compression());
        let _ = self.encoder().encode_input(events, &mut compressor)?;

        let payload = compressor.into_inner().into();
        Ok(payload)
    }

    fn build_request(&self, _metadata: Self::Metadata, _payload: Self::Payload) -> Self::Request {
        todo!()
    }
}
