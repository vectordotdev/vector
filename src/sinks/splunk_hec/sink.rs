use async_trait::async_trait;
use futures_util::stream::BoxStream;
use vector_core::{event::Event, sink::StreamSink};

use crate::sinks::util::RequestBuilder;

pub struct SplunkHecSink {}

impl SplunkHecSink {
    pub(crate) fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) {
        todo!()
    }
}

pub struct SplunkHecRequestBuilder {}

impl RequestBuilder<(String, Vec<Event>)> for SplunkHecRequestBuilder {
    type Metadata;
    type Events;
    type Encoder;
    type Payload;
    type Request;
    type Error;

    fn compression(&self) -> crate::sinks::util::Compression {
        todo!()
    }

    fn encoder(&self) -> &Self::Encoder {
        todo!()
    }

    fn split_input(&self, input: (String, Vec<Event>)) -> (Self::Metadata, Self::Events) {
        todo!()
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        todo!()
    }
}

#[async_trait]
impl StreamSink for SplunkHecSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        todo!()
    }
}
