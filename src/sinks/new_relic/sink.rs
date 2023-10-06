use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;

use super::{NewRelicApiRequest, NewRelicCredentials, NewRelicEncoder};
use crate::{
    http::get_http_scheme_from_uri, internal_events::SinkRequestBuildError, sinks::prelude::*,
};

#[derive(Debug)]
pub struct NewRelicSinkError {
    message: String,
}

impl NewRelicSinkError {
    pub fn new(msg: &str) -> Self {
        NewRelicSinkError {
            message: String::from(msg),
        }
    }

    pub fn boxed(msg: &str) -> Box<Self> {
        Box::new(NewRelicSinkError {
            message: String::from(msg),
        })
    }
}

impl std::fmt::Display for NewRelicSinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for NewRelicSinkError {
    fn description(&self) -> &str {
        &self.message
    }
}

impl From<std::io::Error> for NewRelicSinkError {
    fn from(error: std::io::Error) -> Self {
        Self::new(&error.to_string())
    }
}

impl From<NewRelicSinkError> for std::io::Error {
    fn from(error: NewRelicSinkError) -> Self {
        Self::new(std::io::ErrorKind::Other, error)
    }
}

struct NewRelicRequestBuilder {
    encoder: NewRelicEncoder,
    compression: Compression,
    credentials: Arc<NewRelicCredentials>,
}

impl RequestBuilder<Vec<Event>> for NewRelicRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = NewRelicEncoder;
    type Payload = Bytes;
    type Request = NewRelicApiRequest;
    type Error = NewRelicSinkError;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut input: Vec<Event>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let builder = RequestMetadataBuilder::from_events(&input);
        let finalizers = input.take_finalizers();

        (finalizers, builder, input)
    }

    fn build_request(
        &self,
        finalizers: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        NewRelicApiRequest {
            metadata,
            finalizers,
            credentials: Arc::clone(&self.credentials),
            payload: payload.into_payload(),
            compression: self.compression,
        }
    }
}

pub struct NewRelicSink<S> {
    pub service: S,
    pub encoder: NewRelicEncoder,
    pub credentials: Arc<NewRelicCredentials>,
    pub compression: Compression,
    pub batcher_settings: BatcherSettings,
}

impl<S> NewRelicSink<S>
where
    S: Service<NewRelicApiRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder = NewRelicRequestBuilder {
            encoder: self.encoder,
            compression: self.compression,
            credentials: Arc::clone(&self.credentials),
        };
        let protocol = get_http_scheme_from_uri(&self.credentials.get_uri());

        input
            .batched(self.batcher_settings.as_byte_size_config())
            .request_builder(default_request_builder_concurrency_limit(), request_builder)
            .filter_map(
                |request: Result<NewRelicApiRequest, NewRelicSinkError>| async move {
                    match request {
                        Err(error) => {
                            emit!(SinkRequestBuildError { error });
                            None
                        }
                        Ok(req) => Some(req),
                    }
                },
            )
            .into_driver(self.service)
            .protocol(protocol)
            .run()
            .await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for NewRelicSink<S>
where
    S: Service<NewRelicApiRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
