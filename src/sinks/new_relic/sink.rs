use std::{convert::TryFrom, fmt::Debug, num::NonZeroUsize, sync::Arc};

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::{BoxStream, StreamExt};
use tower::Service;
use vector_common::request_metadata::RequestMetadata;
use vector_core::{
    event::{EventFinalizers, Finalizable},
    stream::{BatcherSettings, DriverResponse},
};

use super::{
    EventsApiModel, LogsApiModel, MetricsApiModel, NewRelicApi, NewRelicApiModel,
    NewRelicApiRequest, NewRelicCredentials, NewRelicEncoder,
};
use crate::{
    codecs::Transformer,
    event::Event,
    http::get_http_scheme_from_uri,
    internal_events::SinkRequestBuildError,
    sinks::util::{
        builder::SinkBuilderExt, metadata::RequestMetadataBuilder, request_builder::EncodeResult,
        Compression, RequestBuilder, StreamSink,
    },
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
    transformer: Transformer,
    encoder: NewRelicEncoder,
    compression: Compression,
    credentials: Arc<NewRelicCredentials>,
}

impl RequestBuilder<Vec<Event>> for NewRelicRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Result<NewRelicApiModel, Self::Error>;
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
        for event in input.iter_mut() {
            self.transformer.transform(event);
        }

        let builder = RequestMetadataBuilder::from_events(&input);

        let finalizers = input.take_finalizers();
        let api_model = || -> Result<NewRelicApiModel, Self::Error> {
            match self.credentials.api {
                NewRelicApi::Events => {
                    Ok(NewRelicApiModel::Events(EventsApiModel::try_from(input)?))
                }
                NewRelicApi::Metrics => {
                    Ok(NewRelicApiModel::Metrics(MetricsApiModel::try_from(input)?))
                }
                NewRelicApi::Logs => Ok(NewRelicApiModel::Logs(LogsApiModel::try_from(input)?)),
            }
        }();

        (finalizers, builder, api_model)
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
    pub transformer: Transformer,
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
        let builder_limit = NonZeroUsize::new(64);
        let request_builder = NewRelicRequestBuilder {
            transformer: self.transformer,
            encoder: self.encoder,
            compression: self.compression,
            credentials: Arc::clone(&self.credentials),
        };
        let protocol = get_http_scheme_from_uri(&self.credentials.get_uri());

        input
            .batched(self.batcher_settings.into_byte_size_config())
            .request_builder(builder_limit, request_builder)
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
