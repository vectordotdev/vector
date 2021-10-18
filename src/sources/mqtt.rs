use crate::{
    codecs::{self, DecodingConfig, FramingConfig, ParserConfig},
    config::{
        log_schema, DataType, GenerateConfig, SourceConfig, SourceContext, SourceDescription,
    },
    event::Event,
    internal_events::{MqttClientError, MqttConnectionError, MqttEventsReceived},
    serde::{default_decoding, default_framing_message_based},
    shutdown::ShutdownSignal,
    sources::util::TcpError,
    Pipeline,
};
use bytes::Bytes;
use chrono::Utc;
use futures::{pin_mut, stream, SinkExt, Stream, StreamExt};
use rumqttc::v4::Packet;
use rumqttc::{AsyncClient, ConnectionError, Event as MqttEvent, EventLoop, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use tokio::time::Duration;
use tokio_util::codec::FramedRead;

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MqttSourceConfig {
    address: String,
    topic: String,

    /// The max allowed packet size.
    ///
    /// Any packet that exceeds this limit is dropped.
    #[serde(default = "max_packet_size_default")]
    max_packet_size: usize,

    /// The client ID used to connect to the MQTT broker.
    ///
    /// Defaults to "vector", but can be changed, if you want to connect multiple Vector instances
    /// to a single broker, each with their own client state.
    #[serde(default = "client_id_default")]
    client_id: String,

    /// The field used to store the name of the topic the MQTT message belongs to.
    ///
    /// If unset, the topic name won't be embedded in the final event payload.
    #[serde(default)]
    topic_field: Option<String>,

    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    framing: Box<dyn FramingConfig>,
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    decoding: Box<dyn ParserConfig>,
}

fn client_id_default() -> String {
    "vector".to_owned()
}

fn max_packet_size_default() -> usize {
    usize::MAX
}

inventory::submit! {
    SourceDescription::new::<MqttSourceConfig>("mqtt")
}

impl GenerateConfig for MqttSourceConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            address = "mqtt://127.0.0.1:4222"
            topic = "\#",
            "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "mqtt")]
impl SourceConfig for MqttSourceConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let (client, eventloop) = create_subscription(self).await?;
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build()?;

        Ok(Box::pin(mqtt_source(
            client,
            eventloop,
            decoder,
            cx.shutdown,
            cx.out,
            self.topic_field.clone(),
        )))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "mqtt"
    }

    fn resources(&self) -> Vec<crate::config::Resource> {
        Vec::new()
    }
}

async fn create_subscription(config: &MqttSourceConfig) -> crate::Result<(AsyncClient, EventLoop)> {
    let uri = config.address.parse::<http::Uri>()?;
    let host = uri.host().ok_or("missing host")?.to_string();
    let port = uri.port_u16().unwrap_or(1883);

    let mut opts = MqttOptions::new(config.client_id.clone(), host, port);
    opts.set_max_packet_size(config.max_packet_size, usize::MAX);

    let (client, eventloop) = AsyncClient::new(opts, 10);

    client.subscribe(&config.topic, QoS::AtMostOnce).await?;

    Ok((client, eventloop))
}

async fn mqtt_source(
    client: AsyncClient,
    eventloop: EventLoop,
    decoder: codecs::Decoder,
    shutdown: ShutdownSignal,
    mut out: Pipeline,
    topic_field: Option<String>,
) -> Result<(), ()> {
    let stream = get_eventloop_stream(eventloop).take_until(shutdown);
    pin_mut!(stream);

    while let Some(msg) = stream.next().await {
        match msg {
            Ok(event) => handle_mqtt_event(event, &decoder, &mut out, topic_field.as_deref()).await,
            Err(error) => {
                emit!(&MqttConnectionError { error });
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }

    let _ = client.cancel().await.map_err(|error| {
        emit!(&MqttClientError { error });
    });

    Ok(())
}

async fn handle_mqtt_event(
    event: MqttEvent,
    decoder: &codecs::Decoder,
    out: &mut Pipeline,
    topic_field: Option<&str>,
) {
    match event {
        rumqttc::Event::Incoming(packet) => match packet {
            Packet::Publish(publish) => {
                let mut stream = FramedRead::new(publish.payload.as_ref(), decoder.clone());
                while let Some(next) = stream.next().await {
                    match next {
                        Ok((events, byte_size)) => {
                            emit!(&MqttEventsReceived {
                                byte_size,
                                count: events.len()
                            });

                            for mut event in events {
                                if let Event::Log(ref mut log) = event {
                                    log.insert(log_schema().timestamp_key(), Utc::now());
                                    log.insert(log_schema().source_type_key(), Bytes::from("mqtt"));

                                    if let Some(field) = topic_field {
                                        log.insert(field, publish.topic.clone());
                                    }
                                }

                                let _ = out.send(event).await.map_err(|error| {
                                    error!(message = "Error sending to sink.", %error);
                                });
                            }
                        }
                        Err(error) => {
                            // Error is logged by `crate::codecs::Decoder`, no further
                            // handling is needed here.
                            if !error.can_continue() {
                                break;
                            }
                        }
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
}

fn get_eventloop_stream(
    eventloop: EventLoop,
) -> impl Stream<Item = Result<MqttEvent, ConnectionError>> {
    stream::try_unfold(eventloop, |mut eventloop| async move {
        eventloop.poll().await.map(|msg| Some((msg, eventloop)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MqttSourceConfig>();
    }
}

#[cfg(feature = "mqtt-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::test_util::{collect_n, random_string};

    #[tokio::test]
    async fn mqtt_happy() {
        let topic = format!("test-{}", random_string(10));

        let conf = MqttSourceConfig {
            address: "mqtt://127.0.0.1:1833".to_owned(),
            topic: topic.clone(),
            ..Default::default()
        };

        let (client, eventloop) = create_subscription(&conf).await.unwrap();
        let decoder = DecodingConfig::new(conf.framing, conf.decoding)
            .build()
            .unwrap();

        let pub_client = client.clone();

        let (tx, rx) = Pipeline::new_test();
        tokio::spawn(mqtt_source(
            client,
            eventloop,
            decoder,
            ShutdownSignal::noop(),
            tx,
            conf.topic_field,
        ));
        let msg = "my message";
        pub_client.publish(&subject, msg).await.unwrap();

        let events = collect_n(rx, 1).await;
        println!("Received event  {:?}", events[0].as_log());
        assert_eq!(events[0].as_log()[log_schema().message_key()], msg.into());
    }
}
