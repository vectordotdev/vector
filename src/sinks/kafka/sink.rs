use std::convert::TryFrom;

use async_trait::async_trait;
use futures::{future, stream::BoxStream, StreamExt};
use rdkafka::{
    consumer::{BaseConsumer, Consumer},
    error::KafkaError,
    producer::FutureProducer,
    ClientConfig,
};
use snafu::{ResultExt, Snafu};
use tokio::time::Duration;
use tower::limit::ConcurrencyLimit;
use vector_core::{buffers::Acker, config::log_schema};

use super::config::{KafkaRole, KafkaSinkConfig};
use crate::{
    event::Event,
    kafka::KafkaStatisticsContext,
    sinks::{
        kafka::{
            config::QUEUED_MIN_MESSAGES, request_builder::KafkaRequestBuilder,
            service::KafkaService,
        },
        util::{
            builder::SinkBuilderExt,
            encoding::{EncodingConfig, StandardEncodings},
            StreamSink,
        },
    },
    template::{Template, TemplateParseError},
};

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("creating kafka producer failed: {}", source))]
    KafkaCreateFailed { source: KafkaError },
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate { source: TemplateParseError },
}

pub struct KafkaSink {
    encoding: EncodingConfig<StandardEncodings>,
    acker: Acker,
    service: KafkaService,
    topic: Template,
    key_field: Option<String>,
    headers_key: Option<String>,
}

pub fn create_producer(
    client_config: ClientConfig,
) -> crate::Result<FutureProducer<KafkaStatisticsContext>> {
    let producer = client_config
        .create_with_context(KafkaStatisticsContext)
        .context(KafkaCreateFailedSnafu)?;
    Ok(producer)
}

impl KafkaSink {
    pub(crate) fn new(config: KafkaSinkConfig, acker: Acker) -> crate::Result<Self> {
        let producer_config = config.to_rdkafka(KafkaRole::Producer)?;
        let producer = create_producer(producer_config)?;

        Ok(KafkaSink {
            headers_key: config.headers_key,
            encoding: config.encoding,
            acker,
            service: KafkaService::new(producer),
            topic: Template::try_from(config.topic).context(TopicTemplateSnafu)?,
            key_field: config.key_field,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // rdkafka will internally retry forever, so we need some limit to prevent this from overflowing
        let service = ConcurrencyLimit::new(self.service, QUEUED_MIN_MESSAGES as usize);
        let request_builder = KafkaRequestBuilder {
            key_field: self.key_field,
            headers_key: self.headers_key,
            topic_template: self.topic,
            encoder: self.encoding,
            log_schema: log_schema(),
        };
        let sink = input
            .filter_map(|event| future::ready(request_builder.build_request(event)))
            .into_driver(service, self.acker);
        sink.run().await
    }
}

pub(crate) async fn healthcheck(config: KafkaSinkConfig) -> crate::Result<()> {
    trace!("Healthcheck started.");
    let client = config.to_rdkafka(KafkaRole::Consumer).unwrap();
    let topic = match Template::try_from(config.topic)
        .context(TopicTemplateSnafu)?
        .render_string(&Event::from(""))
    {
        Ok(topic) => Some(topic),
        Err(error) => {
            warn!(
                message = "Could not generate topic for healthcheck.",
                %error,
            );
            None
        }
    };

    tokio::task::spawn_blocking(move || {
        let consumer: BaseConsumer = client.create().unwrap();
        let topic = topic.as_ref().map(|topic| &topic[..]);

        consumer
            .fetch_metadata(topic, Duration::from_secs(3))
            .map(|_| ())
    })
    .await??;
    trace!("Healthcheck completed.");
    Ok(())
}

#[async_trait]
impl StreamSink<Event> for KafkaSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
