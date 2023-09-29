use std::io;

use bytes::Bytes;
use vector_common::request_metadata::RequestMetadata;

use crate::sinks::util::{
    encoding::Encoder, metadata::RequestMetadataBuilder, request_builder::EncodeResult,
    Compression, RequestBuilder,
};

mod blueprint;
pub use self::blueprint::*;

mod metadata;
pub use self::metadata::*;

use super::HttpRequest;

pub struct HttpRequestBuilder<E = (), IS = ()> {
    blueprint: RequestBlueprint,
    encoder: E,
    splitter: IS,
    compression: Compression,
}

impl<E: Default, IS: Default> HttpRequestBuilder<E, IS> {
    pub fn from_blueprint(blueprint: RequestBlueprint) -> Self {
        Self {
            blueprint,
            encoder: E::default(),
            splitter: IS::default(),
            compression: Compression::None,
        }
    }
}

impl<E, IS> HttpRequestBuilder<E, IS> {
    pub fn with_encoder<E2>(self, encoder: E2) -> HttpRequestBuilder<E2, IS> {
        HttpRequestBuilder {
            blueprint: self.blueprint,
            encoder,
            splitter: self.splitter,
            compression: self.compression,
        }
    }

    pub fn with_input_splitter<IS2>(self, splitter: IS2) -> HttpRequestBuilder<E, IS2> {
        HttpRequestBuilder {
            blueprint: self.blueprint,
            encoder: self.encoder,
            splitter,
            compression: self.compression,
        }
    }

    pub fn with_compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }
}

impl<Input, E, IS> RequestBuilder<Input> for HttpRequestBuilder<E, IS>
where
    E: Encoder<Input>,
    IS: InputSplitter<Input>,
{
    type Metadata = IS::Metadata;
    type Events = IS::Output;
    type Encoder = E;
    type Payload = Bytes;
    type Request = HttpRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, input: Input) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        IS::split(input)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (finalizers, _) = metadata.into_parts();

        let http_request = self
            .blueprint
            .create_http_request(payload.into_payload().into());

        let mut request = HttpRequest {
            http_request,
            finalizers,
            request_metadata,
        };
    }
}
