use std::{fmt::Debug, io};

use bytes::Bytes;
use vector_lib::codecs::{encoding::Framer, CharacterDelimitedEncoder, JsonSerializerConfig};

use crate::sinks::prelude::*;

use super::service::AzureLogsIngestionRequest;

pub struct AzureLogsIngestionSink<S> {
    batch_settings: BatcherSettings,
    encoding: JsonEncoding,
    service: S,
    protocol: String,
}

impl<S> AzureLogsIngestionSink<S>
where
    S: Service<AzureLogsIngestionRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    pub fn new(
        batch_settings: BatcherSettings,
        transformer: Transformer,
        service: S,
        protocol: String,
    ) -> Self {
        Self {
            batch_settings,
            encoding: JsonEncoding::new(transformer),
            service,
            protocol,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .batched(self.batch_settings.as_byte_size_config())
            .request_builder(
                default_request_builder_concurrency_limit(),
                AzureLogsIngestionRequestBuilder {
                    encoding: self.encoding,
                },
            )
            .filter_map(|request| async {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .protocol(self.protocol.clone())
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl<S> StreamSink<Event> for AzureLogsIngestionSink<S>
where
    S: Service<AzureLogsIngestionRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

/// Customized encoding specific to the Azure Logs Ingestion sink, as the API does not support full
/// 9-digit nanosecond precision timestamps.
#[derive(Clone, Debug)]
pub(super) struct JsonEncoding {
    encoder: (Transformer, Encoder<Framer>),
}

impl JsonEncoding {
    pub fn new(transformer: Transformer) -> Self {
        Self {
            encoder: (
                transformer,
                Encoder::<Framer>::new(
                    CharacterDelimitedEncoder::new(b',').into(),
                    JsonSerializerConfig::default().build().into(),
                ),
            ),
        }
    }
}

impl crate::sinks::util::encoding::Encoder<Vec<Event>> for JsonEncoding {
    fn encode_input(
        &self,
        input: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        self.encoder.encode_input(input, writer)
    }
}

struct AzureLogsIngestionRequestBuilder {
    encoding: JsonEncoding,
}

impl RequestBuilder<Vec<Event>> for AzureLogsIngestionRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = JsonEncoding;
    type Payload = Bytes;
    type Request = AzureLogsIngestionRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }

    fn split_input(
        &self,
        mut events: Vec<Event>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        (finalizers, builder, events)
    }

    fn build_request(
        &self,
        finalizers: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        AzureLogsIngestionRequest {
            body: payload.into_payload(),
            finalizers,
            metadata: request_metadata,
        }
    }
}
