use bytes::Bytes;
use vector_lib::codecs::{encoding::Framer, JsonSerializerConfig, NewlineDelimitedEncoderConfig};

use super::service::{ClickhouseRequest, ClickhouseRetryLogic, ClickhouseService};
use crate::sinks::prelude::*;

use crate::sinks::clickhouse::config::Format;

pub struct ClickhouseSink {
    batch_settings: BatcherSettings,
    compression: Compression,
    encoding: (Transformer, Encoder<Framer>),
    service: Svc<ClickhouseService, ClickhouseRetryLogic>,
    protocol: &'static str,
    database: Template,
    table: Template,
    format: Format,
}

impl ClickhouseSink {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        batch_settings: BatcherSettings,
        compression: Compression,
        transformer: Transformer,
        service: Svc<ClickhouseService, ClickhouseRetryLogic>,
        protocol: &'static str,
        database: Template,
        table: Template,
        format: Format,
    ) -> Self {
        Self {
            batch_settings,
            compression,
            encoding: (
                transformer,
                Encoder::<Framer>::new(
                    NewlineDelimitedEncoderConfig.build().into(),
                    JsonSerializerConfig::default().build().into(),
                ),
            ),
            service,
            protocol,
            database,
            table,
            format,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_settings = self.batch_settings;

        input
            .batched_partitioned(KeyPartitioner::new(self.database, self.table), || {
                batch_settings.as_byte_size_config()
            })
            .filter_map(|(key, batch)| async move { key.map(move |k| (k, batch)) })
            .request_builder(
                default_request_builder_concurrency_limit(),
                ClickhouseRequestBuilder {
                    compression: self.compression,
                    encoding: self.encoding,
                    format: self.format,
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
    format: Format,
}

impl RequestBuilder<(PartitionKey, Vec<Event>)> for ClickhouseRequestBuilder {
    type Metadata = (PartitionKey, EventFinalizers);
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
        input: (PartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (key, mut events) = input;

        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        ((key, finalizers), builder, events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (key, finalizers) = metadata;
        ClickhouseRequest {
            database: key.database,
            table: key.table,
            format: self.format,
            body: payload.into_payload(),
            compression: self.compression,
            finalizers,
            metadata: request_metadata,
        }
    }
}

/// PartitionKey used to partition events by (database, table) pair.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
struct PartitionKey {
    database: String,
    table: String,
}

/// KeyPartitioner that partitions events by (database, table) pair.
struct KeyPartitioner {
    database: Template,
    table: Template,
}

impl KeyPartitioner {
    const fn new(database: Template, table: Template) -> Self {
        Self { database, table }
    }

    fn render(template: &Template, item: &Event, field: &'static str) -> Option<String> {
        template
            .render_string(item)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some(field),
                    drop_event: true,
                });
            })
            .ok()
    }
}

impl Partitioner for KeyPartitioner {
    type Item = Event;
    type Key = Option<PartitionKey>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        let database = Self::render(&self.database, item, "database_key")?;
        let table = Self::render(&self.table, item, "table_key")?;
        Some(PartitionKey { database, table })
    }
}
