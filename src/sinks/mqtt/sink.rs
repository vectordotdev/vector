use super::{
    MqttSinkConfig,
    config::MqttQoS,
    request_builder::{MqttEncoder, MqttRequestBuilder},
    service::MqttService,
};
use crate::{
    common::mqtt::{MqttConnector, MqttError, MqttEventLoop},
    internal_events::{
        ConnectionOpen, MqttConnectionError, MqttConnectionShutdown, MqttDirection, OpenGauge,
    },
    sinks::prelude::*,
};
use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};

pub struct MqttSink {
    transformer: Transformer,
    encoder: Encoder<()>,
    connector: MqttConnector,
    topic: Template,
    quality_of_service: MqttQoS,
    retain: bool,
    publish_properties: Option<rumqttc::v5::mqttbytes::v5::PublishProperties>,
}

pub(super) struct MqttEvent {
    pub(super) topic: String,
    pub(super) event: Event,
}

impl MqttSink {
    pub fn new(config: &MqttSinkConfig, connector: MqttConnector) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let topic = config.topic.clone();
        let encoder = Encoder::<()>::new(serializer);

        let publish_properties = config
            .publish_properties
            .as_ref()
            .map(|v5| v5.to_publish_properties())
            .transpose()
            .map_err(|source| MqttError::Configuration { source })?;

        Ok(Self {
            transformer,
            encoder,
            connector,
            topic,
            quality_of_service: config.quality_of_service,
            retain: config.retain,
            publish_properties,
        })
    }

    fn make_mqtt_event(&self, event: Event) -> Option<MqttEvent> {
        let topic = self
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
        let (client, eventloop) = self.connector.connect();

        let _open_token = OpenGauge::new().open(|count| emit!(ConnectionOpen { count }));

        // Spawn the event loop handler based on protocol version. This is necessary to
        // keep the MQTT event loop moving forward.
        //
        // If an error is returned from `poll()` there is currently no way to tie it back
        // to the event that was posted, which means we can't accurately provide
        // delivery guarantees. We need this issue resolved first:
        // https://github.com/bytebeamio/rumqtt/issues/349
        match eventloop {
            MqttEventLoop::V311(mut connection) => {
                crate::spawn_in_current_span(async move {
                    loop {
                        match connection.poll().await {
                            Ok(_) => {}
                            Err(connection_error) => {
                                emit!(MqttConnectionError::V311 {
                                    direction: MqttDirection::Sink,
                                    error: connection_error,
                                });
                            }
                        }
                    }
                });
            }
            MqttEventLoop::V5(mut connection) => {
                crate::spawn_in_current_span(async move {
                    loop {
                        match connection.poll().await {
                            Ok(_) => {}
                            Err(connection_error) => {
                                emit!(MqttConnectionError::V5 {
                                    direction: MqttDirection::Sink,
                                    error: connection_error,
                                });
                            }
                        }
                    }
                });
            }
        }

        let service = ServiceBuilder::new().service(MqttService {
            client,
            quality_of_service: self.quality_of_service,
            retain: self.retain,
            publish_properties: self.publish_properties.clone(),
        });

        let request_builder = MqttRequestBuilder {
            encoder: MqttEncoder {
                encoder: self.encoder.clone(),
                transformer: self.transformer.clone(),
            },
        };

        let result = input
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
            .await;

        emit!(MqttConnectionShutdown);
        result
    }
}

#[async_trait]
impl StreamSink<Event> for MqttSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
