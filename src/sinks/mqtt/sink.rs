use std::{collections::VecDeque, fmt::Debug};

use async_trait::async_trait;
use bytes::BytesMut;
use futures::{pin_mut, stream::BoxStream, Stream, StreamExt};
use rumqttc::{AsyncClient, ClientError, ConnectionError, EventLoop, MqttOptions};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::Encoder as _;
use vector_common::internal_event::{ByteSize, CountByteSize, Output, Protocol};
use vector_core::{
    event::EventFinalizers,
    internal_event::{BytesSent, EventsSent, InternalEventHandle},
    ByteSizeOf,
};

use crate::sinks::mqtt::config::{ConfigurationError, MqttQoS};
use crate::{
    codecs::{Encoder, Transformer},
    emit,
    event::{Event, EventStatus, Finalizable},
    internal_events::TemplateRenderingError,
    internal_events::{ConnectionOpen, MqttClientError, MqttConnectionError, OpenGauge},
    sinks::mqtt::config::MqttSinkConfig,
    sinks::util::StreamSink,
    template::{Template, TemplateParseError},
    tls::TlsError,
};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum MqttError {
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate { source: TemplateParseError },
    #[snafu(display("MQTT connection error: {}", source))]
    Connection { source: ConnectionError },
    #[snafu(display("TLS error: {}", source))]
    Tls { source: TlsError },
    #[snafu(display("MQTT client error: {}", source))]
    Client { source: ClientError },
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
    finalizers_queue: VecDeque<EventFinalizers>,
    quality_of_service: MqttQoS,
}

impl MqttSink {
    pub fn new(config: &MqttSinkConfig, connector: MqttConnector) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);
        let finalizers_queue = VecDeque::new();

        Ok(Self {
            transformer,
            encoder,
            connector,
            finalizers_queue,
            quality_of_service: config.quality_of_service,
        })
    }

    /// outgoing events main loop
    async fn handle_events<I>(
        &mut self,
        input: &mut I,
        client: &mut AsyncClient,
        connection: &mut EventLoop,
    ) -> Result<(), ()>
    where
        I: Stream<Item = Event> + Unpin,
    {
        let events_sent = register!(EventsSent::from(Output(None)));
        let bytes_sent = register!(BytesSent::from(Protocol("mqtt".into())));

        loop {
            tokio::select! {
                // handle connection errors
                msg = connection.poll() => {
                    match msg {
                        Ok(rumqttc::Event::Outgoing(rumqttc::Outgoing::PubRel(_))) => {
                            // publish has been acknowledged by the MQTT server
                            if let Some(finalizers) = self.finalizers_queue.pop_front() {
                                finalizers.update_status(EventStatus::Delivered);
                            }
                        }
                        Ok(_) => {}
                        Err(error) => {
                            emit!(MqttConnectionError { error });
                            return Err(());
                        }
                    }
                },

                // handle outgoing events
                event = input.next() => {
                    let mut event = if let Some(event) = event {
                        event
                    } else {
                        break;
                    };

                    let finalizers = event.take_finalizers();

                    let topic = match self.connector.topic.render_string(&event) {
                        Ok(topic) => topic,
                        Err(error) => {
                            emit!(TemplateRenderingError {
                                error,
                                field: Some("topic"),
                                drop_event: true,
                            });
                            finalizers.update_status(EventStatus::Errored);
                            continue;
                        }
                    };

                    self.transformer.transform(&mut event);

                    let event_byte_size = event.size_of();

                    let mut bytes = BytesMut::new();
                    let message = match self.encoder.encode(event, &mut bytes) {
                        Ok(()) => {
                            bytes.to_vec()
                        }
                        Err(_) => {
                            finalizers.update_status(EventStatus::Errored);
                            continue;
                        }
                    };
                    let message_len = message.len();

                    let retain = false;
                    match client.publish(&topic, self.quality_of_service.into(), retain, message).await {
                        Ok(()) => {
                            events_sent.emit(CountByteSize(1, event_byte_size));
                            bytes_sent.emit(ByteSize(message_len));

                            self.finalizers_queue.push_back(finalizers);
                        }
                        Err(error) => {
                            emit!(MqttClientError { error });
                            finalizers.update_status(EventStatus::Errored);
                            return Err(());
                        }
                    }
                },

                else => break,
            }
        }

        Ok(())
    }
}

#[async_trait]
impl StreamSink<Event> for MqttSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let input = input.fuse().peekable();
        pin_mut!(input);

        let (client, connection) = self.connector.connect();
        pin_mut!(client);
        pin_mut!(connection);
        while input.as_mut().peek().await.is_some() {
            let _open_token = OpenGauge::new().open(|count| emit!(ConnectionOpen { count }));

            let _result = self
                .handle_events(&mut input, &mut client, &mut connection)
                .await;
        }

        let _ = client.disconnect().await;

        Ok(())
    }
}
