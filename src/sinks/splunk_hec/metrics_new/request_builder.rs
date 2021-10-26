use vector_core::{
    buffers::Ackable,
    event::{EventFinalizers, Finalizable},
    ByteSizeOf,
};

use crate::sinks::util::{encoding::EncodingConfig, Compression, RequestBuilder};

use super::{encoder::HecMetricsEncoder, sink::HecProcessedEvent};

pub struct HecMetricsRequest;

impl ByteSizeOf for HecMetricsRequest {
    fn allocated_bytes(&self) -> usize {
        todo!()
    }
}

impl Ackable for HecMetricsRequest {
    fn ack_size(&self) -> usize {
        todo!()
    }
}

impl Finalizable for HecMetricsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        todo!()
    }
}

pub struct HecMetricsRequestBuilder {
    pub compression: Compression,
    pub encoding: EncodingConfig<HecMetricsEncoder>,
}

impl RequestBuilder<Vec<HecProcessedEvent>> for HecMetricsRequestBuilder {
    type Metadata = (usize, usize, EventFinalizers);
    type Events = Vec<HecProcessedEvent>;
    type Encoder = EncodingConfig<HecMetricsEncoder>;
    type Payload = Vec<u8>;
    type Request = HecMetricsRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }

    fn split_input(&self, input: Vec<HecProcessedEvent>) -> (Self::Metadata, Self::Events) {
        todo!()
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        todo!()
    }
}
