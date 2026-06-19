use futures::StreamExt;
use itertools::Itertools;
use rumqttc::{Event as MqttEvent, Incoming, Publish, QoS, SubscribeFilter};
use vector_lib::{
    codecs::Decoder,
    config::{LegacyKey, LogNamespace},
    finalizer::UnorderedFinalizer,
    internal_event::EventsReceived,
    lookup::path,
};

use crate::{
    SourceSender,
    common::mqtt::MqttConnector,
    event::{BatchNotifier, BatchStatus, Event},
    internal_events::{EndpointBytesReceived, StreamClosedError},
    serde::OneOrMany,
    shutdown::ShutdownSignal,
    sources::{mqtt::MqttSourceConfig, util},
};

/// Identifies an in-flight publish so its QoS-1 PUBACK can be sent once the
/// downstream sinks confirm delivery. Only the packet id (carried by `Publish`)
/// is needed to ack; the payload is cleared before the entry is retained so
/// pending acks don't pin payloads in memory under backpressure.
#[derive(Clone, Debug)]
struct FinalizerEntry {
    publish: Publish,
    connection_generation: u64,
}

fn should_ack_finalized_publish(
    status: BatchStatus,
    entry_generation: u64,
    connection_generation: u64,
) -> bool {
    status == BatchStatus::Delivered && entry_generation == connection_generation
}

fn publish_supports_end_to_end_acknowledgements(qos: QoS) -> bool {
    qos != QoS::AtMostOnce
}

fn should_defer_publish_ack(acknowledgements: bool, qos: QoS) -> bool {
    acknowledgements && publish_supports_end_to_end_acknowledgements(qos)
}

fn warn_unsupported_acknowledgement_qos(qos: QoS, topic: &str) {
    warn!(
        message = "MQTT acknowledgements require publishes with QoS 1 or greater; forwarding message without end-to-end acknowledgement guarantee.",
        ?qos,
        topic,
        internal_log_rate_limit = true,
    );
}

const fn should_warn_session_not_resumed(acknowledgements: bool, session_present: bool) -> bool {
    acknowledgements && !session_present
}

const fn should_resubscribe_after_connack(has_connected: bool, session_present: bool) -> bool {
    has_connected && !session_present
}

fn warn_session_not_resumed() {
    warn!(
        message = "MQTT broker started a new session while acknowledgements are enabled; unacknowledged messages from any previous session for this client ID will not be redelivered.",
        internal_log_rate_limit = true,
    );
}

fn warn_resubscribe_failed() {
    warn!(
        message = "Failed to queue MQTT re-subscribe request after reconnect; will retry while connected.",
        internal_log_rate_limit = true,
    );
}

#[derive(Default)]
struct PendingAcks {
    publishes: Vec<Publish>,
}

impl PendingAcks {
    fn push(&mut self, publish: Publish) {
        self.publishes.push(publish);
    }

    fn clear(&mut self) {
        self.publishes.clear();
    }

    fn retry(&mut self, client: &rumqttc::AsyncClient) {
        self.retry_with(|publish| client.try_ack(publish).is_ok());
    }

    fn try_ack(&mut self, connected: bool, publish: Publish, client: &rumqttc::AsyncClient) {
        self.try_ack_with(connected, publish, |publish| {
            client.try_ack(publish).is_ok()
        });
    }

    fn try_ack_with(
        &mut self,
        connected: bool,
        publish: Publish,
        mut try_ack: impl FnMut(&Publish) -> bool,
    ) {
        if connected && !try_ack(&publish) {
            self.push(publish);
        }
    }

