use itertools::Itertools;
use vector_lib::config::LogNamespace;
use vector_lib::internal_event::EventsReceived;

use rumqttc::{Event as MqttEvent, Publish};

use crate::{
    codecs::Decoder,
    event::BatchNotifier,
    internal_events::{EndpointBytesReceived, StreamClosedError},
    shutdown::ShutdownSignal,
    sources::util,
    SourceSender,
};

use rumqttc::{AsyncClient, ClientError, EventLoop, Incoming, MqttOptions, QoS};
use snafu::Snafu;
use vector_lib::tls::TlsError;

use super::config::ConfigurationError;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum MqttError {
    #[snafu(display("TLS error: {}", source))]
    Tls { source: TlsError },
    #[snafu(display("MQTT configuration error: {}", source))]
    Configuration { source: ConfigurationError },
}

#[derive(Clone)]
pub struct MqttConnector {
    options: MqttOptions,
    topic: String,
}

impl MqttConnector {
    pub const fn new(options: MqttOptions, topic: String) -> Result<Self, MqttError> {
        Ok(Self { options, topic })
    }

    async fn connect(&self) -> Result<(AsyncClient, EventLoop), ClientError> {
        let (client, eventloop) = AsyncClient::new(self.options.clone(), 1024);
        client.subscribe(&self.topic, QoS::AtLeastOnce).await?;
        Ok((client, eventloop))
    }
}

pub struct MqttSource {
    connector: MqttConnector,
    decoder: Decoder,
    log_namespace: LogNamespace,
}

impl MqttSource {
    pub fn new(
        connector: MqttConnector,
        decoder: Decoder,
        log_namespace: LogNamespace,
    ) -> crate::Result<Self> {
        Ok(Self {
            connector,
            decoder,
            log_namespace,
        })
    }

    pub async fn run(self, mut out: SourceSender, shutdown: ShutdownSignal) -> Result<(), ()> {
        let (_client, mut connection) = self.connector.connect().await.map_err(|_| ())?;

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
                            // TODO Handle acknowledgement
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
        .collect_vec();

        let count = decoded.len();

        match out.send_batch(decoded).await {
            Ok(()) => {}
            Err(_) => emit!(StreamClosedError { count }),
        }
    }
}
