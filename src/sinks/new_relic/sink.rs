use super::{
    Encoding, EventsApiModel, LogsApiModel, MetricsApiModel, NewRelicApi, NewRelicApiModel,
    NewRelicApiRequest, NewRelicCredentials,
};
use crate::{
    event::Event,
    sinks::util::{
        builder::SinkBuilderExt, encoding::EncodingConfigFixed, Compression, RequestBuilder,
        StreamSink,
    },
};
use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use std::{convert::TryFrom, fmt::Debug, num::NonZeroUsize, sync::Arc};
use tower::Service;
use vector_core::{
    buffers::Acker,
    event::{EventFinalizers, Finalizable},
    stream::{BatcherSettings, DriverResponse},
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
    encoding: EncodingConfigFixed<Encoding>,
    compression: Compression,
    credentials: Arc<NewRelicCredentials>,
}

impl RequestBuilder<Vec<Event>> for NewRelicRequestBuilder {
    type Metadata = (Arc<NewRelicCredentials>, usize, EventFinalizers);
    type Events = Result<NewRelicApiModel, Self::Error>;
    type Encoder = EncodingConfigFixed<Encoding>;
    type Payload = Vec<u8>;
    type Request = NewRelicApiRequest;
    type Error = NewRelicSinkError;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }

    fn split_input(&self, mut input: Vec<Event>) -> (Self::Metadata, Self::Events) {
        let events_len = input.len();
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
        let metadata = (Arc::clone(&self.credentials), events_len, finalizers);
        (metadata, api_model)
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (_credentials, events_len, finalizers) = metadata;
        NewRelicApiRequest {
            batch_size: events_len,
            finalizers,
            credentials: Arc::clone(&self.credentials),
            payload,
            compression: self.compression,
        }
    }
}

pub struct NewRelicSink<S> {
    pub service: S,
    pub acker: Acker,
    pub encoding: EncodingConfigFixed<Encoding>,
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
            encoding: self.encoding,
            compression: self.compression,
            credentials: Arc::clone(&self.credentials),
        };

        let sink = input
            .batched(self.batcher_settings.into_byte_size_config())
            .request_builder(builder_limit, request_builder)
            .filter_map(
                |request: Result<NewRelicApiRequest, NewRelicSinkError>| async move {
                    match request {
                        Err(e) => {
                            error!("Failed to build New Relic request: {:?}.", e);
                            None
                        }
                        Ok(req) => Some(req),
                    }
                },
            )
            .into_driver(self.service, self.acker);

        sink.run().await
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
