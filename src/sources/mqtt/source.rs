use itertools::Itertools;
use rumqttc::{
    AsyncClient, ClientError, Event as MqttEvent, Incoming, Publish, QoS, SubscribeFilter,
};
use vector_lib::{
    codecs::Decoder,
    config::{LegacyKey, LogNamespace},
    internal_event::EventsReceived,
    lookup::path,
};

use crate::{
    SourceSender,
    common::mqtt::MqttConnector,
    event::{BatchNotifier, Event},
    internal_events::{EndpointBytesReceived, MqttSubscribeError, StreamClosedError},
    serde::OneOrMany,
    shutdown::ShutdownSignal,
    sources::{SourceError, mqtt::MqttSourceConfig, util},
};

pub struct MqttSource {
    connector: MqttConnector,
    decoder: Decoder,
    log_namespace: LogNamespace,
    config: MqttSourceConfig,
}

impl MqttSource {
    #[allow(
        clippy::unnecessary_wraps,
        reason = "Removing this fallible API is deferred to keep source construction and error handling unchanged. https://github.com/vectordotdev/vector/issues/23659"
    )]
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

    pub async fn run(
        self,
        mut out: SourceSender,
        shutdown: ShutdownSignal,
    ) -> Result<(), SourceError> {
        let (client, mut connection) = self.connector.connect();
        self.subscribe(&client)?;
        let mut connected_before = false;

        loop {
            tokio::select! {
                _ = shutdown.clone() => return Ok(()),
                mqtt_event = connection.poll() => {
                    // If an error is returned here there is currently no way to tie this back
                    // to the event that was posted which means we can't accurately provide
                    // delivery guarantees.
                    // We need this issue resolved first:
                    // https://github.com/bytebeamio/rumqtt/issues/349
                    // TODO Handle acknowledgement - https://github.com/vectordotdev/vector/issues/21967
                    match mqtt_event {
                        Ok(MqttEvent::Incoming(Incoming::Publish(publish))) => {
                            self.process_message(publish, &mut out).await;
                        }
                        Ok(MqttEvent::Incoming(Incoming::ConnAck(connack))) => {
                            if connected_before
                                && !connack.session_present
                                && let Err(error) = self.subscribe(&client)
                            {
                                emit!(MqttSubscribeError { error });
                            }
                            connected_before = true;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn subscribe(&self, client: &AsyncClient) -> Result<(), ClientError> {
        match &self.config.topic {
            OneOrMany::One(topic) => client.try_subscribe(topic, QoS::AtLeastOnce),
            OneOrMany::Many(topics) => client.try_subscribe_many(
                topics
                    .iter()
                    .cloned()
                    .map(|topic| SubscribeFilter::new(topic, QoS::AtLeastOnce)),
            ),
        }
    }

    async fn process_message(&self, publish: Publish, out: &mut SourceSender) {
        emit!(EndpointBytesReceived {
            byte_size: publish.payload.len(),
            protocol: "mqtt",
            endpoint: &self.connector.options.broker_address().0,
        });
        let events_received = register!(EventsReceived);

        let (batch, _batch_receiver) = BatchNotifier::maybe_new_with_receiver(false);
        // Error is logged by `vector_lib::codecs::Decoder`, no further handling
        // is needed here.
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
            self.apply_metadata(&publish, &mut event);
            event
        })
        .collect_vec();

        let count = decoded.len();

        match out.send_batch(decoded).await {
            Ok(()) => {}
            Err(_) => emit!(StreamClosedError { count }),
        }
    }

    fn apply_metadata(&self, publish: &Publish, event: &mut Event) {
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
        }
    }
}
