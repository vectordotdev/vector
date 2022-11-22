use crate::{
    sinks::{util::http::HttpSink, warp10::encoder::SensissionEncoder},
    Error,
};
use bytes::{Bytes, BytesMut};
use http::Request;
use vector_common::Result;

pub struct Warp10Sink {
    pub uri: String,
    pub token: String,
}

#[async_trait::async_trait]
impl HttpSink for Warp10Sink {
    type Input = BytesMut;
    type Output = BytesMut;
    type Encoder = SensissionEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        SensissionEncoder::new()
    }

    async fn build_request(&self, events: Self::Output) -> Result<Request<Bytes>> {
        Request::builder()
            .method("POST")
            .uri(self.uri.clone())
            .header("Content-Type", "text/plain")
            .header("X-Warp10-Token", self.token.clone())
            .body(events.freeze())
            .map_err(Error::from)
    }
}
