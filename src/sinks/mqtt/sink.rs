use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt};
use rumqttc::{AsyncClient, EventLoop, MqttOptions};
use snafu::{ResultExt, Snafu};
use vector_lib::tls::TlsError;

use crate::internal_events::MqttConnectionError;
use crate::sinks::prelude::*;

use super::{
    config::{ConfigurationError, MqttQoS},
    request_builder::{MqttEncoder, MqttRequestBuilder},
    service::MqttService,
    MqttSinkConfig,
};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum MqttError {
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate { source: TemplateParseError },
    #[snafu(display("TLS error: {}", source))]
    Tls { source: TlsError },
    #[snafu(display("MQTT configuration error: {}", source))]
    Configuration { source: ConfigurationError },
}

#[derive(Clone)]
pub struct MqttConnector {
    options: MqttOptions,
    topic: Template,
}

impl MqttConnector {
    pub fn new(options: MqttOptions, topic: String) -> Result<Self, MqttError> {
        let topic = Template::try_from(topic).context(TopicTemplateSnafu)?;
        Ok(Self { options, topic })
    }

    fn connect(&self) -> (AsyncClient, EventLoop) {
        AsyncClient::new(self.options.clone(), 1024)
    }

    pub async fn healthcheck(&self) -> crate::Result<()> {
        // TODO: Right now there is no way to implement the healthcheck properly: https://github.com/bytebeamio/rumqtt/issues/562
        Ok(())
    }
}

pub struct MqttSink {
    transformer: Transformer,
    encoder: Encoder<()>,
    connector: MqttConnector,
    quality_of_service: MqttQoS,
    retain: bool,
}

pub(super) struct MqttEvent {
    pub(super) topic: String,
    pub(super) event: Event,
}

impl MqttSink {
    pub fn new(config: &MqttSinkConfig, connector: MqttConnector) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);

        Ok(Self {
            transformer,
            encoder,
            connector,
            quality_of_service: config.quality_of_service,
            retain: config.retain,
        })
    }

    fn make_mqtt_event(&self, event: Event) -> Option<MqttEvent> {
        let topic = self
            .connector
            .topic
            .render_string(&event)
            .map_err(|missing_keys| {
                emit!(TemplateRenderingError {
                    error: missing_keys,
                    field: Some("topic"),
                    drop_event: true,
                })
            })
            .ok()?;

        Some(MqttEvent { topic, event })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let (client, mut connection) = self.connector.connect();

        // This is necessary to keep the mqtt event loop moving forward.
        tokio::spawn(async move {
            loop {
                // If an error is returned here there is currently no way to tie this back
                // to the event that was posted which means we can't accurately provide
                // delivery guarantees.
                // We need this issue resolved first:
                // https://github.com/bytebeamio/rumqtt/issues/349
                match connection.poll().await {
                    Ok(_) => {}
                    Err(connection_error) => {
                        emit!(MqttConnectionError {
                            error: connection_error
                        });
                    }
                }
            }
        });

        let service = ServiceBuilder::new().service(MqttService {
            client,
            quality_of_service: self.quality_of_service,
            retain: self.retain,
        });

        let request_builder = MqttRequestBuilder {
            encoder: MqttEncoder {
                encoder: self.encoder.clone(),
                transformer: self.transformer.clone(),
            },
        };

        input
            .filter_map(|event| std::future::ready(self.make_mqtt_event(event)))
            .request_builder(default_request_builder_concurrency_limit(), request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build MQTT request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(service)
            .protocol("mqtt")
            .run()
            .await
    }
}

#[async_trait]
impl StreamSink<Event> for MqttSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
