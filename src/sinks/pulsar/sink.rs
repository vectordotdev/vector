use async_trait::async_trait;
use bytes::Bytes;
use futures::{stream::BoxStream, StreamExt};
use pulsar::{Error as PulsarError, Pulsar, TokioExecutor};
use serde::Serialize;
use snafu::{ResultExt, Snafu};
use std::collections::HashMap;
use tower::ServiceBuilder;

use super::{
    config::PulsarSinkConfig,
    encoder::PulsarEncoder,
    request_builder::PulsarRequestBuilder,
    service::{PulsarRetryLogic, PulsarService},
    util,
};

use crate::template::{Template, TemplateParseError};
use crate::{
    codecs::{Encoder, Transformer},
    event::Event,
    sinks::util::{ServiceBuilderExt, SinkBuilderExt, TowerRequestConfig},
};
use vector_buffers::EventCount;
use vector_common::byte_size_of::ByteSizeOf;
use vector_core::{
    event::{EstimatedJsonEncodedSizeOf, LogEvent},
    sink::StreamSink,
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
    config: PulsarSinkConfig,
    topic_template: Template,
}

/// Stores the event together with the extracted keys, topics, etc.
/// This is passed into the `RequestBuilder` which then splits it out into the event
/// and metadata containing the keys, and metadata.
/// This event needs to be created prior to building the request so we can filter out
/// any events that error whilst rendering the templates.
#[derive(Serialize)]
pub(super) struct PulsarEvent {
    pub(super) event: Event,
    pub(super) topic: String,
    pub(super) key: Option<Bytes>,
    pub(super) properties: Option<HashMap<String, Bytes>>,
    pub(super) timestamp_millis: Option<i64>,
}

impl EventCount for PulsarEvent {
    fn event_count(&self) -> usize {
        // A PulsarEvent represents one event.
        1
    }
}

impl ByteSizeOf for PulsarEvent {
    fn allocated_bytes(&self) -> usize {
        self.event.size_of()
            + self.topic.size_of()
            + self.key.as_ref().map_or(0, |bytes| bytes.size_of())
            + self.properties.as_ref().map_or(0, |props| {
                props
                    .iter()
                    .map(|(key, val)| key.capacity() + val.size_of())
                    .sum()
            })
    }
}

impl EstimatedJsonEncodedSizeOf for PulsarEvent {
    fn estimated_json_encoded_size_of(&self) -> usize {
        self.event.estimated_json_encoded_size_of()
    }
}

pub(crate) async fn healthcheck(config: PulsarSinkConfig) -> crate::Result<()> {
    let client = config.create_pulsar_client().await?;
    let topic = Template::try_from(config.topic)
        .context(TopicTemplateSnafu)?
        .render_string(&LogEvent::from_str_legacy(""))?;
    client.lookup_topic(topic).await?;
    Ok(())
}

impl PulsarSink {
    pub(crate) async fn new(
        client: Pulsar<TokioExecutor>,
        config: PulsarSinkConfig,
    ) -> crate::Result<Self> {
        let producer_opts = config.build_producer_options();
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);
        let service = PulsarService::new(
            client,
            producer_opts,
            config.producer_name.clone(),
            &config.topic,
        )
        .await?;
        let topic = config.topic.clone();

        Ok(PulsarSink {
            config,
            transformer,
            encoder,
            service,
            topic_template: Template::try_from(topic).context(TopicTemplateSnafu)?,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_config = TowerRequestConfig::default();
        let request_settings = request_config.unwrap_with(&request_config);
        let service = ServiceBuilder::new()
            .settings(request_settings, PulsarRetryLogic)
            .service(self.service);
        let request_builder = PulsarRequestBuilder {
            encoder: PulsarEncoder {
                transformer: self.transformer.clone(),
                encoder: self.encoder.clone(),
            },
        };
        let sink = input
            .filter_map(|event| {
                std::future::ready(util::make_pulsar_event(
                    &self.topic_template,
                    &self.config,
                    event,
                ))
            })
            .request_builder(None, request_builder)
            .filter_map(|request| async move {
                request
                    .map_err(|e| error!("Failed to build Pulsar request: {:?}.", e))
                    .ok()
            })
            .into_driver(service)
            .protocol("tcp");

        sink.run().await
    }
}

#[async_trait]
impl StreamSink<Event> for PulsarSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
