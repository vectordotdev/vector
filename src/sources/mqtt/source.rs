use itertools::Itertools;
use rumqttc::v5::{
    AsyncClient as AsyncClientV5, Event as MqttEventV5,
    mqttbytes::v5::{Filter as FilterV5, Packet as PacketV5, Publish as PublishV5},
};
use rumqttc::{
    AsyncClient as AsyncClientV3, Event as MqttEventV3, Incoming as IncomingV3,
    Publish as PublishV3, QoS as QoSV3, SubscribeFilter,
};
use vector_lib::{
    codecs::Decoder,
    config::{LegacyKey, LogNamespace},
    event::{BatchStatus, Value},
    internal_event::EventsReceived,
    lookup::path,
};

use crate::{
    SourceSender,
    common::mqtt::{MqttClient, MqttConnector, MqttEventLoop},
    event::{BatchNotifier, Event},
    internal_events::{
        ConnectionOpen, EndpointBytesReceived, MqttAckError, MqttConnectionError,
        MqttConnectionShutdown, MqttDirection, MqttSubscribeError, OpenGauge, StreamClosedError,
    },
    serde::OneOrMany,
    shutdown::ShutdownSignal,
    sources::{mqtt::MqttSourceConfig, util},
};

pub struct MqttSource {
    connector: MqttConnector,
    decoder: Decoder,
    log_namespace: LogNamespace,
    config: MqttSourceConfig,
    acknowledgements: bool,
}

