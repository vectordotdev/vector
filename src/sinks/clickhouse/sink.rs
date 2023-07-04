use bytes::Bytes;
use codecs::{encoding::Framer, JsonSerializerConfig, NewlineDelimitedEncoderConfig};

use super::service::{ClickhouseRequest, ClickhouseRetryLogic, ClickhouseService};
use crate::{internal_events::SinkRequestBuildError, sinks::prelude::*};

pub struct ClickhouseSink {
    batch_settings: BatcherSettings,
    compression: Compression,
    encoding: (Transformer, Encoder<Framer>),
    service: Svc<ClickhouseService, ClickhouseRetryLogic>,
    protocol: &'static str,
}

impl ClickhouseSink {
    pub fn new(
        batch_settings: BatcherSettings,
        compression: Compression,
        transformer: Transformer,
        service: Svc<ClickhouseService, ClickhouseRetryLogic>,
        protocol: &'static str,
    ) -> Self {
        Self {
            batch_settings,
            compression,
            encoding: (
                transformer,
                Encoder::<Framer>::new(
                    NewlineDelimitedEncoderConfig::default().build().into(),
                    JsonSerializerConfig::default().build().into(),
                ),
            ),
            service,
            protocol,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .batched(self.batch_settings.into_byte_size_config())
            .request_builder(
                None,
                ClickhouseRequestBuilder {
                    compression: self.compression,
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
            .protocol(self.protocol)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for ClickhouseSink {
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

struct ClickhouseRequestBuilder {
    compression: Compression,
    encoding: (Transformer, Encoder<Framer>),
}

impl RequestBuilder<Vec<Event>> for ClickhouseRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = ClickhouseRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
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
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        ClickhouseRequest {
            body: payload.into_payload(),
            compression: self.compression,
            finalizers: metadata,
            metadata: request_metadata,
        }
    }
}
