use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;

use futures::{stream::BoxStream, StreamExt};
use pulsar::{Error as PulsarError, Pulsar, TokioExecutor};
use serde::Serialize;
use snafu::{ResultExt, Snafu};
use tower::ServiceBuilder;
use vector_buffers::EventCount;
use vector_common::byte_size_of::ByteSizeOf;
use vector_core::event::LogEvent;
use vector_core::sink::StreamSink;

use crate::sinks::pulsar::config::PulsarSinkConfig;
use crate::sinks::pulsar::encoder::PulsarEncoder;
use crate::sinks::pulsar::request_builder::PulsarRequestBuilder;
use crate::sinks::pulsar::service::{PulsarRetryLogic, PulsarService};
use crate::sinks::pulsar::util;
use crate::sinks::util::{
    ServiceBuilderExt, SinkBuilderExt, TowerRequestConfig, TowerRequestSettings,
};
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
    request_settings: TowerRequestSettings,
    config: PulsarSinkConfig,
    topic_template: Template,
}

/// Stores the event together with the extracted keys, topics, etc
/// This is passed into the `RequestBuilder` which then splits it out into the event
/// and metadata containing the keys, and metadata
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
            + self.key.as_ref().map_or(0, |bytes| bytes.clone().size_of())
            + self.properties.as_ref().map_or(0, |props| {
                props
                    .iter()
                    .map(|(key, val)| key.capacity() + val.size_of())
                    .sum()
            })
    }
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
        let request_settings = config.request.unwrap_with(&TowerRequestConfig::default());
        let encoder = Encoder::<()>::new(serializer);
        let service = PulsarService::new(client, producer_opts, None);
        let topic = config.topic.clone();

        Ok(PulsarSink {
            config,
            transformer,
            request_settings,
            encoder,
            service,
            topic_template: Template::try_from(topic).context(TopicTemplateSnafu)?,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let service = ServiceBuilder::new()
            .settings(self.request_settings, PulsarRetryLogic)
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
                match request {
                    Err(e) => {
                        error!("Failed to build Pulsar request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
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
