use itertools::Itertools;
use rumqttc::v5::{
    Event as MqttEventV5,
    mqttbytes::v5::{Filter as FilterV5, Packet as PacketV5, Publish as PublishV5},
};
use rumqttc::{
    Event as MqttEventV3, Incoming as IncomingV3, Publish as PublishV3, QoS as QoSV3,
    SubscribeFilter,
};
use vector_lib::{
    codecs::Decoder,
    config::{LegacyKey, LogNamespace},
    event::Value,
    internal_event::EventsReceived,
    lookup::path,
};

use crate::{
    SourceSender,
    common::mqtt::{MqttClient, MqttConnector, MqttEventLoop},
    event::{BatchNotifier, Event},
    internal_events::{EndpointBytesReceived, StreamClosedError},
    serde::OneOrMany,
    shutdown::ShutdownSignal,
    sources::{mqtt::MqttSourceConfig, util},
};

pub struct MqttSource {
    connector: MqttConnector,
    decoder: Decoder,
    log_namespace: LogNamespace,
    config: MqttSourceConfig,
}

impl MqttSource {
    pub fn new(
        connector: MqttConnector,
        decoder: Decoder,
        log_namespace: LogNamespace,
        config: MqttSourceConfig,
    ) -> crate::Result<Self> {
        Ok(Self {
            connector,
            decoder,
            log_namespace,
            config,
        })
    }

    pub async fn run(self, mut out: SourceSender, shutdown: ShutdownSignal) -> Result<(), ()> {
        let (client, eventloop) = self.connector.connect();

        // Subscribe to topics
        match &client {
            MqttClient::V311(c) => {
                self.subscribe_v3(c).await?;
            }
            MqttClient::V5(c) => {
                self.subscribe_v5(c).await?;
            }
        }

        match eventloop {
            MqttEventLoop::V311(eventloop) => self.run_v3(*eventloop, &mut out, shutdown).await,
            MqttEventLoop::V5(eventloop) => self.run_v5(*eventloop, &mut out, shutdown).await,
        }
    }

    async fn subscribe_v3(&self, client: &rumqttc::AsyncClient) -> Result<(), ()> {
        match &self.config.topic {
            OneOrMany::One(topic) => {
                client
                    .subscribe(topic, QoSV3::AtLeastOnce)
                    .await
                    .map_err(|_| ())?;
            }
            OneOrMany::Many(topics) => {
                client
                    .subscribe_many(
                        topics
                            .iter()
                            .cloned()
                            .map(|topic| SubscribeFilter::new(topic, QoSV3::AtLeastOnce)),
                    )
                    .await
                    .map_err(|_| ())?;
            }
        }
        Ok(())
    }

    async fn subscribe_v5(&self, client: &rumqttc::v5::AsyncClient) -> Result<(), ()> {
        match &self.config.topic {
            OneOrMany::One(topic) => {
                client
                    .subscribe(topic, rumqttc::v5::mqttbytes::QoS::AtLeastOnce)
                    .await
                    .map_err(|_| ())?;
            }
            OneOrMany::Many(topics) => {
                client
                    .subscribe_many(topics.iter().cloned().map(|topic| {
                        FilterV5::new(topic, rumqttc::v5::mqttbytes::QoS::AtLeastOnce)
                    }))
                    .await
                    .map_err(|_| ())?;
            }
        }
        Ok(())
    }

