use crate::sinks::util::{StreamSink, SinkBuilderExt, Compression};
use crate::event::Event;
use futures::stream::BoxStream;
use async_trait::async_trait;
use super::config::KafkaSinkConfig;
use vector_core::buffers::Acker;
use std::num::NonZeroUsize;
use crate::sinks::kafka::request_builder::KafkaRequestBuilder;
use crate::sinks::util::encoding::EncodingConfig;
use crate::sinks::kafka::config::{Encoding, create_producer};
use crate::sinks::kafka::service::{KafkaService};
use futures::StreamExt;
use super::config::KafkaRole;

pub struct KafkaSink {
    encoding: EncodingConfig<Encoding>,
    acker: Acker,
    service: KafkaService,
    topic: String,
    key_field: Option<String>,
    headers_field: Option<String>,
}

impl KafkaSink {
    pub(crate) fn new(config: KafkaSinkConfig, acker: Acker) -> crate::Result<Self> {

        let producer_config = config.to_rdkafka(KafkaRole::Producer)?;
        let producer = create_producer(producer_config)?;

        Ok(KafkaSink {
            headers_field: config.headers_field,
            encoding: config.encoding,
            acker,
            service: KafkaService::new(producer),
            topic: config.topic,
            key_field: config.key_field,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder_concurrency_limit = NonZeroUsize::new(50);

        let request_builder = KafkaRequestBuilder{
            topic: self.topic,
            key_field: self.key_field,
            headers_field: self.headers_field
        };

        let sink = input
            .request_builder(
                request_builder_concurrency_limit,
                request_builder,
                self.encoding,
                //TODO: This doesn't seem like it would work with Kafka?
                Compression::None
            )
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("failed to build Kafka request: {:?}", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service, self.acker);
        sink.run().await
    }
}

#[async_trait]
impl StreamSink for KafkaSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}


