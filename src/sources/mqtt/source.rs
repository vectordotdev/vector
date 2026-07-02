use std::ops::ControlFlow;

use futures::StreamExt;
use itertools::Itertools;
use rumqttc::{
    AsyncClient, Event as MqttEvent, EventLoop, Incoming, Publish, QoS, SubscribeFilter,
};
use tokio::sync::mpsc;
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

const SUBSCRIPTION_QOS: QoS = QoS::AtLeastOnce;

/// Bound on the number of polled MQTT events buffered between the poller task and
/// the main task, for events it's safe to drop under backlog (see
/// `safe_to_drop_under_backlog`). Matches the capacity of rumqttc's own
/// outgoing-request channel (see `MqttConnector::connect`) as a reasonable
/// starting point; the two aren't required to match, but there's no reason to
/// buffer more here than rumqttc itself buffers for outgoing acks/subscribes.
///
/// `pub(super)` so the integration test that exercises the saturation path can
/// publish comfortably more than this many messages without hard-coding the value.
pub(super) const EVENTS_CHANNEL_CAPACITY: usize = 1024;

/// Identifies an in-flight publish so its QoS-1 PUBACK can be sent once the
/// downstream sinks confirm delivery. Only the packet id (carried by `Publish`)
/// is needed to ack; the payload is cleared before the entry is retained so
/// pending acks don't pin payloads in memory under backpressure.
#[derive(Clone, Debug)]
struct FinalizerEntry {
    publish: Publish,
    connection_generation: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ProtocolState {
    connected: bool,
    connection_generation: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ConnAckActions {
    warn_session_not_resumed: bool,
    flush_finalizer: bool,
    resubscribe: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DisconnectActions {
    flush_finalizer: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PublishAckDecision {
    defer_ack: bool,
    warn_unsupported_qos: bool,
}

enum PolledMqttEvent {
    Event(MqttEvent),
    Disconnect,
}

impl ProtocolState {
    const fn on_connack(
        &mut self,
        acknowledgements: bool,
        session_present: bool,
    ) -> ConnAckActions {
        let actions = ConnAckActions {
            warn_session_not_resumed: acknowledgements && !session_present,
            flush_finalizer: true,
            resubscribe: self.connection_generation > 0 && !session_present,
        };

        self.connected = true;
        self.connection_generation += 1;

        actions
    }

    const fn on_disconnect(&mut self) -> DisconnectActions {
        self.connected = false;

        DisconnectActions {
            flush_finalizer: true,
        }
    }

    fn should_ack_finalized_publish(&self, status: BatchStatus, entry_generation: u64) -> bool {
        self.connected
            && status == BatchStatus::Delivered
            && entry_generation == self.connection_generation
    }
}

fn publish_supports_end_to_end_acknowledgements(qos: QoS) -> bool {
    qos != QoS::AtMostOnce
}

fn publish_ack_decision(acknowledgements: bool, qos: QoS) -> PublishAckDecision {
    let defer_ack = acknowledgements && publish_supports_end_to_end_acknowledgements(qos);

    PublishAckDecision {
        defer_ack,
        warn_unsupported_qos: acknowledgements && !defer_ack,
    }
}

fn warn_unsupported_acknowledgement_qos(qos: QoS, topic: &str) {
    warn!(
        message = "MQTT acknowledgements require publishes with QoS 1 or greater; forwarding message without end-to-end acknowledgement guarantee.",
        ?qos,
        topic,
        internal_log_rate_limit = true,
    );
}

fn warn_session_not_resumed() {
    warn!(
        message = "MQTT broker started a new session while acknowledgements are enabled; unacknowledged messages from any previous session for this client ID will not be redelivered.",
        internal_log_rate_limit = true,
    );
}

fn warn_subscribe_failed() {
    warn!(
        message = "Failed to queue MQTT subscribe request.",
        internal_log_rate_limit = true,
    );
}

fn warn_event_backlog_saturated() {
    warn!(
        message = "MQTT event backlog is saturated because downstream is not keeping up; dropping this message. It was not yet acknowledged, so the broker's own retry timer will redeliver it independently of any reconnect.",
        internal_log_rate_limit = true,
    );
}

/// Whether dropping this specific polled event under a saturated backlog is
/// protocol-safe. Only a QoS 1/2 publish that will actually be deferred-acked
/// (acknowledgements enabled, see `publish_ack_decision`) is safe to drop here:
/// only then does the broker still consider it unacknowledged and worth
/// retrying on its own. Everything else must never be dropped this way:
/// connection lifecycle events (ConnAck/SubAck/Disconnect) drive `ProtocolState`
/// and losing one desyncs it from the real connection (e.g. permanently skipping
/// a post-reconnect resubscribe); a publish rumqttc has already auto-acked
/// (acknowledgements disabled) or that has no acknowledgement at all (QoS 0) has
/// no redelivery guarantee, so dropping it would be true, unrecoverable data
/// loss rather than a redelivered retry.
fn safe_to_drop_under_backlog(event: &PolledMqttEvent, acknowledgements: bool) -> bool {
    match event {
        PolledMqttEvent::Event(MqttEvent::Incoming(Incoming::Publish(publish))) => {
            acknowledgements && publish_supports_end_to_end_acknowledgements(publish.qos)
        }
        _ => false,
    }
}

// Polls the MQTT event loop on a dedicated task so the main task (see `run`) can
// use the blocking `client.ack`/`client.subscribe` without risking a deadlock:
// `EventLoop::poll` is the only thing that drains rumqttc's outgoing request
// channel (used by those calls), so this task must never block on anything other
// than `poll` itself.
//
// Polled events are split across two channels by `safe_to_drop_under_backlog`:
// - Events safe to drop go through a bounded channel with a non-blocking
//   `try_send`; if the main task falls behind and it fills, the event is
//   dropped (with a warning) rather than buffered without bound or blocking.
//   An earlier version of this forced a reconnect on saturation instead, but
//   that reintroduces the underlying problem immediately: the broker refloods
//   a freshly (re)connected client from the same backlog, so under sustained
//   backpressure it degenerates into a reconnect storm instead of actually
//   recovering.
// - Everything else is forwarded over an unbounded channel and never dropped.
//   `send` on an unbounded channel never blocks, so this doesn't reintroduce
//   the deadlock this task is designed to avoid.
async fn poll_mqtt_connection(
    mut connection: EventLoop,
    acknowledgements: bool,
    droppable_tx: mpsc::Sender<PolledMqttEvent>,
    guaranteed_tx: mpsc::UnboundedSender<PolledMqttEvent>,
    shutdown: ShutdownSignal,
) {
    loop {
        let event = tokio::select! {
            _ = shutdown.clone() => break,
            event = connection.poll() => match event {
                Ok(event) => PolledMqttEvent::Event(event),
                Err(_) => PolledMqttEvent::Disconnect,
            },
        };

        if safe_to_drop_under_backlog(&event, acknowledgements) {
            match droppable_tx.try_send(event) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => warn_event_backlog_saturated(),
                Err(mpsc::error::TrySendError::Closed(_)) => break,
            }
        } else if guaranteed_tx.send(event).is_err() {
            break;
        }
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
        let (client, connection) = self.connector.connect();
        let (droppable_tx, mut droppable_rx) = mpsc::channel(EVENTS_CHANNEL_CAPACITY);
        let (guaranteed_tx, mut guaranteed_rx) = mpsc::unbounded_channel();
        tokio::spawn(poll_mqtt_connection(
            connection,
            self.acknowledgements,
            droppable_tx,
            guaranteed_tx,
            shutdown.clone(),
        ));

        self.subscribe(&client).await?;

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

        let mut protocol_state = ProtocolState::default();

        loop {
            tokio::select! {
                _ = shutdown.clone() => return Ok(()),
                entry = ack_stream.next() => {
                    // Only PUBACK delivered events. On Errored/Rejected we skip the
                    // ack so the broker redelivers after reconnect (QoS-1 +
                    // clean_session=false), giving at-least-once delivery. The MQTT
                    // event loop is polled by a separate task, so awaiting `ack` here
                    // does not block the task responsible for draining rumqttc's
                    // request channel. Stale finalizers from prior connections are
                    // ignored because packet IDs are only valid on their connection.
                    if let Some((status, entry)) = entry
                        && protocol_state.should_ack_finalized_publish(
                            status,
                            entry.connection_generation,
                        )
                    {
                        client.ack(&entry.publish).await.map_err(|_| ())?;
                    }
                },
                mqtt_event = droppable_rx.recv() => {
                    if let ControlFlow::Break(result) = self.handle_mqtt_event(
                        mqtt_event, &mut out, &finalizer, &mut protocol_state, &client,
                    ).await {
                        return result;
                    }
                },
                mqtt_event = guaranteed_rx.recv() => {
                    if let ControlFlow::Break(result) = self.handle_mqtt_event(
                        mqtt_event, &mut out, &finalizer, &mut protocol_state, &client,
                    ).await {
                        return result;
                    }
                },
            }
        }
    }

    // Providing at-least-once here does not require correlating a
    // connection/poll error back to a specific in-flight publish. rumqtt#349 (no
    // packet id for *outbound* publishes) concerns the publish/sink direction
    // and does not apply to a subscribe-only source: each incoming Publish
    // already carries its packet id, and we withhold its QoS-1 PUBACK until the
    // event is delivered end-to-end. Anything left unacked when the connection
    // drops is redelivered by the broker on reconnect (clean_session=false + QoS
    // AtLeastOnce).
    //
    // Returns `ControlFlow::Break` with the result `run` should return, or
    // `ControlFlow::Continue` to keep looping.
    async fn handle_mqtt_event(
        &self,
        mqtt_event: Option<PolledMqttEvent>,
        out: &mut SourceSender,
        finalizer: &Option<UnorderedFinalizer<FinalizerEntry>>,
        protocol_state: &mut ProtocolState,
        client: &AsyncClient,
    ) -> ControlFlow<Result<(), ()>> {
        match mqtt_event {
            Some(PolledMqttEvent::Event(MqttEvent::Incoming(Incoming::Publish(publish)))) => {
                self.process_message(
                    publish,
                    out,
                    finalizer.as_ref(),
                    protocol_state.connection_generation,
                )
                .await;
            }
            Some(PolledMqttEvent::Event(MqttEvent::Incoming(Incoming::SubAck(suback))))
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
            // A (re)connected session resumes here; the broker will redeliver
            // any unacknowledged publishes, so drop deferred PUBACKs whose
            // packet ids came from the previous connection.
            Some(PolledMqttEvent::Event(MqttEvent::Incoming(Incoming::ConnAck(connack)))) => {
                let actions =
                    protocol_state.on_connack(self.acknowledgements, connack.session_present);
                if actions.warn_session_not_resumed {
                    warn_session_not_resumed();
                }
                if actions.flush_finalizer
                    && let Some(finalizer) = finalizer
                {
                    finalizer.flush();
                }
                if actions.resubscribe
                    && let Err(()) = self.subscribe(client).await
                {
                    return ControlFlow::Break(Err(()));
                }
            }
            // Connection lost: same stale-packet-id reasoning, and rumqttc drops
            // its own queued acks while reconnecting.
            Some(PolledMqttEvent::Disconnect) => {
                let actions = protocol_state.on_disconnect();
                if actions.flush_finalizer
                    && let Some(finalizer) = finalizer
                {
                    finalizer.flush();
                }
            }
            None => return ControlFlow::Break(Ok(())),
            _ => {}
        }
        ControlFlow::Continue(())
    }

    async fn subscribe(&self, client: &AsyncClient) -> Result<(), ()> {
        let result = match &self.config.topic {
            OneOrMany::One(topic) => client.subscribe(topic, SUBSCRIPTION_QOS).await,
            OneOrMany::Many(topics) => {
                client
                    .subscribe_many(
                        topics
                            .iter()
                            .cloned()
                            .map(|topic| SubscribeFilter::new(topic, SUBSCRIPTION_QOS)),
                    )
                    .await
            }
        };

        if result.is_err() {
            warn_subscribe_failed();
        }

        result.map_err(|_| ())
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

        let ack_decision = publish_ack_decision(finalizer.is_some(), publish.qos);
        if ack_decision.warn_unsupported_qos {
            warn_unsupported_acknowledgement_qos(publish.qos, &publish.topic);
        }

        let (batch, batch_receiver) =
            BatchNotifier::maybe_new_with_receiver(ack_decision.defer_ack);
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

    #[test]
    fn protocol_contract_matrix_for_requested_and_granted_qos() {
        assert!(publish_supports_end_to_end_acknowledgements(
            SUBSCRIPTION_QOS
        ));

        for (granted_qos, supports_acknowledgements) in [
            (QoS::AtMostOnce, false),
            (QoS::AtLeastOnce, true),
            (QoS::ExactlyOnce, true),
        ] {
            assert_eq!(
                publish_supports_end_to_end_acknowledgements(granted_qos),
                supports_acknowledgements
            );
        }
    }

    #[test]
    fn protocol_contract_matrix_for_publisher_qos() {
        for (acknowledgements, publisher_qos, expected) in [
            (
                false,
                QoS::AtMostOnce,
                PublishAckDecision {
                    defer_ack: false,
                    warn_unsupported_qos: false,
                },
            ),
            (
                false,
                QoS::AtLeastOnce,
                PublishAckDecision {
                    defer_ack: false,
                    warn_unsupported_qos: false,
                },
            ),
            (
                true,
                QoS::AtMostOnce,
                PublishAckDecision {
                    defer_ack: false,
                    warn_unsupported_qos: true,
                },
            ),
            (
                true,
                QoS::AtLeastOnce,
                PublishAckDecision {
                    defer_ack: true,
                    warn_unsupported_qos: false,
                },
            ),
            (
                true,
                QoS::ExactlyOnce,
                PublishAckDecision {
                    defer_ack: true,
                    warn_unsupported_qos: false,
                },
            ),
        ] {
            assert_eq!(
                publish_ack_decision(acknowledgements, publisher_qos),
                expected
            );
        }
    }

    #[test]
    fn protocol_contract_matrix_for_session_reset_and_connection_generation() {
        for (acknowledgements, session_present, expected_warn) in [
            (false, false, false),
            (true, false, true),
            (true, true, false),
        ] {
            let mut state = ProtocolState::default();
            let actions = state.on_connack(acknowledgements, session_present);

            assert_eq!(actions.warn_session_not_resumed, expected_warn);
            assert!(actions.flush_finalizer);
            assert!(!actions.resubscribe);
            assert!(state.connected);
            assert_eq!(state.connection_generation, 1);
        }

        let mut resumed_session = ProtocolState::default();
        resumed_session.on_connack(true, true);
        let actions = resumed_session.on_connack(true, true);
        assert_eq!(
            actions,
            ConnAckActions {
                warn_session_not_resumed: false,
                flush_finalizer: true,
                resubscribe: false,
            }
        );
        assert_eq!(resumed_session.connection_generation, 2);

        let mut fresh_session = ProtocolState::default();
        fresh_session.on_connack(true, true);
        let actions = fresh_session.on_connack(true, false);
        assert_eq!(
            actions,
            ConnAckActions {
                warn_session_not_resumed: true,
                flush_finalizer: true,
                resubscribe: true,
            }
        );
        assert_eq!(fresh_session.connection_generation, 2);
    }

    #[test]
    fn protocol_contract_matrix_for_disconnect() {
        let mut state = ProtocolState::default();
        state.on_connack(true, true);

        let actions = state.on_disconnect();

        assert_eq!(
            actions,
            DisconnectActions {
                flush_finalizer: true,
            }
        );
        assert!(!state.connected);
        assert_eq!(state.connection_generation, 1);
    }

    #[test]
    fn protocol_contract_matrix_for_finalization_statuses() {
        let mut state = ProtocolState::default();
        state.on_connack(true, true);
        state.on_connack(true, true);

        for (status, entry_generation, should_ack) in [
            (BatchStatus::Delivered, 2, true),
            (BatchStatus::Delivered, 1, false),
            (BatchStatus::Errored, 2, false),
            (BatchStatus::Rejected, 2, false),
        ] {
            assert_eq!(
                state.should_ack_finalized_publish(status, entry_generation),
                should_ack
            );
        }

        state.on_disconnect();
        assert!(!state.should_ack_finalized_publish(BatchStatus::Delivered, 2));
    }

    #[test]
    fn protocol_contract_matrix_for_backlog_drop_safety() {
        fn publish_event(qos: QoS) -> PolledMqttEvent {
            PolledMqttEvent::Event(MqttEvent::Incoming(Incoming::Publish(Publish::new(
                "topic",
                qos,
                vec![1, 2, 3],
            ))))
        }

        for (acknowledgements, qos, safe_to_drop) in [
            (false, QoS::AtMostOnce, false),
            (false, QoS::AtLeastOnce, false),
            (false, QoS::ExactlyOnce, false),
            (true, QoS::AtMostOnce, false),
            (true, QoS::AtLeastOnce, true),
            (true, QoS::ExactlyOnce, true),
        ] {
            assert_eq!(
                safe_to_drop_under_backlog(&publish_event(qos), acknowledgements),
                safe_to_drop,
                "acknowledgements={acknowledgements}, qos={qos:?}"
            );
        }

        assert!(!safe_to_drop_under_backlog(
            &PolledMqttEvent::Disconnect,
            true
        ));
    }
}
