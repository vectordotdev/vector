use async_trait::async_trait;

use futures::{future, stream::BoxStream, StreamExt};
use pulsar::{Error as PulsarError, Pulsar, TokioExecutor};
use snafu::{ResultExt, Snafu};
use tower::limit::ConcurrencyLimit;
use vector_core::config::log_schema;
use vector_core::event::LogEvent;
use vector_core::sink::StreamSink;

use crate::sinks::pulsar::config::{PulsarSinkConfig, QUEUED_MIN_MESSAGES};
use crate::sinks::pulsar::request_builder::PulsarRequestBuilder;
use crate::sinks::pulsar::service::PulsarService;
use crate::sinks::util::SinkBuilderExt;
use crate::template::{Template, TemplateParseError};
use crate::{
    codecs::{Encoder, Transformer},
    event::Event,
};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub(crate) enum BuildError {
    #[snafu(display("creating pulsar producer failed: {}", source))]
    CreatePulsarSink { source: PulsarError },
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate { source: TemplateParseError },
}

pub(crate) struct PulsarSink {
    transformer: Transformer,
    encoder: Encoder<()>,
    service: PulsarService<TokioExecutor>,
    topic: Template,
    key_field: Option<String>,
    properties_key: Option<String>,
}

pub(crate) async fn healthcheck(config: PulsarSinkConfig) -> crate::Result<()> {
    trace!("Healthcheck started.");
    let client = config.create_pulsar_client().await?;
    let topic = Template::try_from(config.topic)
        .context(TopicTemplateSnafu)?
        .render_string(&LogEvent::from_str_legacy(""))?;
    client.lookup_topic(topic).await?;
    trace!("Healthcheck completed.");
    Ok(())
}

impl PulsarSink {
    pub(crate) fn new(
        client: Pulsar<TokioExecutor>,
        config: PulsarSinkConfig,
    ) -> crate::Result<Self> {
        let producer_opts = config.build_producer_options();
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);
        let service = PulsarService::new(client, producer_opts, None);

        Ok(PulsarSink {
            properties_key: config.properties_key,
            key_field: config.key_field,
            transformer,
            encoder,
            service,
            topic: Template::try_from(config.topic).context(TopicTemplateSnafu)?,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let service = ConcurrencyLimit::new(self.service, QUEUED_MIN_MESSAGES as usize);
        let mut request_builder = PulsarRequestBuilder {
            key_field: self.key_field,
            properties_key: self.properties_key,
            topic_template: self.topic,
            transformer: self.transformer,
            encoder: self.encoder,
            log_schema: log_schema(),
        };
        let sink = input
            .filter_map(|event| future::ready(request_builder.build_request(event)))
            .into_driver(service);
        sink.run().await
    }
}

#[async_trait]
impl StreamSink<Event> for PulsarSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
