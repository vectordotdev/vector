use vector_core::event::{EventFinalizers, Finalizable};

use crate::sinks::util::{encoding::EncodingConfig, Compression, RequestBuilder};

use super::{encoder::HecMetricsEncoder, sink::HecProcessedEvent};

pub struct HecMetricsRequest;

pub struct HecMetricsRequestBuilder {
    pub compression: Compression,
    pub encoding: EncodingConfig<HecMetricsEncoder>,
}

impl<'a> RequestBuilder<Vec<HecProcessedEvent<'a>>> for HecMetricsRequestBuilder {
    type Metadata = (usize, usize, EventFinalizers);
    type Events = Vec<HecProcessedEvent<'a>>;
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