impl MqttSource {
    pub fn new(
        connector: MqttConnector,
        decoder: Decoder,
        log_namespace: LogNamespace,
        config: MqttSourceConfig,
        acknowledgements: bool,
    ) -> crate::Result<Self> {
        Ok(Self {
            connector,
            decoder,
            log_namespace,
            config,
            acknowledgements,
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

        let _open_token = OpenGauge::new().open(|count| emit!(ConnectionOpen { count }));

        let result = match (client, eventloop) {
            (MqttClient::V311(client), MqttEventLoop::V311(eventloop)) => {
                self.run_v3(client, *eventloop, &mut out, shutdown).await
            }
            (MqttClient::V5(client), MqttEventLoop::V5(eventloop)) => {
                self.run_v5(client, *eventloop, &mut out, shutdown).await
            }
            _ => unreachable!("client and event loop protocol versions must match"),
        };

        emit!(MqttConnectionShutdown);
        result
    }

    async fn subscribe_v3(&self, client: &rumqttc::AsyncClient) -> Result<(), ()> {
        match &self.config.topic {
            OneOrMany::One(topic) => {
                if let Err(error) = client.subscribe(topic, QoSV3::AtLeastOnce).await {
                    emit!(MqttSubscribeError {
                        topic: topic.clone(),
                        error: error.to_string(),
                    });
                    return Err(());
                }
            }
            OneOrMany::Many(topics) => {
                if let Err(error) = client
                    .subscribe_many(
                        topics
                            .iter()
                            .cloned()
                            .map(|topic| SubscribeFilter::new(topic, QoSV3::AtLeastOnce)),
                    )
                    .await
                {
                    emit!(MqttSubscribeError {
                        topic: topics.join(","),
                        error: error.to_string(),
                    });
                    return Err(());
                }
            }
        }
        Ok(())
    }

    async fn subscribe_v5(&self, client: &rumqttc::v5::AsyncClient) -> Result<(), ()> {
        match &self.config.topic {
            OneOrMany::One(topic) => {
                if let Err(error) = client
                    .subscribe(topic, rumqttc::v5::mqttbytes::QoS::AtLeastOnce)
                    .await
                {
                    emit!(MqttSubscribeError {
                        topic: topic.clone(),
                        error: error.to_string(),
                    });
                    return Err(());
                }
            }
            OneOrMany::Many(topics) => {
                if let Err(error) = client
                    .subscribe_many(topics.iter().cloned().map(|topic| {
                        FilterV5::new(topic, rumqttc::v5::mqttbytes::QoS::AtLeastOnce)
                    }))
                    .await
                {
                    emit!(MqttSubscribeError {
                        topic: topics.join(","),
                        error: error.to_string(),
                    });
                    return Err(());
                }
            }
        }
        Ok(())
    }

    async fn run_v3(
        &self,
        client: AsyncClientV3,
        mut eventloop: rumqttc::EventLoop,
        out: &mut SourceSender,
        shutdown: ShutdownSignal,
    ) -> Result<(), ()> {
        loop {
            tokio::select! {
                _ = shutdown.clone() => return Ok(()),
                // If `poll()` returns an error there is currently no way to tie it back
                // to a specific event, which limits delivery-guarantee accuracy until
                // https://github.com/bytebeamio/rumqtt/issues/349 is resolved.
                mqtt_event = eventloop.poll() => {
                    match mqtt_event {
                        Ok(MqttEventV3::Incoming(IncomingV3::Publish(publish))) => {
                            self.process_message_v3(&client, publish, out).await;
                        }
                        Ok(_) => {}
                        Err(error) => {
                            emit!(MqttConnectionError::V311 {
                                direction: MqttDirection::Source,
                                error,
                            });
                        }
                    }
                }
            }
        }
    }

    async fn run_v5(
        &self,
        client: AsyncClientV5,
        mut eventloop: rumqttc::v5::EventLoop,
        out: &mut SourceSender,
        shutdown: ShutdownSignal,
    ) -> Result<(), ()> {
        loop {
            tokio::select! {
                _ = shutdown.clone() => return Ok(()),
                // See run_v3 for the rumqttc poll-error correlation caveat:
                // https://github.com/bytebeamio/rumqtt/issues/349
                mqtt_event = eventloop.poll() => {
                    match mqtt_event {
                        Ok(MqttEventV5::Incoming(PacketV5::Publish(publish))) => {
                            self.process_message_v5(&client, publish, out).await;
                        }
                        Ok(_) => {}
                        Err(error) => {
                            emit!(MqttConnectionError::V5 {
                                direction: MqttDirection::Source,
                                error,
                            });
                        }
                    }
                }
            }
        }
    }

    async fn process_message_v3(
        &self,
        client: &AsyncClientV3,
        publish: PublishV3,
        out: &mut SourceSender,
    ) {
        emit!(EndpointBytesReceived {
            byte_size: publish.payload.len(),
            protocol: "mqtt",
            endpoint: &self.connector.broker_address(),
        });
        let events_received = register!(EventsReceived);

        let (batch, batch_receiver) = BatchNotifier::maybe_new_with_receiver(self.acknowledgements);
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
            return;
        }

        if let Some(receiver) = batch_receiver {
            let client = client.clone();
            crate::spawn_in_current_span(async move {
                if receiver.await == BatchStatus::Delivered
                    && let Err(error) = client.ack(&publish).await
                {
                    emit!(MqttAckError {
                        error: error.to_string(),
                    });
                }
            });
        }
    }

    async fn process_message_v5(
        &self,
        client: &AsyncClientV5,
        publish: PublishV5,
        out: &mut SourceSender,
    ) {
        emit!(EndpointBytesReceived {
            byte_size: publish.payload.len(),
            protocol: "mqtt",
            endpoint: &self.connector.broker_address(),
        });
        let events_received = register!(EventsReceived);

        let (batch, batch_receiver) = BatchNotifier::maybe_new_with_receiver(self.acknowledgements);
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
            return;
        }

        if let Some(receiver) = batch_receiver {
            let client = client.clone();
            crate::spawn_in_current_span(async move {
                if receiver.await == BatchStatus::Delivered
                    && let Err(error) = client.ack(&publish).await
                {
                    emit!(MqttAckError {
                        error: error.to_string(),
                    });
                }
            });
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

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use rumqttc::v5::mqttbytes::v5::{Publish as PublishV5, PublishProperties};
    use rumqttc::{Publish as PublishV3, QoS as QoSV3};
    use vector_lib::codecs::decoding::{Deserializer, Framer};
    use vector_lib::codecs::{BytesDecoderConfig, BytesDeserializerConfig};
    use vector_lib::config::LogNamespace;
    use vector_lib::event::{Event, LogEvent, Metric, MetricKind, MetricValue};
    use vector_lib::lookup::{lookup_v2::OptionalValuePath, owned_value_path};

    use super::MqttSource;
    use crate::codecs::Decoder;
    use crate::common::mqtt::{MqttCommonConfig, MqttProtocolVersion, build_connector};
    use crate::sources::mqtt::MqttSourceConfig;

    fn make_source(common: MqttCommonConfig) -> MqttSource {
        let connector = build_connector(&common, "vectorSource", false, false).unwrap();
        // `MqttSourceConfig::default()` leaves the key fields without paths (serde defaults
        // are only applied during deserialization), so set them explicitly for tests.
        let config = MqttSourceConfig {
            common,
            topic_key: OptionalValuePath::from(owned_value_path!("topic")),
            protocol_version_key: OptionalValuePath::from(owned_value_path!("protocol_version")),
            content_type_key: OptionalValuePath::from(owned_value_path!("content_type")),
            response_topic_key: OptionalValuePath::from(owned_value_path!("response_topic")),
            correlation_data_key: OptionalValuePath::from(owned_value_path!("correlation_data")),
            payload_format_indicator_key: OptionalValuePath::from(owned_value_path!(
                "payload_format_indicator"
            )),
            message_expiry_interval_key: OptionalValuePath::from(owned_value_path!(
                "message_expiry_interval"
            )),
            user_properties_key: OptionalValuePath::from(owned_value_path!("user_properties")),
            ..Default::default()
        };
        let decoder = Decoder::new(
            Framer::Bytes(BytesDecoderConfig::new().build()),
            Deserializer::Bytes(BytesDeserializerConfig::new().build()),
        );
        MqttSource::new(connector, decoder, LogNamespace::Legacy, config, false).unwrap()
    }

    fn v3_publish(topic: &str) -> PublishV3 {
        PublishV3::new(topic, QoSV3::AtLeastOnce, Vec::<u8>::new())
    }

    fn v5_publish(topic: &str) -> PublishV5 {
        PublishV5::new(
            topic,
            rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            Vec::<u8>::new(),
            None,
        )
    }

    fn metric_event() -> Event {
        Event::Metric(Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        ))
    }

    #[test]
    fn apply_metadata_v3_sets_topic_and_protocol_version() {
        let source = make_source(MqttCommonConfig {
            client_id: Some("test".into()),
            ..Default::default()
        });
        let mut event = Event::Log(LogEvent::default());
        source.apply_metadata_v3(&v3_publish("sensors/room1"), &mut event);

        let log = event.as_log();
        assert_eq!(
            log.get("topic").and_then(|v| v.as_str()).as_deref(),
            Some("sensors/room1"),
        );
        assert_eq!(
            log.get("protocol_version")
                .and_then(|v| v.as_str())
                .as_deref(),
            Some("v311"),
        );
    }

    #[test]
    fn apply_metadata_v3_no_op_on_non_log_events() {
        let source = make_source(MqttCommonConfig {
            client_id: Some("test".into()),
            ..Default::default()
        });
        let mut event = metric_event();
        // Should not panic on a metric event.
        source.apply_metadata_v3(&v3_publish("t"), &mut event);
    }

    #[test]
    fn apply_metadata_v5_sets_topic_and_protocol_version() {
        let source = make_source(MqttCommonConfig {
            client_id: Some("test".into()),
            protocol_version: MqttProtocolVersion::V5,
            ..Default::default()
        });
        let mut event = Event::Log(LogEvent::default());
        source.apply_metadata_v5(&v5_publish("telemetry/device-1"), &mut event);

        let log = event.as_log();
        assert_eq!(
            log.get("topic").and_then(|v| v.as_str()).as_deref(),
            Some("telemetry/device-1"),
        );
        assert_eq!(
            log.get("protocol_version")
                .and_then(|v| v.as_str())
                .as_deref(),
            Some("v5"),
        );
    }

    #[test]
    fn apply_metadata_v5_sets_all_publish_properties() {
        let source = make_source(MqttCommonConfig {
            client_id: Some("test".into()),
            protocol_version: MqttProtocolVersion::V5,
            ..Default::default()
        });
        let mut publish = v5_publish("t");
        publish.properties = Some(PublishProperties {
            payload_format_indicator: Some(1),
            message_expiry_interval: Some(300),
            content_type: Some("application/json".into()),
            response_topic: Some("responses/abc".into()),
            correlation_data: Some(Bytes::from_static(&[1, 2, 3])),
            user_properties: vec![("k".into(), "v".into())],
            topic_alias: None,
            subscription_identifiers: Vec::new(),
        });
        let mut event = Event::Log(LogEvent::default());
        source.apply_metadata_v5(&publish, &mut event);

        let log = event.as_log();
        assert_eq!(
            log.get("content_type").and_then(|v| v.as_str()).as_deref(),
            Some("application/json"),
        );
        assert_eq!(
            log.get("response_topic")
                .and_then(|v| v.as_str())
                .as_deref(),
            Some("responses/abc"),
        );
        assert_eq!(
            log.get("correlation_data").and_then(|v| v.as_bytes()),
            Some(&Bytes::from_static(&[1, 2, 3])),
        );
        assert_eq!(
            log.get("payload_format_indicator")
                .and_then(|v| v.as_integer()),
            Some(1),
        );
        assert_eq!(
            log.get("message_expiry_interval")
                .and_then(|v| v.as_integer()),
            Some(300),
        );
        assert_eq!(
            log.get("user_properties")
                .and_then(|v| v.as_array())
                .map(<[_]>::len),
            Some(1),
        );
    }

    #[test]
    fn apply_metadata_v5_skips_unset_optional_properties() {
        let source = make_source(MqttCommonConfig {
            client_id: Some("test".into()),
            protocol_version: MqttProtocolVersion::V5,
            ..Default::default()
        });
        let mut event = Event::Log(LogEvent::default());
        source.apply_metadata_v5(&v5_publish("t"), &mut event);

        let log = event.as_log();
        assert!(log.get("content_type").is_none());
        assert!(log.get("response_topic").is_none());
        assert!(log.get("correlation_data").is_none());
        assert!(log.get("payload_format_indicator").is_none());
        assert!(log.get("message_expiry_interval").is_none());
        assert!(log.get("user_properties").is_none());
        assert_eq!(
            log.get("protocol_version")
                .and_then(|v| v.as_str())
                .as_deref(),
            Some("v5"),
        );
    }

    #[test]
    fn apply_metadata_v5_no_op_on_non_log_events() {
        let source = make_source(MqttCommonConfig {
            client_id: Some("test".into()),
            protocol_version: MqttProtocolVersion::V5,
            ..Default::default()
        });
        let mut event = metric_event();
        source.apply_metadata_v5(&v5_publish("t"), &mut event);
    }
}
