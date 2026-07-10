use std::time::Duration;

use futures::{StreamExt, stream::BoxStream};
use itertools::Itertools;
use rumqttc::{Event as MqttEvent, Incoming, Publish, QoS, SubscribeFilter};
use vector_lib::{
    codecs::Decoder,
    config::{LegacyKey, LogNamespace},
    finalizer::OrderedFinalizer,
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

/// Minimum time between forced reconnects triggered by a withheld ack (see
/// `ack_suppressed`). Brokers that only redeliver in-flight QoS 1 messages on
/// reconnect would otherwise leave a withheld publish (and everything
/// suppressed after it) stuck until some unrelated reconnect happens, so a
/// forced reconnect gives it another chance. But if the downstream failure
/// causing the suppression is persistent, the redelivered publish will likely
/// fail again immediately, and reconnecting on every failure would thrash the
/// connection; this cooldown caps that to at most once per interval.
const FORCED_RECONNECT_COOLDOWN: Duration = Duration::from_secs(30);

/// How long to wait, after sending a graceful `Disconnect`, for the broker to
/// close the connection before dropping it locally. [MQTT-3.14.4-2] puts the
/// close obligation on the client anyway; the server only SHOULD close. A
/// broker that keeps the socket open would otherwise leave `ack_suppressed`
/// pinned for the rest of the (never-ending) generation, stalling delivery
/// with nothing left to trigger recovery.
const FORCED_RECONNECT_ESCALATION_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to wait before re-attempting to queue a forced-reconnect
/// `Disconnect` after rumqttc's request channel was too full to accept it.
/// Non-zero so the retry timer can't monopolize the select loop while the
/// event loop drains the channel.
const FORCED_RECONNECT_RETRY_DELAY: Duration = Duration::from_millis(100);

/// How long to wait before resending a (re)subscribe after the broker
/// rejected it (SUBACK failure return code, e.g. an ACL denial). Immediate
/// retries would flood the broker with SUBSCRIBE packets at network round-trip
/// speed; never retrying would leave the source silently subscribed to
/// nothing on an otherwise healthy connection if the denial is later lifted.
const RESUBSCRIBE_RETRY_DELAY: Duration = Duration::from_secs(30);

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
    /// A (re)subscribe is needed and has not yet been confirmed by a SUBACK.
    /// Starts `true`: the initial subscribe needs SUBACK confirmation exactly
    /// like a re-subscribe does (a lost initial SUBSCRIBE followed by a
    /// `session_present = true` reconnect would otherwise never be retried).
    pending_resubscribe: bool,
    /// A (re)subscribe request has been queued and we're waiting to see
    /// whether it results in a SUBACK, so `loop_actions` doesn't keep
    /// resending it every iteration in the meantime. Cleared by `on_suback`
    /// (success) or by queueing failing again (so the next iteration retries).
    resubscribe_awaiting_suback: bool,
    connection_generation: u64,
    /// Set once a publish in the current generation finalizes as anything
    /// other than `Delivered`. MQTT requires PUBACKs to be sent in the order
    /// their publishes were received ([MQTT-4.6.0-2]); acking a later packet
    /// while an earlier one in the same generation was never acked would
    /// violate that order, so once set, no further acks are sent until the
    /// next reconnect. The withheld ones are redelivered the same way the
    /// packet that triggered this already is.
    ///
    /// [MQTT-4.6.0-2]: https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html
    ack_suppressed: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct LoopActions {
    retry_pending_acks: bool,
    retry_resubscribe: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ConnAckActions {
    warn_session_not_resumed: bool,
    clear_pending_acks: bool,
    recreate_finalizer: bool,
    resubscribe: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DisconnectActions {
    clear_pending_acks: bool,
    recreate_finalizer: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PublishAckDecision {
    defer_ack: bool,
    warn_unsupported_qos: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct FinalizedPublishDecision {
    should_ack: bool,
    just_suppressed: bool,
}

/// Schedules the graceful forced reconnects triggered by withheld acks (see
/// `FORCED_RECONNECT_COOLDOWN`). A suppression inside the cooldown window
/// must schedule the reconnect for when the cooldown expires rather than skip
/// it: `just_suppressed` fires only once per connection generation, so a
/// skipped reconnect would never get another trigger and the source would
/// stay connected (and stuck) indefinitely.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ForcedReconnectSchedule {
    due_at: Option<tokio::time::Instant>,
    cooldown_until: Option<tokio::time::Instant>,
    escalate_at: Option<tokio::time::Instant>,
}

impl ForcedReconnectSchedule {
    /// An ack was just withheld: schedule a reconnect, immediately if outside
    /// the cooldown window, at cooldown expiry otherwise.
    fn schedule(&mut self, now: tokio::time::Instant) {
        let at = self.cooldown_until.map_or(now, |until| until.max(now));
        self.due_at = Some(self.due_at.map_or(at, |already_due| already_due.min(at)));
    }

    /// When the scheduled reconnect should fire, if one is scheduled.
    const fn due_at(&self) -> Option<tokio::time::Instant> {
        self.due_at
    }

    /// The disconnect was queued: clear the schedule, start the cooldown, and
    /// arm the escalation deadline in case the broker never closes the
    /// connection in response (see `FORCED_RECONNECT_ESCALATION_TIMEOUT`).
    fn sent(&mut self, now: tokio::time::Instant) {
        self.due_at = None;
        self.cooldown_until = Some(now + FORCED_RECONNECT_COOLDOWN);
        self.escalate_at = Some(now + FORCED_RECONNECT_ESCALATION_TIMEOUT);
    }

    /// The disconnect couldn't be queued (rumqttc's request channel was
    /// full): retry shortly, once the event loop has drained the channel.
    fn retry(&mut self, now: tokio::time::Instant) {
        self.due_at = Some(now + FORCED_RECONNECT_RETRY_DELAY);
    }

    /// When to drop the connection locally because the broker never closed it
    /// after our `Disconnect`, if one was sent and is still unanswered.
    const fn escalate_at(&self) -> Option<tokio::time::Instant> {
        self.escalate_at
    }

    /// A `Disconnect` has been sent and the connection hasn't dropped yet, so
    /// nothing further should be written to it ([MQTT-3.14.4-1]).
    const fn disconnect_sent(&self) -> bool {
        self.escalate_at.is_some()
    }

    /// The escalation fired: the local teardown is happening now.
    const fn escalated(&mut self) {
        self.escalate_at = None;
    }

    /// The connection dropped: whatever reconnect or escalation was scheduled
    /// has been achieved by that disconnect.
    const fn cancel(&mut self) {
        self.due_at = None;
        self.escalate_at = None;
    }
}

impl ProtocolState {
    const fn loop_actions(&self) -> LoopActions {
        LoopActions {
            retry_pending_acks: self.connected,
            retry_resubscribe: self.connected
                && self.pending_resubscribe
                && !self.resubscribe_awaiting_suback,
        }
    }

    const fn on_connack(
        &mut self,
        acknowledgements: bool,
        session_present: bool,
    ) -> ConnAckActions {
        // `session_present` alone can't be trusted when a previous (re)subscribe
        // was never confirmed by a SUBACK: the broker's persistent session can
        // exist (session_present = true) while missing that subscription,
        // because the SUBSCRIBE that would have added it was lost (e.g. a
        // disconnect racing the send) before this reconnect. So once a
        // resubscribe is needed, it stays needed across reconnects regardless
        // of what this connection's `session_present` says, until a SUBACK
        // actually confirms it (`on_suback`).
        let resubscribe =
            self.pending_resubscribe || (self.connection_generation > 0 && !session_present);

        let actions = ConnAckActions {
            warn_session_not_resumed: acknowledgements && !session_present,
            clear_pending_acks: true,
            recreate_finalizer: true,
            resubscribe,
        };

        self.connected = true;
        self.connection_generation += 1;
        self.ack_suppressed = false;
        self.pending_resubscribe = resubscribe;
        // A fresh connection means any previous queue attempt no longer
        // applies (its SUBACK, if it ever arrives, would be for a subscribe
        // on a connection that's gone) -- retry immediately on this one.
        self.resubscribe_awaiting_suback = false;

        actions
    }

    const fn on_disconnect(&mut self) -> DisconnectActions {
        self.connected = false;

        DisconnectActions {
            clear_pending_acks: true,
            recreate_finalizer: true,
        }
    }

    /// Records whether a (re)subscribe request was successfully queued.
    /// `pending_resubscribe` is intentionally left set either way: queueing
    /// only means rumqttc accepted the request locally, not that the broker
    /// received or processed it, so it's only cleared once a SUBACK actually
    /// confirms success (`on_suback`). A request that's lost (e.g. a
    /// disconnect racing the send) is retried on the next reconnect.
    const fn on_resubscribe_result(&mut self, queued: bool) {
        self.resubscribe_awaiting_suback = queued;
    }

    /// A SUBACK granting every requested topic filter confirms the
    /// (re)subscribe, so stop retrying it. Only called when no return code
    /// was a failure; a rejected filter keeps `pending_resubscribe` set and
    /// the retry is paced by `RESUBSCRIBE_RETRY_DELAY` instead.
    const fn on_suback(&mut self) {
        self.pending_resubscribe = false;
        self.resubscribe_awaiting_suback = false;
    }

    /// Decides whether a finalized publish should be acked, and whether this
    /// is the moment its generation's `ack_suppressed` first became set. See
    /// `ack_suppressed`'s doc comment for why a non-`Delivered` status
    /// withholds every ack after it, not just its own; `just_suppressed` is
    /// how the caller learns it needs to force a reconnect so the broker
    /// redelivers the withheld publish (see `FORCED_RECONNECT_COOLDOWN`).
    fn finalize_publish(
        &mut self,
        status: BatchStatus,
        entry_generation: u64,
    ) -> FinalizedPublishDecision {
        if entry_generation != self.connection_generation {
            return FinalizedPublishDecision {
                should_ack: false,
                just_suppressed: false,
            };
        }

        if status != BatchStatus::Delivered {
            let just_suppressed = !self.ack_suppressed;
            self.ack_suppressed = true;
            return FinalizedPublishDecision {
                should_ack: false,
                just_suppressed,
            };
        }

        FinalizedPublishDecision {
            should_ack: !self.ack_suppressed,
            just_suppressed: false,
        }
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

fn warn_resubscribe_failed() {
    warn!(
        message = "Failed to queue MQTT re-subscribe request after reconnect; will retry while connected.",
        internal_log_rate_limit = true,
    );
}

fn warn_forcing_reconnect_after_suppressed_ack() {
    warn!(
        message = "An MQTT publish was not delivered and its acknowledgement was withheld; forcing a reconnect so the broker redelivers it.",
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
        if !connected {
            return;
        }
        // Earlier acks are still queued (the loop-top retry couldn't flush
        // them this iteration): this newer ack must queue behind them, not
        // jump ahead -- PUBACKs must go out in publish-receipt order
        // ([MQTT-4.6.0-2]), and rumqttc's channel can free up between the
        // failed retry and now (the intervening `connection.poll()` may have
        // partially drained it before being cancelled by this branch).
        if !self.publishes.is_empty() || !try_ack(&publish) {
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

        // Finalizer drives end-to-end acknowledgements: each in-flight publish is
        // registered with its batch-status receiver, and we send the QoS-1 PUBACK
        // only once the sinks report `Delivered`. Unused when acknowledgements are
        // disabled (rumqttc auto-acks in that mode). PUBACKs must be sent in the
        // order their publishes were received ([MQTT-4.6.0-2]), so finalization is
        // ordered: a slow/stuck earlier batch holds back acks for publishes
        // received after it, same as the `kafka` source's ordered offset commits.
        let (mut finalizer, mut ack_stream) = self.new_finalizer(&shutdown);

        // PUBACKs that rumqttc's bounded request channel was too full to accept,
        // retained for retry rather than dropped. Dropping a PUBACK for an already
        // delivered message would pin it in the broker's in-flight window until the
        // next reconnect. This is bounded in practice by that in-flight window (the
        // broker stops delivering once it fills), and the event loop below drains the
        // request channel, so entries flush on subsequent iterations.
        let mut pending_acks = PendingAcks::default();

        // The initial subscribe is issued on the first ConnAck, driven by
        // `pending_resubscribe` starting `true`, so it goes through the same
        // SUBACK-confirmation tracking as every re-subscribe.
        let mut protocol_state = ProtocolState {
            pending_resubscribe: true,
            ..ProtocolState::default()
        };

        let mut forced_reconnect = ForcedReconnectSchedule::default();

        // When the broker rejects a (re)subscribe (SUBACK failure return
        // code), the retry is paced by this timer instead of resent
        // immediately; see `RESUBSCRIBE_RETRY_DELAY`.
        let mut resubscribe_retry_at: Option<tokio::time::Instant> = None;

        loop {
            // Once a graceful `Disconnect` has been queued, nothing else may
            // be written to the connection ([MQTT-3.14.4-1]); the retries
            // resume after the reconnect it causes.
            let draining = forced_reconnect.disconnect_sent();

            let actions = protocol_state.loop_actions();
            if actions.retry_resubscribe && !draining {
                protocol_state.on_resubscribe_result(self.try_subscribe(&client));
            }

            // Retry deferred PUBACKs while connected (the event loop below drains the
            // request channel). Skipped while disconnected: a publish's packet id is
            // only valid on the connection it arrived on, so stale PUBACKs must not be
            // replayed across a reconnect.
            if actions.retry_pending_acks && !draining {
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
                    if let Some((status, entry)) = entry {
                        let decision = protocol_state.finalize_publish(
                            status,
                            entry.connection_generation,
                        );
                        if decision.should_ack {
                            pending_acks.try_ack(protocol_state.connected, entry.publish, &client);
                        }
                        // A withheld ack leaves the publish (and everything
                        // suppressed after it) stuck until the connection
                        // reconnects, so schedule a forced reconnect: sent by
                        // the timer branch below, immediately if outside the
                        // cooldown window, at cooldown expiry otherwise (so a
                        // persistent downstream failure -- the redelivered
                        // publish failing again right away -- reconnects at a
                        // capped rate instead of thrashing the connection or,
                        // worse, never again).
                        if decision.just_suppressed {
                            forced_reconnect.schedule(tokio::time::Instant::now());
                        }
                    }
                },
                // A scheduled forced reconnect is due: gracefully disconnect
                // (`try_disconnect` queues an MQTT `Disconnect` packet) so the
                // broker redelivers the withheld publish once reconnected.
                // Graceful rather than dropping the network directly: an
                // earlier attempt at the latter (`EventLoop::clean()`) was
                // verified against a live EMQX broker to trigger a broker-side
                // `unexpected_sock_close` error that stopped delivery
                // entirely, whereas an explicit `Disconnect` is a normal,
                // expected client-initiated close.
                _ = tokio::time::sleep_until(
                    forced_reconnect.due_at().unwrap_or_else(tokio::time::Instant::now)
                ), if forced_reconnect.due_at().is_some() && protocol_state.connected => {
                    let now = tokio::time::Instant::now();
                    if client.try_disconnect().is_ok() {
                        warn_forcing_reconnect_after_suppressed_ack();
                        forced_reconnect.sent(now);
                    } else {
                        // rumqttc's request channel is full; the event loop
                        // branch below drains it, so retrying stays live.
                        forced_reconnect.retry(now);
                    }
                },
                // The broker never closed the connection in response to our
                // `Disconnect`: close it locally, as [MQTT-3.14.4-2] requires
                // of the client anyway. Safe to be abrupt here -- the session
                // was already ended gracefully by the `Disconnect` packet, so
                // this is not the mid-session socket drop that upset brokers
                // when tried previously (see the comment on the branch above).
                _ = tokio::time::sleep_until(
                    forced_reconnect.escalate_at().unwrap_or_else(tokio::time::Instant::now)
                ), if forced_reconnect.escalate_at().is_some() => {
                    forced_reconnect.escalated();
                    connection.clean();
                    (finalizer, ack_stream) = self.handle_connection_lost(
                        &mut connection,
                        &mut protocol_state,
                        &mut pending_acks,
                        &mut forced_reconnect,
                        &mut resubscribe_retry_at,
                        &shutdown,
                    );
                },
                // The broker rejected a (re)subscribe earlier; the pacing
                // delay has passed, so try again.
                _ = tokio::time::sleep_until(
                    resubscribe_retry_at.unwrap_or_else(tokio::time::Instant::now)
                ), if resubscribe_retry_at.is_some() => {
                    resubscribe_retry_at = None;
                    if protocol_state.connected
                        && protocol_state.pending_resubscribe
                        && !forced_reconnect.disconnect_sent()
                    {
                        protocol_state.on_resubscribe_result(self.try_subscribe(&client));
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
                                protocol_state.connection_generation,
                            ).await;
                        }
                        Ok(MqttEvent::Incoming(Incoming::SubAck(suback))) => {
                            // A SUBACK only confirms the broker *processed* the
                            // (re)subscribe; each return code says whether the
                            // matching topic filter was actually granted, in
                            // SUBSCRIBE order ([MQTT-3.9.3-1]). A rejected
                            // filter (failure code 0x80, e.g. an ACL denial)
                            // must not count as subscribed -- keep retrying,
                            // paced by `RESUBSCRIBE_RETRY_DELAY`.
                            let mut any_rejected = false;
                            for (topic, return_code) in
                                self.subscribed_topics().zip(suback.return_codes.iter())
                            {
                                match return_code {
                                    rumqttc::SubscribeReasonCode::Success(qos) => {
                                        if self.acknowledgements
                                            && !publish_supports_end_to_end_acknowledgements(*qos)
                                        {
                                            warn!(
                                                message = "MQTT broker granted a subscription QoS below the level required for end-to-end acknowledgements.",
                                                topic,
                                                ?qos,
                                                internal_log_rate_limit = true,
                                            );
                                        }
                                    }
                                    rumqttc::SubscribeReasonCode::Failure => {
                                        any_rejected = true;
                                        error!(
                                            message = "MQTT broker rejected the subscription; will retry.",
                                            topic,
                                            internal_log_rate_limit = true,
                                        );
                                    }
                                }
                            }
                            if any_rejected {
                                resubscribe_retry_at = Some(
                                    tokio::time::Instant::now() + RESUBSCRIBE_RETRY_DELAY,
                                );
                            } else {
                                protocol_state.on_suback();
                                resubscribe_retry_at = None;
                            }
                        }
                        // A (re)connected session resumes here; the broker will
                        // redeliver any unacknowledged publishes, so drop deferred
                        // PUBACKs whose packet ids came from the previous connection.
                        Ok(MqttEvent::Incoming(Incoming::ConnAck(connack))) => {
                            let actions = protocol_state.on_connack(
                                self.acknowledgements,
                                connack.session_present,
                            );
                            if actions.warn_session_not_resumed {
                                warn_session_not_resumed();
                            }
                            if actions.clear_pending_acks {
                                pending_acks.clear();
                            }
                            if actions.recreate_finalizer {
                                (finalizer, ack_stream) = self.new_finalizer(&shutdown);
                            }
                            if actions.resubscribe {
                                protocol_state.on_resubscribe_result(self.try_subscribe(&client));
                            }
                            // A retry the previous connection had scheduled no
                            // longer applies; this connection's subscribe was
                            // just issued above (or is already confirmed).
                            resubscribe_retry_at = None;
                        }
                        // Connection lost: `poll()` already called
                        // `EventLoop::clean()` internally before returning this
                        // error; scrub everything from the dead connection that
                        // rumqttc would otherwise carry over into the next one.
                        Err(_) => {
                            (finalizer, ack_stream) = self.handle_connection_lost(
                                &mut connection,
                                &mut protocol_state,
                                &mut pending_acks,
                                &mut forced_reconnect,
                                &mut resubscribe_retry_at,
                                &shutdown,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// The connection is gone (either `poll()` returned an error, or we tore
    /// it down locally after an unanswered `Disconnect`): reset all per-
    /// connection state, and scrub everything rumqttc would otherwise replay
    /// from the dead connection onto the next one.
    fn handle_connection_lost(
        &self,
        connection: &mut rumqttc::EventLoop,
        protocol_state: &mut ProtocolState,
        pending_acks: &mut PendingAcks,
        forced_reconnect: &mut ForcedReconnectSchedule,
        resubscribe_retry_at: &mut Option<tokio::time::Instant>,
        shutdown: &ShutdownSignal,
    ) -> (
        Option<OrderedFinalizer<FinalizerEntry>>,
        BoxStream<'static, (BatchStatus, FinalizerEntry)>,
    ) {
        let actions = protocol_state.on_disconnect();
        if actions.clear_pending_acks {
            pending_acks.clear();
            // `EventLoop::clean()` moved any queued-but-unsent requests into
            // `connection.pending` for automatic replay on the next
            // connection. A PUBACK/PUBREC references a packet id that was
            // only valid on the connection that just died (replaying it could
            // ack the wrong publish), and a replayed `Disconnect` would kill
            // the fresh connection the moment it comes up -- drop both.
            // (`Subscribe` requests are left in: replaying one is a harmless,
            // idempotent re-subscribe.)
            connection.pending.retain(|request| {
                !matches!(
                    request,
                    rumqttc::Request::PubAck(_)
                        | rumqttc::Request::PubRec(_)
                        | rumqttc::Request::Disconnect(_)
                )
            });
            // rumqttc buffers batches of incoming packets in `state.events`
            // and yields them one per poll; `clean()` does NOT clear that
            // buffer, so events from the dead connection would otherwise be
            // yielded *after* the next ConnAck and be indistinguishable from
            // new-connection traffic. Everything buffered here belongs to the
            // dead connection (the reconnect ConnAck is returned directly by
            // `poll()`, never through this buffer), but what's safe to drop
            // depends on the ack mode:
            if self.acknowledgements {
                // Manual-ack mode: nothing buffered has been acked (acks are
                // only sent after end-to-end finalization, and these publishes
                // were never even yielded), so the broker redelivers all of
                // it. Dropping everything prevents a stale Publish from being
                // tagged with the new generation (its stale packet id then
                // acked on the new connection) and a stale SubAck from
                // falsely confirming the new connection's subscribe.
                connection.state.events.clear();
            } else {
                // Auto-ack mode: rumqttc queues the PUBACK the moment it
                // buffers a publish, so it may already be on the wire and the
                // broker is then allowed to never redeliver -- dropping a
                // buffered publish here would lose it outright. Keep the
                // publishes for processing (there's no generation-sensitive
                // ack machinery in this mode to confuse); drop only the
                // stale non-publish events, so e.g. a stale SubAck can't
                // falsely confirm the new connection's subscribe.
                connection
                    .state
                    .events
                    .retain(|event| matches!(event, MqttEvent::Incoming(Incoming::Publish(_))));
            }
        }
        // The reconnect any scheduled forced-disconnect was asking for has
        // now happened; no need to send another once reconnected.
        forced_reconnect.cancel();
        *resubscribe_retry_at = None;
        debug_assert!(actions.recreate_finalizer);
        self.new_finalizer(shutdown)
    }

    // Builds a fresh finalizer/ack-stream pair, discarding whatever the
    // previous one held. Used instead of `FinalizerSet::flush` on
    // (re)connect: `flush` only clears entries already pulled into the
    // stream's internal ordered set, not ones an earlier `finalizer.add`
    // call sent that are still sitting in its channel; those survive the
    // flush and get pulled into the set afterwards anyway. Because
    // `OrderedFinalizer` won't yield anything newer until that stale entry
    // resolves, it could hold back acks for the new connection generation
    // even though `finalize_publish` would correctly skip it once yielded.
    // Dropping BOTH halves of the old pair instead destroys the channel and
    // the ordered set wholesale (the stream is polled inline as `ack_stream`,
    // not by a spawned task), so nothing stale can survive into the new pair.
    fn new_finalizer(
        &self,
        shutdown: &ShutdownSignal,
    ) -> (
        Option<OrderedFinalizer<FinalizerEntry>>,
        BoxStream<'static, (BatchStatus, FinalizerEntry)>,
    ) {
        OrderedFinalizer::<FinalizerEntry>::maybe_new(self.acknowledgements, Some(shutdown.clone()))
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
                .try_subscribe(topic, SUBSCRIPTION_QOS)
                .map_err(|_| ()),
            OneOrMany::Many(topics) => client
                .try_subscribe_many(
                    topics
                        .iter()
                        .cloned()
                        .map(|topic| SubscribeFilter::new(topic, SUBSCRIPTION_QOS)),
                )
                .map_err(|_| ()),
        }
    }

    /// The configured topic filters in the order `subscribe` sends them,
    /// which per [MQTT-3.9.3-1] is also the order of a SUBACK's return codes.
    fn subscribed_topics(&self) -> impl Iterator<Item = &str> {
        match &self.config.topic {
            OneOrMany::One(topic) => std::slice::from_ref(topic),
            OneOrMany::Many(topics) => topics.as_slice(),
        }
        .iter()
        .map(String::as_str)
    }

    async fn process_message(
        &self,
        mut publish: Publish,
        out: &mut SourceSender,
        finalizer: Option<&OrderedFinalizer<FinalizerEntry>>,
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
    fn pending_acks_backpressure_matrix() {
        for (connected, try_ack_succeeds, expected_attempted, expected_queued) in [
            (false, true, false, false),
            (true, true, true, false),
            (true, false, true, true),
        ] {
            let mut pending_acks = PendingAcks::default();
            let mut attempted = false;

            pending_acks.try_ack_with(connected, publish(1), |_| {
                attempted = true;
                try_ack_succeeds
            });

            assert_eq!(attempted, expected_attempted);
            assert_eq!(!pending_acks.publishes.is_empty(), expected_queued);
        }
    }

    // A newer ack must never jump ahead of earlier acks still queued for
    // retry, even if the channel has room for it now -- PUBACKs go out in
    // publish-receipt order ([MQTT-4.6.0-2]).
    #[test]
    fn pending_acks_new_ack_queues_behind_earlier_failures() {
        let mut pending_acks = PendingAcks::default();

        // An earlier ack failed and is parked for retry.
        pending_acks.try_ack_with(true, publish(1), |_| false);
        assert_eq!(pending_acks.publishes.len(), 1);

        // The channel now has room, but the newer ack must still queue
        // behind the parked one rather than being sent.
        let mut attempted = false;
        pending_acks.try_ack_with(true, publish(2), |_| {
            attempted = true;
            true
        });
        assert!(!attempted);
        assert_eq!(
            pending_acks
                .publishes
                .iter()
                .map(|publish| publish.pkid)
                .collect::<Vec<_>>(),
            vec![1, 2],
            "retry order must match receipt order"
        );
    }

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
            assert!(actions.clear_pending_acks);
            assert!(actions.recreate_finalizer);
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
                clear_pending_acks: true,
                recreate_finalizer: true,
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
                clear_pending_acks: true,
                recreate_finalizer: true,
                resubscribe: true,
            }
        );
        assert_eq!(fresh_session.connection_generation, 2);
    }

    // Queueing a (re)subscribe request only means rumqttc accepted it
    // locally, not that the broker processed it: a disconnect can lose it
    // between queueing and actually reaching the broker, and the broker's
    // next SUBACK-less reconnect can report `session_present = true` for a
    // session that never got the subscription. So `pending_resubscribe` may
    // only be cleared by an actual SUBACK (`on_suback`), and a successfully
    // queued request must stop the loop from resending it every iteration
    // (`resubscribe_awaiting_suback`) without yet considering it done.
    #[test]
    fn protocol_contract_matrix_for_pending_resubscribe() {
        let mut state = ProtocolState::default();
        state.on_connack(true, true);
        state.on_disconnect();
        let actions = state.on_connack(true, false);
        assert!(actions.resubscribe);
        assert_eq!(
            state.loop_actions(),
            LoopActions {
                retry_pending_acks: true,
                retry_resubscribe: true,
            }
        );

        // Successfully queueing it stops the per-iteration resend, but does
        // not yet count as done: only a SUBACK can do that.
        state.on_resubscribe_result(true);
        assert_eq!(
            state.loop_actions(),
            LoopActions {
                retry_pending_acks: true,
                retry_resubscribe: false,
            }
        );

        // The queued request never reaches the broker (lost in a race with
        // a disconnect); failing to queue it again resumes the per-iteration
        // retry rather than leaving it stuck waiting for a SUBACK that will
        // never come.
        state.on_resubscribe_result(false);
        assert_eq!(
            state.loop_actions(),
            LoopActions {
                retry_pending_acks: true,
                retry_resubscribe: true,
            }
        );

        // Queued again, and this time a SUBACK actually confirms it.
        state.on_resubscribe_result(true);
        state.on_suback();
        assert_eq!(
            state.loop_actions(),
            LoopActions {
                retry_pending_acks: true,
                retry_resubscribe: false,
            }
        );
    }

    // A fresh-session reconnect whose queued SUBSCRIBE is lost (dropped
    // before reaching the broker) must not be considered resubscribed just
    // because a later reconnect happens to report `session_present = true`
    // for that now-subscription-less session: only a SUBACK may clear
    // `pending_resubscribe`.
    #[test]
    fn resubscribe_stays_pending_across_a_session_present_reconnect_without_suback() {
        let mut state = ProtocolState::default();
        state.on_connack(true, true);
        state.on_disconnect();
        state.on_connack(true, false);
        state.on_resubscribe_result(true);
        assert!(state.pending_resubscribe);

        // The connection drops again before a SUBACK arrives, and the next
        // reconnect reports the (empty) session as present.
        state.on_disconnect();
        let actions = state.on_connack(true, true);

        assert!(
            actions.resubscribe,
            "must still resubscribe: the previous attempt was never confirmed by a SUBACK, \
             even though this reconnect reports session_present = true"
        );
        assert!(state.pending_resubscribe);
        assert_eq!(
            state.loop_actions(),
            LoopActions {
                retry_pending_acks: true,
                retry_resubscribe: true,
            }
        );
    }

    #[test]
    fn protocol_contract_matrix_for_disconnect() {
        let mut state = ProtocolState::default();
        state.on_connack(true, true);

        let actions = state.on_disconnect();

        assert_eq!(
            actions,
            DisconnectActions {
                clear_pending_acks: true,
                recreate_finalizer: true,
            }
        );
        assert!(!state.connected);
        assert_eq!(state.connection_generation, 1);
        assert_eq!(
            state.loop_actions(),
            LoopActions {
                retry_pending_acks: false,
                retry_resubscribe: false,
            }
        );
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
                state.finalize_publish(status, entry_generation).should_ack,
                should_ack
            );
        }
    }

    // [MQTT-4.6.0-2] requires PUBACKs to be sent in the order their publishes
    // were received: once an earlier packet in a generation goes unacked
    // (Errored/Rejected), a later packet in that same generation must not be
    // acked either, even if it was Delivered -- or the ack order would be
    // violated. Only a reconnect (new generation) should resume acking.
    #[test]
    fn finalization_suppresses_later_acks_in_generation_after_a_failure() {
        let mut state = ProtocolState::default();
        state.on_connack(true, true);

        assert!(
            state
                .finalize_publish(BatchStatus::Delivered, 1)
                .should_ack
        );

        assert!(
            !state
                .finalize_publish(BatchStatus::Rejected, 1)
                .should_ack
        );
        assert!(
            !state
                .finalize_publish(BatchStatus::Delivered, 1)
                .should_ack
        );
        assert!(
            !state
                .finalize_publish(BatchStatus::Delivered, 1)
                .should_ack
        );

        // A reconnect starts a fresh generation, resuming normal acking.
        state.on_connack(true, false);
        assert!(
            state
                .finalize_publish(BatchStatus::Delivered, 2)
                .should_ack
        );
    }

    // A withheld ack must trigger a forced reconnect (so the broker
    // redelivers the withheld publish) exactly once per suppression episode,
    // not on every subsequent non-`Delivered` finalization in the same
    // generation -- the caller only needs one signal to act on, and a fresh
    // generation (after that reconnect) can trigger it again if it too fails.
    #[test]
    fn finalize_publish_signals_just_suppressed_once_per_episode() {
        let mut state = ProtocolState::default();
        state.on_connack(true, true);

        let first = state.finalize_publish(BatchStatus::Rejected, 1);
        assert!(!first.should_ack);
        assert!(first.just_suppressed);

        let second = state.finalize_publish(BatchStatus::Errored, 1);
        assert!(!second.should_ack);
        assert!(
            !second.just_suppressed,
            "already suppressed in this generation; must not re-signal"
        );

        // A new generation can independently signal suppression again.
        state.on_connack(true, false);
        let after_reconnect = state.finalize_publish(BatchStatus::Rejected, 2);
        assert!(after_reconnect.just_suppressed);

        // An out-of-generation (stale) entry never signals, even if its
        // status would otherwise suppress.
        let stale = state.finalize_publish(BatchStatus::Rejected, 1);
        assert!(!stale.should_ack);
        assert!(!stale.just_suppressed);
    }

    // A suppression that lands inside the cooldown window must reschedule the
    // forced reconnect for cooldown expiry, not skip it: `just_suppressed`
    // fires only once per connection generation, so a skipped reconnect never
    // gets another trigger and the source would stay connected (and stuck)
    // indefinitely.
    #[test]
    fn forced_reconnect_inside_cooldown_is_deferred_not_skipped() {
        let mut schedule = ForcedReconnectSchedule::default();
        let t0 = tokio::time::Instant::now();

        // First suppression, no cooldown yet: due immediately.
        schedule.schedule(t0);
        assert_eq!(schedule.due_at(), Some(t0));
        schedule.sent(t0);
        assert_eq!(schedule.due_at(), None);

        // The redelivered publish is rejected again 2s later, well within
        // the cooldown: due at cooldown expiry, not dropped.
        schedule.schedule(t0 + Duration::from_secs(2));
        assert_eq!(schedule.due_at(), Some(t0 + FORCED_RECONNECT_COOLDOWN));

        let t1 = t0 + FORCED_RECONNECT_COOLDOWN;
        schedule.sent(t1);
        assert_eq!(schedule.due_at(), None);

        // A suppression after the cooldown has fully expired is due
        // immediately again.
        let t2 = t1 + FORCED_RECONNECT_COOLDOWN + Duration::from_secs(1);
        schedule.schedule(t2);
        assert_eq!(schedule.due_at(), Some(t2));
    }

    #[test]
    fn forced_reconnect_schedule_retry_and_cancel() {
        let mut schedule = ForcedReconnectSchedule::default();
        let t0 = tokio::time::Instant::now();

        // Queueing the disconnect failed: retried shortly (not immediately,
        // which would let the timer branch monopolize the select loop).
        schedule.schedule(t0);
        let t1 = t0 + Duration::from_millis(10);
        schedule.retry(t1);
        assert_eq!(schedule.due_at(), Some(t1 + FORCED_RECONNECT_RETRY_DELAY));

        // A natural disconnect supersedes the scheduled one.
        schedule.cancel();
        assert_eq!(schedule.due_at(), None);
    }

    // rumqttc never closes its own side of the connection after sending
    // DISCONNECT, and the broker is only required to SHOULD-close: if it
    // doesn't, nothing else would ever end the generation whose acks are
    // suppressed. Sending the disconnect must therefore arm an escalation
    // deadline for closing the connection locally, cleared either by
    // escalating or by the disconnect actually happening.
    #[test]
    fn forced_reconnect_escalates_when_broker_never_closes() {
        let mut schedule = ForcedReconnectSchedule::default();
        let t0 = tokio::time::Instant::now();

        assert!(!schedule.disconnect_sent());
        schedule.schedule(t0);
        assert!(!schedule.disconnect_sent());

        schedule.sent(t0);
        assert!(schedule.disconnect_sent());
        assert_eq!(
            schedule.escalate_at(),
            Some(t0 + FORCED_RECONNECT_ESCALATION_TIMEOUT)
        );

        // The deadline fires: local teardown happens, nothing further armed.
        schedule.escalated();
        assert!(!schedule.disconnect_sent());
        assert_eq!(schedule.escalate_at(), None);

        // Alternate path: the broker closes in time, the resulting poll error
        // cancels both the schedule and the escalation.
        schedule.sent(t0 + Duration::from_secs(60));
        assert!(schedule.disconnect_sent());
        schedule.cancel();
        assert!(!schedule.disconnect_sent());
        assert_eq!(schedule.escalate_at(), None);
    }

    // The initial subscribe must be confirmed by a SUBACK exactly like a
    // re-subscribe: if the first SUBSCRIBE is lost after the broker created
    // the persistent session, the next reconnect reports
    // `session_present = true` for a session with no subscription, and
    // without this the source would never subscribe again.
    #[test]
    fn initial_subscribe_requires_suback_confirmation() {
        // As `run()` initializes it.
        let mut state = ProtocolState {
            pending_resubscribe: true,
            ..ProtocolState::default()
        };

        // First ConnAck must ask for the (initial) subscribe.
        let actions = state.on_connack(true, false);
        assert!(actions.resubscribe);
        state.on_resubscribe_result(true);

        // The SUBSCRIBE is lost; the connection drops; the broker resumes
        // the (subscription-less) session.
        state.on_disconnect();
        let actions = state.on_connack(true, true);
        assert!(
            actions.resubscribe,
            "initial subscribe was never SUBACK-confirmed, so a \
             session_present reconnect must still resubscribe"
        );

        // Once a SUBACK confirms it, later resumed sessions don't resend.
        state.on_resubscribe_result(true);
        state.on_suback();
        state.on_disconnect();
        let actions = state.on_connack(true, true);
        assert!(!actions.resubscribe);
    }
}