    fn retry_with(&mut self, mut try_ack: impl FnMut(&Publish) -> bool) {
        self.publishes.retain(|publish| !try_ack(publish));
    }
}

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
        let (client, mut connection) = self.connector.connect();

        self.subscribe(&client)?;

        // Finalizer drives end-to-end acknowledgements: each in-flight publish is
        // registered with its batch-status receiver, and we send the QoS-1 PUBACK
        // only once the sinks report `Delivered`. Unused when acknowledgements are
        // disabled (rumqttc auto-acks in that mode). MQTT PUBACKs are independent
        // per packet id (unlike Kafka offsets), so finalization is unordered — a
        // slow/stuck batch must not hold back acks for already-delivered publishes.
        let (finalizer, mut ack_stream) = UnorderedFinalizer::<FinalizerEntry>::maybe_new(
            self.acknowledgements,
            Some(shutdown.clone()),
        );

        // PUBACKs that rumqttc's bounded request channel was too full to accept,
        // retained for retry rather than dropped. Dropping a PUBACK for an already
        // delivered message would pin it in the broker's in-flight window until the
        // next reconnect. This is bounded in practice by that in-flight window (the
        // broker stops delivering once it fills), and the event loop below drains the
        // request channel, so entries flush on subsequent iterations.
        let mut pending_acks = PendingAcks::default();
        let mut connected = false;
        let mut has_connected = false;
        let mut pending_resubscribe = false;
        let mut connection_generation = 0;

        loop {
            if connected && pending_resubscribe {
                pending_resubscribe = !self.try_subscribe(&client);
            }

            // Retry deferred PUBACKs while connected (the event loop below drains the
            // request channel). Skipped while disconnected: a publish's packet id is
            // only valid on the connection it arrived on, so stale PUBACKs must not be
            // replayed across a reconnect.
            if connected {
                pending_acks.retry(&client);
            }

            tokio::select! {
                _ = shutdown.clone() => return Ok(()),
                entry = ack_stream.next() => {
                    // Only PUBACK delivered events. On Errored/Rejected we skip the
                    // ack so the broker redelivers after reconnect (QoS-1 +
                    // clean_session=false), giving at-least-once delivery. Use the
                    // non-blocking `try_ack` — awaiting `ack` could deadlock, since
                    // this same task polls the event loop that drains rumqttc's request
                    // channel. If that channel is full, retain the PUBACK for retry
                    // (above) instead of dropping it.
                    if let Some((status, entry)) = entry
                        && should_ack_finalized_publish(
                            status,
                            entry.connection_generation,
                            connection_generation,
                        )
                    {
                        pending_acks.try_ack(connected, entry.publish, &client);
                    }
                },
                mqtt_event = connection.poll() => {
                    // Providing at-least-once here does not require correlating a
                    // connection/poll error back to a specific in-flight publish.
                    // rumqtt#349 (no packet id for *outbound* publishes) concerns the
                    // publish/sink direction and does not apply to a subscribe-only
                    // source: each incoming Publish already carries its packet id, and
                    // we withhold its QoS-1 PUBACK until the event is delivered
                    // end-to-end. Anything left unacked when the connection drops is
                    // redelivered by the broker on reconnect (clean_session=false + QoS
                    // AtLeastOnce).
                    match mqtt_event {
                        Ok(MqttEvent::Incoming(Incoming::Publish(publish))) => {
                            self.process_message(
                                publish,
                                &mut out,
                                finalizer.as_ref(),
                                connection_generation,
                            ).await;
                        }
                        Ok(MqttEvent::Incoming(Incoming::SubAck(suback)))
                            if self.acknowledgements =>
                        {
                            for return_code in suback.return_codes {
                                if let rumqttc::SubscribeReasonCode::Success(qos) = return_code
                                    && !publish_supports_end_to_end_acknowledgements(qos)
                                {
                                    warn!(
                                        message = "MQTT broker granted a subscription QoS below the level required for end-to-end acknowledgements.",
                                        ?qos,
                                        internal_log_rate_limit = true,
                                    );
                                }
                            }
                        }
                        // A (re)connected session resumes here; the broker will
                        // redeliver any unacknowledged publishes, so drop deferred
                        // PUBACKs whose packet ids came from the previous connection.
                        Ok(MqttEvent::Incoming(Incoming::ConnAck(connack))) => {
                            let needs_resubscribe = should_resubscribe_after_connack(
                                has_connected,
                                connack.session_present,
                            );
                            if should_warn_session_not_resumed(
                                self.acknowledgements,
                                connack.session_present,
                            ) {
                                warn_session_not_resumed();
                            }
                            connected = true;
                            has_connected = true;
                            connection_generation += 1;
                            pending_acks.clear();
                            if let Some(finalizer) = &finalizer {
                                finalizer.flush();
                            }
                            if needs_resubscribe && !self.try_subscribe(&client) {
                                pending_resubscribe = true;
                            }
                        }
                        // Connection lost: same stale-packet-id reasoning, and rumqttc
                        // drops its own queued acks while reconnecting.
                        Err(_) => {
                            connected = false;
                            pending_acks.clear();
                            if let Some(finalizer) = &finalizer {
                                finalizer.flush();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn try_subscribe(&self, client: &rumqttc::AsyncClient) -> bool {
        match self.subscribe(client) {
            Ok(()) => true,
            Err(()) => {
                warn_resubscribe_failed();
                false
            }
        }
    }

    fn subscribe(&self, client: &rumqttc::AsyncClient) -> Result<(), ()> {
        match &self.config.topic {
            OneOrMany::One(topic) => client
                .try_subscribe(topic, QoS::AtLeastOnce)
                .map_err(|_| ()),
            OneOrMany::Many(topics) => client
                .try_subscribe_many(
                    topics
                        .iter()
                        .cloned()
                        .map(|topic| SubscribeFilter::new(topic, QoS::AtLeastOnce)),
                )
                .map_err(|_| ()),
        }
    }

    async fn process_message(
        &self,
        mut publish: Publish,
        out: &mut SourceSender,
        finalizer: Option<&UnorderedFinalizer<FinalizerEntry>>,
        connection_generation: u64,
    ) {
        emit!(EndpointBytesReceived {
            byte_size: publish.payload.len(),
            protocol: "mqtt",
            endpoint: &self.connector.options.broker_address().0,
        });
        let events_received = register!(EventsReceived);

        let use_end_to_end_acknowledgements =
            should_defer_publish_ack(finalizer.is_some(), publish.qos);
        if finalizer.is_some() && !use_end_to_end_acknowledgements {
            warn_unsupported_acknowledgement_qos(publish.qos, &publish.topic);
        }

        let (batch, batch_receiver) =
            BatchNotifier::maybe_new_with_receiver(use_end_to_end_acknowledgements);
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
            Ok(()) => {
                // Register the publish for deferred PUBACK once the batch is
                // delivered. Without acknowledgements `batch_receiver` is None and
                // rumqttc has already auto-acked. The payload is no longer needed
                // (ack only uses the packet id), so clear it before retaining the
                // entry to avoid pinning payloads in memory while sinks process.
                if let Some((finalizer, receiver)) = finalizer.zip(batch_receiver) {
                    publish.payload = Default::default();
                    finalizer.add(
                        FinalizerEntry {
                            publish,
                            connection_generation,
                        },
                        receiver,
                    );
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn publish(pkid: u16) -> Publish {
        let mut publish = Publish::new("topic", QoS::AtLeastOnce, vec![1, 2, 3]);
        publish.pkid = pkid;
        publish
    }

    #[test]
    fn pending_acks_keeps_failed_retries() {
        let mut pending_acks = PendingAcks::default();
        pending_acks.push(publish(1));
        pending_acks.push(publish(2));
        pending_acks.push(publish(3));

        let mut attempted = Vec::new();
        pending_acks.retry_with(|publish| {
            attempted.push(publish.pkid);
            publish.pkid != 2
        });

        assert_eq!(attempted, vec![1, 2, 3]);
        assert_eq!(pending_acks.publishes.len(), 1);
        assert_eq!(pending_acks.publishes[0].pkid, 2);

        pending_acks.retry_with(|_| true);
        assert!(pending_acks.publishes.is_empty());
    }

    #[test]
    fn pending_acks_clear_drops_stale_packet_ids() {
        let mut pending_acks = PendingAcks::default();
        pending_acks.push(publish(1));
        pending_acks.push(publish(2));

        pending_acks.clear();

        assert!(pending_acks.publishes.is_empty());
    }

    #[test]
    fn pending_acks_drops_finalized_publish_while_disconnected() {
        let mut pending_acks = PendingAcks::default();
        let mut attempted = false;

        pending_acks.try_ack_with(false, publish(1), |_| {
            attempted = true;
            true
        });

        assert!(!attempted);
        assert!(pending_acks.publishes.is_empty());
    }

    #[test]
    fn finalized_publish_must_match_current_connection_generation() {
        assert!(should_ack_finalized_publish(BatchStatus::Delivered, 2, 2));
        assert!(!should_ack_finalized_publish(BatchStatus::Delivered, 1, 2));
        assert!(!should_ack_finalized_publish(BatchStatus::Rejected, 2, 2));
    }

    #[test]
    fn qos0_publish_does_not_defer_acknowledgement() {
        assert!(!should_defer_publish_ack(true, QoS::AtMostOnce));
        assert!(should_defer_publish_ack(true, QoS::AtLeastOnce));
        assert!(should_defer_publish_ack(true, QoS::ExactlyOnce));
        assert!(!should_defer_publish_ack(false, QoS::AtLeastOnce));
    }

    #[test]
    fn warns_when_acknowledgement_session_is_not_resumed() {
        assert!(should_warn_session_not_resumed(true, false));
        assert!(!should_warn_session_not_resumed(true, true));
        assert!(!should_warn_session_not_resumed(false, false));
    }

    #[test]
    fn resubscribes_when_reconnect_starts_fresh_session() {
        assert!(!should_resubscribe_after_connack(false, false));
        assert!(!should_resubscribe_after_connack(false, true));
        assert!(!should_resubscribe_after_connack(true, true));
        assert!(should_resubscribe_after_connack(true, false));
    }
}
