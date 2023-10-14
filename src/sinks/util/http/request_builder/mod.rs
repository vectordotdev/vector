use std::{io, marker::PhantomData};

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

pub struct HttpRequestBuilder<IS, E = ()> {
    blueprint: RequestBlueprint,
    splitter: PhantomData<IS>,
    encoder: E,
    compression: Compression,
}

impl HttpRequestBuilder<GenericEventInputSplitter> {
    pub const fn from_blueprint(blueprint: RequestBlueprint) -> Self {
        Self {
            blueprint,
            splitter: PhantomData,
            encoder: (),
            compression: Compression::None,
        }
    }
}

impl<IS, E> HttpRequestBuilder<IS, E> {
    pub fn with_encoder<E2>(self, encoder: E2) -> HttpRequestBuilder<IS, E2> {
        HttpRequestBuilder {
            blueprint: self.blueprint,
            splitter: self.splitter,
            encoder,
            compression: self.compression,
        }
    }

    pub fn with_input_splitter<IS2>(self) -> HttpRequestBuilder<IS2, E> {
        HttpRequestBuilder {
            blueprint: self.blueprint,
            splitter: PhantomData,
            encoder: self.encoder,
            compression: self.compression,
        }
    }

    pub const fn with_compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }

    pub const fn encoder(&self) -> &E {
        &self.encoder
    }
}

impl<Input, IS, E> RequestBuilder<Input> for HttpRequestBuilder<IS, E>
where
    IS: InputSplitter<Input>,
    E: Encoder<IS::Output>,
{
    type Metadata = EventMetadata<IS::Metadata>;
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

        let http_request = self.blueprint.create_http_request(payload.into_payload());

        HttpRequest {
            http_request,
            finalizers,
            request_metadata,
        }
    }
}
