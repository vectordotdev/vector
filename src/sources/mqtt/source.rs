use itertools::Itertools;
use vector_lib::config::LogNamespace;
use vector_lib::internal_event::EventsReceived;

use crate::{
    codecs::Decoder,
    common::mqtt::MqttConnector,
    event::BatchNotifier,
    event::Event,
    internal_events::{EndpointBytesReceived, StreamClosedError},
    shutdown::ShutdownSignal,
    sources::mqtt::MqttSourceConfig,
    sources::util,
    SourceSender,
};
use rumqttc::{Event as MqttEvent, Incoming, Publish, QoS};
use vector_lib::config::LegacyKey;
use vector_lib::lookup::path;

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
        let (client, mut connection) = self.connector.connect();

        client
            .subscribe(&self.config.topic, QoS::AtLeastOnce)
            .await
            .map_err(|_| ())?;

        loop {
            tokio::select! {
                _ = shutdown.clone() => return Ok(()),
                mqtt_event = connection.poll() => {
                    // If an error is returned here there is currently no way to tie this back
                    // to the event that was posted which means we can't accurately provide
                    // delivery guarantees.
                    // We need this issue resolved first:
                    // https://github.com/bytebeamio/rumqtt/issues/349
                    match mqtt_event {
                        Ok(MqttEvent::Incoming(Incoming::Publish(publish))) => {
                            self.process_message(publish, &mut out).await;
                        }
                        Ok(MqttEvent::Incoming(
                            Incoming::PubAck(_) | Incoming::PubRec(_) | Incoming::PubComp(_),
                        )) => {
                            // TODO Handle acknowledgement - https://github.com/vectordotdev/vector/issues/21967
                        }
                        _ => {}
                    }
                }
            }
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
        // Error is logged by `crate::codecs::Decoder`, no further handling
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
