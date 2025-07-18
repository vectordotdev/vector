use std::{fmt::Debug, io};

use bytes::Bytes;
use vector_lib::codecs::{encoding::Framer, CharacterDelimitedEncoder, JsonSerializerConfig};
use vector_lib::lookup::{OwnedValuePath, PathPrefix};

use crate::sinks::prelude::*;

use super::service::AzureMonitorLogsRequest;

pub struct AzureMonitorLogsSink<S> {
    batch_settings: BatcherSettings,
    encoding: JsonEncoding,
    service: S,
    protocol: String,
}

impl<S> AzureMonitorLogsSink<S>
where
    S: Service<AzureMonitorLogsRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    pub fn new(
        batch_settings: BatcherSettings,
        transformer: Transformer,
        service: S,
        time_generated_key: Option<OwnedValuePath>,
        protocol: String,
    ) -> Self {
        Self {
            batch_settings,
            encoding: JsonEncoding::new(transformer, time_generated_key),
            service,
            protocol,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .batched(self.batch_settings.as_byte_size_config())
            .request_builder(
                default_request_builder_concurrency_limit(),
                AzureMonitorLogsRequestBuilder {
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
impl<S> StreamSink<Event> for AzureMonitorLogsSink<S>
where
    S: Service<AzureMonitorLogsRequest> + Send + 'static,
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

/// Customized encoding specific to the Azure Monitor Logs sink, as the API does not support full
/// 9-digit nanosecond precision timestamps.
#[derive(Clone, Debug)]
pub(super) struct JsonEncoding {
    time_generated_key: Option<OwnedValuePath>,
    encoder: (Transformer, Encoder<Framer>),
}

impl JsonEncoding {
    pub fn new(transformer: Transformer, time_generated_key: Option<OwnedValuePath>) -> Self {
        Self {
            time_generated_key,
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
        mut input: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        for event in input.iter_mut() {
            let log = event.as_mut_log();

            // `.remove_timestamp()` will return the `timestamp` value regardless of location in Event or
            // Metadata, the following `insert()` ensures it's encoded in the request.
            let timestamp = if let Some(Value::Timestamp(ts)) = log.remove_timestamp() {
                ts
            } else {
                chrono::Utc::now()
            };

            if let Some(timestamp_key) = &self.time_generated_key {
                log.insert(
                    (PathPrefix::Event, timestamp_key),
                    serde_json::Value::String(
                        timestamp.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                    ),
                );
            }
        }

        self.encoder.encode_input(input, writer)
    }
}

struct AzureMonitorLogsRequestBuilder {
    encoding: JsonEncoding,
}

impl RequestBuilder<Vec<Event>> for AzureMonitorLogsRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = JsonEncoding;
    type Payload = Bytes;
    type Request = AzureMonitorLogsRequest;
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
        AzureMonitorLogsRequest {
            body: payload.into_payload(),
            finalizers,
            metadata: request_metadata,
        }
    }
}
