use std::{
    fmt::Debug,
};

use async_trait::async_trait;
use bytes::BytesMut;
use futures::{
    pin_mut,
    stream::BoxStream,
    Stream, StreamExt,
};
use rumqttc::{
    AsyncClient, ClientError, ConnectionError,
    EventLoop, MqttOptions,
    QoS,
};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::Encoder as _;
use vector_core::{
    internal_event::{BytesSent, EventsSent},
    ByteSizeOf,
};

use crate::{
    codecs::{Encoder, Transformer},
    emit,
    event::{Event, EventStatus, Finalizable},
    internal_events::{
        ConnectionOpen, OpenGauge, MqttClientError, MqttConnectionError,
    },
    internal_events::TemplateRenderingError,
    sinks::util::StreamSink,
    sinks::mqtt::config::MqttSinkConfig,
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
}

#[derive(Clone)]
pub struct MqttConnector {
    options: MqttOptions,
    topic: Template,
}

impl MqttConnector {
    pub fn new(options: MqttOptions, topic: String) -> Result<Self, MqttError> {
        let topic = Template::try_from(topic).context(TopicTemplateSnafu)?;
        Ok(Self {
            options,
            topic,
        })
    }

    fn connect(&self) -> (AsyncClient, EventLoop) {
        AsyncClient::new(self.options.clone(), 1024)
    }

    pub async fn healthcheck(&self) -> crate::Result<()> {
        let (client, connection) = self.connect();
        drop(client);
        drop(connection);
        Ok(())
    }
}

pub struct MqttSink {
    transformer: Transformer,
    encoder: Encoder<()>,
    connector: MqttConnector,
}

impl MqttSink {
    pub fn new(
        config: &MqttSinkConfig,
        connector: MqttConnector,
    ) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);

        Ok(Self {
            transformer,
            encoder,
            connector,
        })
    }

    async fn handle_events<I>(
        &mut self,
        input: &mut I,
        client: &mut AsyncClient,
        connection: &mut EventLoop,
    ) -> Result<(), ()>
    where
        I: Stream<Item = Event> + Unpin,
    {
        loop {
            tokio::select! {
                msg = connection.poll() => {
                    if let Err(error) = msg {
                        emit!(MqttConnectionError { error });
                        return Err(());
                    }
                },

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
                    let res = match self.encoder.encode(event, &mut bytes) {
                        Ok(()) => {
                            finalizers.update_status(EventStatus::Delivered);

                            let message = bytes.to_vec();
                            let message_len = message.len();

                            let qos = QoS::ExactlyOnce;
                            let retain = false;
                            client.publish(&topic, qos, retain, message).await.map(|_| {
                                emit!(EventsSent {
                                    count: 1,
                                    byte_size: event_byte_size,
                                    output: None
                                });
                                emit!(BytesSent {
                                    byte_size: message_len,
                                    protocol: "mqtt"
                                });
                            })
                        },
                        Err(_) => {
                            // Error is handled by `Encoder`.
                            finalizers.update_status(EventStatus::Errored);
                            Ok(())
                        }
                    };

                    if let Err(error) = res {
                        emit!(MqttClientError { error });
                        return Err(());
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

        while input.as_mut().peek().await.is_some() {
            let (client, connection) = self.connector.connect();
            pin_mut!(client);
            pin_mut!(connection);

            let _open_token = OpenGauge::new().open(|count| emit!(ConnectionOpen { count }));

            if self
                .handle_events(&mut input, &mut client, &mut connection)
                .await
                .is_ok()
            {
                let _ = client.disconnect().await;
            }
        }

        Ok(())
    }
}