    async fn run_v3(
        &self,
        mut eventloop: rumqttc::EventLoop,
        out: &mut SourceSender,
        shutdown: ShutdownSignal,
    ) -> Result<(), ()> {
        loop {
            tokio::select! {
                _ = shutdown.clone() => return Ok(()),
                mqtt_event = eventloop.poll() => {
                    match mqtt_event {
                        Ok(MqttEventV3::Incoming(IncomingV3::Publish(publish))) => {
                            self.process_message_v3(publish, out).await;
                        }
                        Ok(MqttEventV3::Incoming(
                            IncomingV3::PubAck(_)
                            | IncomingV3::PubRec(_)
                            | IncomingV3::PubComp(_),
                        )) => {
                            // TODO Handle acknowledgement - https://github.com/vectordotdev/vector/issues/21967
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    async fn run_v5(
        &self,
        mut eventloop: rumqttc::v5::EventLoop,
        out: &mut SourceSender,
        shutdown: ShutdownSignal,
    ) -> Result<(), ()> {
        loop {
            tokio::select! {
                _ = shutdown.clone() => return Ok(()),
                mqtt_event = eventloop.poll() => {
                    match mqtt_event {
                        Ok(MqttEventV5::Incoming(PacketV5::Publish(publish))) => {
                            self.process_message_v5(publish, out).await;
                        }
                        Ok(MqttEventV5::Incoming(
                            PacketV5::PubAck(_)
                            | PacketV5::PubRec(_)
                            | PacketV5::PubComp(_),
                        )) => {
                            // TODO Handle acknowledgement
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    async fn process_message_v3(&self, publish: PublishV3, out: &mut SourceSender) {
        emit!(EndpointBytesReceived {
            byte_size: publish.payload.len(),
            protocol: "mqtt",
            endpoint: &self.connector.broker_address(),
        });
        let events_received = register!(EventsReceived);

        let (batch, _batch_receiver) = BatchNotifier::maybe_new_with_receiver(false);
        let decoded = util::decode_message(
            self.decoder.clone(),
            "mqtt",
            &publish.payload,
            None,
            &batch,
            self.log_namespace,
            &events_received,
        )
        .map(|mut event| {
            self.apply_metadata_v3(&publish, &mut event);
            event
        })
        .collect_vec();

        let count = decoded.len();
        if out.send_batch(decoded).await.is_err() {
            emit!(StreamClosedError { count });
        }
    }

    async fn process_message_v5(&self, publish: PublishV5, out: &mut SourceSender) {
        emit!(EndpointBytesReceived {
            byte_size: publish.payload.len(),
            protocol: "mqtt",
            endpoint: &self.connector.broker_address(),
        });
        let events_received = register!(EventsReceived);

        let (batch, _batch_receiver) = BatchNotifier::maybe_new_with_receiver(false);
        let decoded = util::decode_message(
            self.decoder.clone(),
            "mqtt",
            &publish.payload,
            None,
            &batch,
            self.log_namespace,
            &events_received,
        )
        .map(|mut event| {
            self.apply_metadata_v5(&publish, &mut event);
            event
        })
        .collect_vec();

        let count = decoded.len();
        if out.send_batch(decoded).await.is_err() {
            emit!(StreamClosedError { count });
        }
    }

    fn apply_metadata_v3(&self, publish: &PublishV3, event: &mut Event) {
        if let Event::Log(log) = event {
            self.log_namespace.insert_source_metadata(
                MqttSourceConfig::NAME,
                log,
                self.config
                    .topic_key
                    .path
                    .as_ref()
                    .map(LegacyKey::Overwrite),
                path!("topic"),
                publish.topic.clone(),
            );

            self.log_namespace.insert_source_metadata(
                MqttSourceConfig::NAME,
                log,
                self.config
                    .protocol_version_key
                    .path
                    .as_ref()
                    .map(LegacyKey::Overwrite),
                path!("protocol_version"),
                "v311",
            );
        }
    }

    fn apply_metadata_v5(&self, publish: &PublishV5, event: &mut Event) {
        if let Event::Log(log) = event {
            let topic = String::from_utf8_lossy(&publish.topic).into_owned();

            self.log_namespace.insert_source_metadata(
                MqttSourceConfig::NAME,
                log,
                self.config
                    .topic_key
                    .path
                    .as_ref()
                    .map(LegacyKey::Overwrite),
                path!("topic"),
                topic,
            );

            self.log_namespace.insert_source_metadata(
                MqttSourceConfig::NAME,
                log,
                self.config
                    .protocol_version_key
                    .path
                    .as_ref()
                    .map(LegacyKey::Overwrite),
                path!("protocol_version"),
                "v5",
            );

            // Add v5 properties as metadata if present
            if let Some(props) = &publish.properties {
                if let Some(content_type) = &props.content_type {
                    self.log_namespace.insert_source_metadata(
                        MqttSourceConfig::NAME,
                        log,
                        self.config
                            .content_type_key
                            .path
                            .as_ref()
                            .map(LegacyKey::Overwrite),
                        path!("content_type"),
                        content_type.clone(),
                    );
                }

                if let Some(response_topic) = &props.response_topic {
                    self.log_namespace.insert_source_metadata(
                        MqttSourceConfig::NAME,
                        log,
                        self.config
                            .response_topic_key
                            .path
                            .as_ref()
                            .map(LegacyKey::Overwrite),
                        path!("response_topic"),
                        response_topic.clone(),
                    );
                }

                if let Some(correlation_data) = &props.correlation_data {
                    self.log_namespace.insert_source_metadata(
                        MqttSourceConfig::NAME,
                        log,
                        self.config
                            .correlation_data_key
                            .path
                            .as_ref()
                            .map(LegacyKey::Overwrite),
                        path!("correlation_data"),
                        Value::Bytes(bytes::Bytes::copy_from_slice(correlation_data)),
                    );
                }

                if let Some(payload_format) = props.payload_format_indicator {
                    self.log_namespace.insert_source_metadata(
                        MqttSourceConfig::NAME,
                        log,
                        self.config
                            .payload_format_indicator_key
                            .path
                            .as_ref()
                            .map(LegacyKey::Overwrite),
                        path!("payload_format_indicator"),
                        payload_format as i64,
                    );
                }

                if let Some(message_expiry) = props.message_expiry_interval {
                    self.log_namespace.insert_source_metadata(
                        MqttSourceConfig::NAME,
                        log,
                        self.config
                            .message_expiry_interval_key
                            .path
                            .as_ref()
                            .map(LegacyKey::Overwrite),
                        path!("message_expiry_interval"),
                        message_expiry as i64,
                    );
                }

                if !props.user_properties.is_empty() {
                    let user_props = Value::Array(
                        props
                            .user_properties
                            .iter()
                            .map(|(key, value)| {
                                Value::Object(vrl::value::ObjectMap::from([
                                    (vrl::value::KeyString::from("key"), Value::from(key.clone())),
                                    (
                                        vrl::value::KeyString::from("value"),
                                        Value::from(value.clone()),
                                    ),
                                ]))
                            })
                            .collect(),
                    );

                    self.log_namespace.insert_source_metadata(
                        MqttSourceConfig::NAME,
                        log,
                        self.config
                            .user_properties_key
                            .path
                            .as_ref()
                            .map(LegacyKey::Overwrite),
                        path!("user_properties"),
                        user_props,
                    );
                }
            }
        }
    }
}
