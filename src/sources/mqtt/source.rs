use std::{
    collections::{HashSet, VecDeque},
    time::Duration,
};

use futures::{
    FutureExt, StreamExt,
    future::BoxFuture,
    stream::{BoxStream, FuturesUnordered},
};
use itertools::Itertools;
use rumqttc::{ConnectionError, Event as MqttEvent, Incoming, Publish, QoS, SubscribeFilter};
use vector_lib::{
    codecs::Decoder,
    config::{LegacyKey, LogNamespace},
    finalizer::OrderedFinalizer,
    internal_event::{EventsReceived, emit, register},
    lookup::path,
};

use crate::{
    SourceSender,
    common::mqtt::MqttConnector,
    event::{BatchNotifier, BatchStatus, Event},
    internal_events::{EndpointBytesReceived, MqttSubscribeError, StreamClosedError},
    serde::OneOrMany,
    shutdown::ShutdownSignal,
    sources::{SourceError, mqtt::MqttSourceConfig, util},
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
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent protocol flags, each with its own transition rules; the exhaustive model check enumerates their combinations."
)]
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
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent actions the ConnAck handler performs; a flag set keeps on_connack pure and testable."
)]
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

/// Schedules the forced reconnects triggered by withheld acks (see
/// `FORCED_RECONNECT_COOLDOWN`). A suppression inside the cooldown window
/// must schedule the reconnect for when the cooldown expires rather than skip
/// it: `just_suppressed` fires only once per connection generation, so a
/// skipped reconnect would never get another trigger and the source would
/// stay connected (and stuck) indefinitely.
///
/// The reconnect itself is a priority `DISCONNECT` (`try_disconnect_now`)
/// followed by rebuilding the client and event loop: in rumqttc-v4-next a
/// client-requested disconnect is terminal for its `EventLoop` (`poll` then
/// returns `RequestsDone` forever), so recovery needs a fresh one, which
/// reconnects with the same client ID and resumes the persistent session.
/// `disconnect_now` sends a real MQTT `DISCONNECT` (an abrupt socket drop was
/// verified against EMQX to stop delivery entirely), and dropping the old
/// `EventLoop` closes our side of the socket as [MQTT-3.14.4-2] requires --
/// no escalation timer is needed for a broker that never closes its side.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ForcedReconnectSchedule {
    due_at: Option<tokio::time::Instant>,
    cooldown_until: Option<tokio::time::Instant>,
    disconnect_sent: bool,
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

    /// The disconnect was queued: clear the schedule and start the cooldown.
    /// The event loop now winds down to `RequestsDone`, at which point `run`
    /// rebuilds it.
    fn sent(&mut self, now: tokio::time::Instant) {
        self.due_at = None;
        self.cooldown_until = Some(now + FORCED_RECONNECT_COOLDOWN);
        self.disconnect_sent = true;
    }

    /// A `DISCONNECT` has been queued and the event loop hasn't wound down
    /// yet, so nothing further should be queued for it ([MQTT-3.14.4-1]; the
    /// library also discards anything it receives after the barrier).
    const fn disconnect_sent(&self) -> bool {
        self.disconnect_sent
    }

    /// The connection is gone: whatever reconnect was scheduled has been
    /// achieved by that disconnect. The cooldown deliberately survives, so a
    /// suppression right after reconnecting is still paced.
    const fn cancel(&mut self) {
        self.due_at = None;
        self.disconnect_sent = false;
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

/// A publish rumqttc had parsed (into `state.events`) but this source had not
/// yet processed when its connection died. All of them are moved out of
/// rumqttc, in arrival order, by `take_buffered_publishes` and resolved by
/// the next `ConnAck` (see `run`), once the session outcome is known.
#[derive(Clone, Debug, PartialEq)]
struct BufferedPublish {
    publish: Publish,
    /// The broker still holds this publish unacknowledged and redelivers it
    /// if the next connection resumes the session, so forwarding our copy
    /// too would duplicate it. Only true for a manual-ack QoS 1/2 publish:
    /// - A QoS 0 publish is fire-and-forget in either ack mode.
    /// - A QoS 1/2 publish in auto-ack mode may already have had its ack
    ///   queued or sent by rumqttc the moment it was parsed.
    ///
    /// Dropping either of those would lose it outright, so they are
    /// forwarded whatever the session outcome.
    redelivered_if_session_resumes: bool,
}

/// Empties rumqttc's buffer of parsed-but-unyielded packets from a dead
/// connection, returning its publishes in arrival order.
///
/// Everything else in the buffer is stale control traffic and is dropped.
///
/// The publishes are held by this source rather than left in the buffer for
/// two reasons: rumqttc would yield them after the next `ConnAck` tagged as
/// new-connection traffic, and a forced reconnect replaces the `EventLoop`
/// (see `ForcedReconnectSchedule`), which would discard them.
fn take_buffered_publishes(
    events: &mut VecDeque<MqttEvent>,
    acknowledgements: bool,
) -> impl Iterator<Item = BufferedPublish> {
    std::mem::take(events)
        .into_iter()
        .filter_map(move |event| match event {
            MqttEvent::Incoming(Incoming::Publish(publish)) => Some(BufferedPublish {
                redelivered_if_session_resumes: acknowledgements
                    && publish_supports_end_to_end_acknowledgements(publish.qos),
                publish,
            }),
            _ => None,
        })
}

/// Which configured topic filters the broker's session is not yet known to
/// hold, so a retry after a partly rejected SUBSCRIBE resends only what is
/// still missing. Resending a filter the broker already granted is not
/// harmless: an MQTT 3.1.1 broker answers every SUBSCRIBE by delivering the
/// matching retained messages again, so one permanently denied filter would
/// otherwise re-emit the other filters' retained messages on every retry.
#[derive(Debug, Default)]
struct SubscriptionTracker {
    /// Filters not yet granted in the broker's session, in configured order.
    unconfirmed: Vec<String>,
}

#[derive(Debug, Default, Eq, PartialEq)]
struct SubAckOutcome {
    granted: Vec<(String, QoS)>,
    rejected: Vec<String>,
    /// Every configured filter is now granted.
    all_confirmed: bool,
}

impl SubscriptionTracker {
    /// The broker's session holds none of the filters: before the first
    /// subscribe, and whenever a `ConnAck` reports a fresh session.
    fn reset<'a>(&mut self, filters: impl Iterator<Item = &'a str>) {
        self.unconfirmed = filters.map(str::to_owned).collect();
    }

    /// Applies the SUBACK for a SUBSCRIBE that carried `filters`. Its return
    /// codes are in the SUBSCRIBE's filter order ([MQTT-3.9.3-1]), and
    /// rumqttc has already checked there is exactly one per filter.
    fn on_suback(
        &mut self,
        filters: Vec<String>,
        return_codes: &[rumqttc::SubscribeReasonCode],
    ) -> SubAckOutcome {
        let mut outcome = SubAckOutcome::default();
        for (filter, return_code) in filters.into_iter().zip(return_codes) {
            match return_code {
                rumqttc::SubscribeReasonCode::Success(qos) => {
                    self.unconfirmed
                        .retain(|unconfirmed| *unconfirmed != filter);
                    outcome.granted.push((filter, *qos));
                }
                rumqttc::SubscribeReasonCode::Failure => outcome.rejected.push(filter),
            }
        }
        outcome.all_confirmed = self.unconfirmed.is_empty();
        outcome
    }
}

/// The SUBSCRIBE requests rumqttc currently holds for this source, each
/// resolving to the filters it carried and its outcome.
///
/// These are rumqttc's tracked subscribes, used so this source never has to
/// guess which SUBSCRIBE a SUBACK answers or whether a request survived a
/// reconnect. rumqttc matches each SUBACK to its SUBSCRIBE by packet id, and
/// owns the request until it resolves:
/// - It replays a SUBSCRIBE that has no SUBACK yet (sent or not) on the next
///   connection when the session is resumed. Queueing another one then would
///   make the broker deliver the retained messages twice, so a reconnect
///   with a request still outstanding sends nothing.
/// - It drops them when the broker starts a fresh session, and they die with
///   the `EventLoop` when a forced reconnect replaces it. `run` discards
///   them here in both cases and subscribes again.
type SubscribeResults = FuturesUnordered<
    BoxFuture<
        'static,
        (
            Vec<String>,
            Result<rumqttc::SubAck, rumqttc::SubscribeNoticeError>,
        ),
    >,
>;

/// The packet ids of the manual-ack publishes received on the current
/// connection whose acknowledgement has not been sent yet.
///
/// A broker may retransmit an unacknowledged QoS 1/2 publish on the same
/// connection (EMQX does, on its retry interval). It is the same message
/// under the same packet id, and its first copy is already on its way to the
/// sinks, so the retransmission is dropped. Processing it would register a
/// second acknowledgement for that packet id: once the first is sent the
/// second is stale, and rumqttc either rejects it (tearing down the
/// connection, see `handle_connection_lost`) or, if the broker has reused
/// the id by then, acknowledges a newer publish before it was delivered.
///
/// An id is released at the point rumqttc itself starts treating a publish
/// with that id as new: when the PUBACK is written (QoS 1), or when the
/// broker's PUBREL arrives (QoS 2). An id whose ack is withheld stays here
/// until the reconnect that redelivers it.
#[derive(Debug, Default)]
struct InFlightPublishes {
    packet_ids: HashSet<u16>,
}

impl InFlightPublishes {
    /// Records a newly received publish. Returns `false` for a
    /// retransmission of one that is still in flight.
    fn admit(&mut self, packet_id: u16) -> bool {
        self.packet_ids.insert(packet_id)
    }

    fn release(&mut self, packet_id: u16) {
        self.packet_ids.remove(&packet_id);
    }

    fn clear(&mut self) {
        self.packet_ids.clear();
    }
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

pub(super) struct MqttSource {
    connector: MqttConnector,
    decoder: Decoder,
    log_namespace: LogNamespace,
    config: MqttSourceConfig,
    acknowledgements: bool,
}

/// The subset of `run`'s per-connection retry/pending state that
/// `handle_connection_lost` needs, bundled into one borrow purely to stay
/// under `clippy::too_many_arguments` -- each field is still owned and used
/// individually everywhere else in `run`'s loop.
/// Queues a SUBSCRIBE for the filters still unconfirmed. Returns whether it
/// was queued; a failure (rumqttc's request channel is full) is retried by
/// the caller while connected.
fn try_subscribe(
    client: &rumqttc::AsyncClient,
    subscriptions: &SubscriptionTracker,
    subscribe_results: &SubscribeResults,
) -> bool {
    let filters = subscriptions.unconfirmed.clone();
    let request = filters
        .iter()
        .cloned()
        .map(|filter| SubscribeFilter::new(filter, SUBSCRIPTION_QOS));
    match client.try_subscribe_many_tracked(request) {
        Ok(notice) => {
            subscribe_results.push(async move { (filters, notice.wait_async().await) }.boxed());
            true
        }
        Err(error) => {
            emit(MqttSubscribeError { error });
            false
        }
    }
}

struct ConnectionAuxState<'a> {
    pending_acks: &'a mut PendingAcks,
    forced_reconnect: &'a mut ForcedReconnectSchedule,
    resubscribe_retry_at: &'a mut Option<tokio::time::Instant>,
    buffered_publishes: &'a mut Vec<BufferedPublish>,
    in_flight_publishes: &'a mut InFlightPublishes,
}

impl MqttSource {
    #[allow(
        clippy::unnecessary_wraps,
        reason = "Removing this fallible API is deferred to keep source construction and error handling unchanged. https://github.com/vectordotdev/vector/issues/23659"
    )]
    pub(super) fn new(
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

    #[allow(
        clippy::too_many_lines,
        reason = "One select! loop over the connection, ack stream and timers; its arms already delegate to helpers."
    )]
    pub(super) async fn run(
        self,
        mut out: SourceSender,
        shutdown: ShutdownSignal,
    ) -> Result<(), SourceError> {
        let (mut client, mut connection) = self.connector.connect();

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

        // Which topic filters still need a (re)subscribe; see
        // `SubscriptionTracker`.
        let mut subscriptions = SubscriptionTracker::default();
        subscriptions.reset(self.subscribed_topics());
        let mut subscribe_results = SubscribeResults::new();

        // See `InFlightPublishes`. Unused when acknowledgements are disabled.
        let mut in_flight_publishes = InFlightPublishes::default();

        // Publishes rumqttc had buffered but this source hadn't yet processed
        // when their connection died, in arrival order. Whether the
        // manual-ack QoS 1/2 ones must be forwarded or dropped depends on
        // whether the next reconnect resumes the persisted session -- not
        // known until that ConnAck arrives, so they all wait here until
        // then; see `handle_connection_lost` (where they're extracted) and
        // the ConnAck handler below (where they're resolved).
        let mut buffered_publishes: Vec<BufferedPublish> = Vec::new();

        loop {
            // Once a graceful `Disconnect` has been queued, nothing else may
            // be written to the connection ([MQTT-3.14.4-1]); the retries
            // resume after the reconnect it causes.
            let draining = forced_reconnect.disconnect_sent();

            let actions = protocol_state.loop_actions();
            if actions.retry_resubscribe && !draining {
                protocol_state.on_resubscribe_result(try_subscribe(
                    &client,
                    &subscriptions,
                    &subscribe_results,
                ));
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
                // A scheduled forced reconnect is due: queue a priority MQTT
                // `DISCONNECT` so the broker redelivers the withheld publish
                // once a fresh event loop reconnects (see
                // `ForcedReconnectSchedule`; the rebuild happens when this
                // event loop reports `RequestsDone`, in the poll arm below).
                () = tokio::time::sleep_until(
                    forced_reconnect.due_at().unwrap_or_else(tokio::time::Instant::now)
                ), if forced_reconnect.due_at().is_some() && protocol_state.connected => {
                    // `disconnect_now` uses an unbounded channel, so this can
                    // only fail if the event loop is gone -- impossible while
                    // `run` owns it. Mark the reconnect sent either way so the
                    // cooldown still applies.
                    if let Err(error) = client.try_disconnect_now() {
                        warn!(message = "Failed to queue MQTT forced-reconnect disconnect.", %error);
                    } else {
                        warn_forcing_reconnect_after_suppressed_ack();
                    }
                    forced_reconnect.sent(tokio::time::Instant::now());
                },
                // The broker rejected a (re)subscribe earlier; the pacing
                // delay has passed, so try again.
                () = tokio::time::sleep_until(
                    resubscribe_retry_at.unwrap_or_else(tokio::time::Instant::now)
                ), if resubscribe_retry_at.is_some() => {
                    resubscribe_retry_at = None;
                    if protocol_state.connected
                        && protocol_state.pending_resubscribe
                        && !forced_reconnect.disconnect_sent()
                    {
                        protocol_state.on_resubscribe_result(try_subscribe(
                    &client,
                    &subscriptions,
                    &subscribe_results,
                ));
                    }
                },
                // A SUBSCRIBE resolved. A SUBACK only confirms the broker
                // *processed* it; each return code says whether the matching
                // topic filter was actually granted. A rejected filter
                // (failure code 0x80, e.g. an ACL denial) must not count as
                // subscribed -- keep retrying it (and only it, see
                // `SubscriptionTracker`), paced by `RESUBSCRIBE_RETRY_DELAY`.
                Some((filters, result)) = subscribe_results.next(), if !subscribe_results.is_empty() => {
                    match result {
                        Ok(suback) => {
                            let outcome = subscriptions.on_suback(filters, &suback.return_codes);
                            for (topic, qos) in &outcome.granted {
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
                            for topic in &outcome.rejected {
                                error!(
                                    message = "MQTT broker rejected the subscription; will retry.",
                                    topic,
                                    internal_log_rate_limit = true,
                                );
                            }
                            if outcome.all_confirmed {
                                protocol_state.on_suback();
                                resubscribe_retry_at = None;
                            } else {
                                resubscribe_retry_at = Some(
                                    tokio::time::Instant::now() + RESUBSCRIBE_RETRY_DELAY,
                                );
                            }
                        }
                        // rumqttc gave up the request without a SUBACK. The
                        // cases it does that in (fresh session, replaced
                        // event loop) are handled where they happen, so this
                        // is only a backstop: if nothing else is outstanding,
                        // let the loop queue the subscribe again.
                        Err(error) => {
                            debug!(message = "MQTT subscribe request was dropped before its SUBACK.", %error);
                            if subscribe_results.is_empty() {
                                protocol_state.on_resubscribe_result(false);
                            }
                        }
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
                            // A retransmission of a publish whose ack is
                            // still outstanding is dropped; see
                            // `InFlightPublishes`.
                            let defers_ack =
                                publish_ack_decision(self.acknowledgements, publish.qos).defer_ack;
                            if defers_ack && !in_flight_publishes.admit(publish.pkid) {
                                debug!(
                                    message = "Dropping an MQTT publish retransmitted while its acknowledgement is still pending.",
                                    packet_id = publish.pkid,
                                    internal_log_rate_limit = true,
                                );
                            } else {
                                self.process_message(
                                    publish,
                                    &mut out,
                                    finalizer
                                        .as_ref()
                                        .map(|finalizer| (finalizer, protocol_state.connection_generation)),
                                ).await;
                            }
                        }
                        // The acknowledgement for this packet id is on its
                        // way, so the next publish carrying it is a new one.
                        Ok(MqttEvent::Outgoing(rumqttc::Outgoing::PubAck(packet_id))) => {
                            in_flight_publishes.release(packet_id);
                        }
                        Ok(MqttEvent::Incoming(Incoming::PubRel(pubrel))) => {
                            in_flight_publishes.release(pubrel.pkid);
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
                            in_flight_publishes.clear();
                            // A fresh session holds none of our subscriptions,
                            // and rumqttc dropped the SUBSCRIBE requests it
                            // held for the old one (see `SubscribeResults`).
                            if !connack.session_present {
                                subscriptions.reset(self.subscribed_topics());
                                subscribe_results.clear();
                            }
                            // Publishes buffered but unprocessed when the previous
                            // connection died (see `handle_connection_lost`),
                            // forwarded in arrival order. The exception: if this
                            // reconnect resumed the persisted session, the broker
                            // redelivers the manual-ack QoS 1/2 ones itself --
                            // drop ours, or we'd duplicate them. If the session
                            // was lost, the broker has forgotten them entirely
                            // and ours is the only remaining copy.
                            //
                            // They are forwarded without a finalizer. Their
                            // packet ids belong to a dead connection, so there
                            // is nothing to acknowledge, and MQTT's ack ordering
                            // doesn't span connections: registering them in this
                            // connection's ordered finalizer would only hold
                            // back the acks of every publish received after
                            // them for as long as their batch stays pending.
                            for buffered in std::mem::take(&mut buffered_publishes) {
                                if buffered.redelivered_if_session_resumes
                                    && connack.session_present
                                {
                                    continue;
                                }
                                self.process_message(buffered.publish, &mut out, None)
                                    .await;
                            }
                            if actions.resubscribe {
                                // A SUBSCRIBE still outstanding from the
                                // previous connection is replayed by rumqttc
                                // on this one; sending another would make the
                                // broker deliver retained messages twice.
                                let queued = !subscribe_results.is_empty()
                                    || try_subscribe(&client, &subscriptions, &subscribe_results);
                                protocol_state.on_resubscribe_result(queued);
                            }
                            // A retry the previous connection had scheduled no
                            // longer applies; this connection's subscribe is
                            // outstanding (or already confirmed).
                            resubscribe_retry_at = None;
                        }
                        // Connection lost: `poll()` already called
                        // `EventLoop::clean()` internally before returning this
                        // error; scrub everything from the dead connection that
                        // rumqttc would otherwise carry over into the next one.
                        Err(error) => {
                            (finalizer, ack_stream) = self.handle_connection_lost(
                                &mut connection,
                                &mut protocol_state,
                                ConnectionAuxState {
                                    pending_acks: &mut pending_acks,
                                    forced_reconnect: &mut forced_reconnect,
                                    resubscribe_retry_at: &mut resubscribe_retry_at,
                                    buffered_publishes: &mut buffered_publishes,
                                    in_flight_publishes: &mut in_flight_publishes,
                                },
                                &shutdown,
                            );
                            // Our forced-reconnect `DISCONNECT` completed. That
                            // is terminal for this event loop (every later poll
                            // returns `RequestsDone`), so replace it: the fresh
                            // one reconnects with the same client ID and
                            // resumes the persistent session. Only `run` issues
                            // disconnects, and it holds `client`, so nothing
                            // else ends polling this way.
                            if matches!(error, ConnectionError::RequestsDone) {
                                (client, connection) = self.connector.connect();
                                // The requests the old event loop held are
                                // gone with it; the next ConnAck subscribes
                                // again if anything is still unconfirmed.
                                subscribe_results.clear();
                            }
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
    ///
    /// # rumqttc state inventory across a reconnect
    ///
    /// rumqttc reuses one `EventLoop` (and its `MqttState`) across ordinary
    /// reconnects; `EventLoop::clean()`, run inside `poll()` before it returns
    /// a connection error, is the only fix-up between connections. Everything
    /// it does not reset either needs scrubbing here or needs an argument for
    /// why it's safe. The complete inventory, verified against
    /// rumqttc-v4-next 0.34 (`eventloop.rs`, `state.rs`, `framed.rs`):
    ///
    /// Reset or handled by rumqttc itself -- nothing for us to do:
    /// - The network (and with it the `Framed` read and write buffers),
    ///   `keepalive_timeout`, any pending graceful disconnect, ping state,
    ///   `inflight`, and `incoming_puback` (QoS 1 receive tracking is not
    ///   session state).
    /// - The private replay queue: `clean()` replays queued requests on the
    ///   next connection *except* `PubAck`/`PubRec`, whose packet ids belong
    ///   to the dead connection. This source relies on that instead of
    ///   scrubbing the queue (pinned by `connection_lost_scrubs_the_rumqttc_seam`,
    ///   so an upgrade that starts replaying acks fails a test). A SUBSCRIBE
    ///   with no SUBACK yet is among the replayed requests, which is why a
    ///   reconnect with one outstanding sends no new one (see
    ///   `SubscribeResults`; pinned by the same test).
    /// - `session_present = false`: `reset_session_state` rebuilds
    ///   `MqttState` wholesale and fails tracked requests.
    ///
    /// Persists, scrubbed here:
    /// - `state.events`: incoming packets parsed but not yet yielded.
    ///   `select()` drains this before reading the network, but `readb()`
    ///   parses a whole burst and returns `ConnectionAborted` (or a
    ///   deserialization error) if the burst's next frame is EOF or
    ///   malformed -- so a broker that closes the connection right after a
    ///   burst of publishes leaves them buffered at error time. Emptied by
    ///   `take_buffered_publishes`: the publishes wait in
    ///   `buffered_publishes` for the next `ConnAck`'s session outcome, the
    ///   rest is dropped. Left in place, they would be yielded after the
    ///   next `ConnAck` and tagged with the new generation (or discarded
    ///   with the `EventLoop` by a forced reconnect).
    ///
    /// Kept in this source even though rumqttc validates acks:
    /// - Generation tagging (`FinalizerEntry::connection_generation`).
    ///   rumqttc rejects an ack for a packet id it has no record of on the
    ///   current connection, but not one the broker has reused for a
    ///   *different* publish on a fresh session -- acking that would lose
    ///   it. And a rejected ack is not a no-op: it surfaces as
    ///   `StateError::Unsolicited` out of `poll()` and tears down a healthy
    ///   connection. So stale acks must never be sent at all.
    ///
    /// Not a reconnect -- a replacement:
    /// - A client-requested `DISCONNECT` is terminal for its `EventLoop`
    ///   (`poll()` returns `RequestsDone` forever), so the forced reconnect
    ///   rebuilds the client and event loop instead (see
    ///   `ForcedReconnectSchedule`). Dropping the old `EventLoop` closes our
    ///   side of the socket.
    ///
    /// Persists, safe by construction (do not "fix" these without a reason):
    /// - `state.collision`: only ever set by outgoing publishes, which this
    ///   source never sends; if it could be set, it would gate the request
    ///   path and starve acks -- a reminder for any future publish path.
    /// - Outbound packet-id allocation: used only for SUBSCRIBE here, and
    ///   rumqttc matches each SUBACK to its SUBSCRIBE itself.
    ///
    /// Residual risks that cannot be closed at this layer (documented so
    /// they're a decision, not an oversight):
    /// - Bytes received but not yet decoded live in the private `Framed`
    ///   read buffer and die with the network. If the next reconnect also
    ///   loses the session, those publishes are gone; the at-least-once
    ///   guarantee is conditional on session retention, as the user docs
    ///   state.
    /// - Auto-ack mode acks a publish as rumqttc parses it, before we
    ///   process it; a crash in that window loses it (identical to master's
    ///   behavior; manual-ack mode exists to close exactly this gap).
    fn handle_connection_lost(
        &self,
        connection: &mut rumqttc::EventLoop,
        protocol_state: &mut ProtocolState,
        aux: ConnectionAuxState<'_>,
        shutdown: &ShutdownSignal,
    ) -> (
        Option<OrderedFinalizer<FinalizerEntry>>,
        BoxStream<'static, (BatchStatus, FinalizerEntry)>,
    ) {
        let ConnectionAuxState {
            pending_acks,
            forced_reconnect,
            resubscribe_retry_at,
            buffered_publishes,
            in_flight_publishes,
        } = aux;

        let actions = protocol_state.on_disconnect();
        if actions.clear_pending_acks {
            pending_acks.clear();
            // rumqttc buffers batches of incoming packets in `state.events`
            // and yields them one per poll; `clean()` does NOT clear that
            // buffer, so events from the dead connection would otherwise be
            // yielded *after* the next ConnAck and be indistinguishable from
            // new-connection traffic. Everything buffered here belongs to the
            // dead connection (the reconnect ConnAck is returned directly by
            // `poll()`, never through this buffer), so empty it: the
            // publishes wait, in arrival order, for the ConnAck handler to
            // resolve them once the session outcome is known.
            buffered_publishes.extend(take_buffered_publishes(
                &mut connection.state.events,
                self.acknowledgements,
            ));
        }
        in_flight_publishes.clear();
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

    /// The configured topic filters.
    fn subscribed_topics(&self) -> impl Iterator<Item = &str> {
        match &self.config.topic {
            OneOrMany::One(topic) => std::slice::from_ref(topic),
            OneOrMany::Many(topics) => topics.as_slice(),
        }
        .iter()
        .map(String::as_str)
    }

    /// Decodes a publish and sends its events downstream. `finalizer` is the
    /// current connection's ordered finalizer and generation; `None` when
    /// acknowledgements are disabled or the publish can no longer be
    /// acknowledged (it arrived on a connection that is gone).
    async fn process_message(
        &self,
        mut publish: Publish,
        out: &mut SourceSender,
        finalizer: Option<(&OrderedFinalizer<FinalizerEntry>, u64)>,
    ) {
        emit(EndpointBytesReceived {
            byte_size: publish.payload.len(),
            protocol: "mqtt",
            endpoint: &self.config.common.host,
        });
        let events_received = register(EventsReceived);

        let ack_decision = publish_ack_decision(finalizer.is_some(), publish.qos);
        if ack_decision.warn_unsupported_qos {
            warn_unsupported_acknowledgement_qos(
                publish.qos,
                &String::from_utf8_lossy(&publish.topic),
            );
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
                if let Some(((finalizer, connection_generation), receiver)) =
                    finalizer.zip(batch_receiver)
                {
                    publish.payload = bytes::Bytes::default();
                    finalizer.add(
                        FinalizerEntry {
                            publish,
                            connection_generation,
                        },
                        receiver,
                    );
                }
            }
            Err(_) => emit(StreamClosedError { count }),
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

    // What `take_buffered_publishes` does with a dead connection's
    // `state.events`: every publish is taken, in arrival order (an
    // interleaved QoS 0 / QoS 1 burst must not be reordered by resolving the
    // two kinds separately), and only manual-ack QoS 1/2 publishes are
    // marked as redelivered by a resumed session -- QoS 0 has no redelivery
    // in either mode, and auto-ack QoS 1/2 may already be acked. Non-publish
    // events are always stale and dropped: keeping a SubAck would falsely
    // confirm the new connection's subscribe.
    #[test]
    fn take_buffered_publishes_keeps_arrival_order_and_drops_control_traffic() {
        let publish_with_qos = |pkid, qos| {
            let mut p = publish(pkid);
            p.qos = qos;
            MqttEvent::Incoming(Incoming::Publish(p))
        };
        let buffered = || {
            VecDeque::from([
                publish_with_qos(1, QoS::AtMostOnce),
                publish_with_qos(2, QoS::AtLeastOnce),
                MqttEvent::Incoming(Incoming::SubAck(rumqttc::SubAck::new(
                    99,
                    vec![rumqttc::SubscribeReasonCode::Success(QoS::AtLeastOnce)],
                ))),
                publish_with_qos(3, QoS::AtMostOnce),
                MqttEvent::Outgoing(rumqttc::Outgoing::PingReq),
                publish_with_qos(4, QoS::ExactlyOnce),
            ])
        };

        for (acknowledgements, expected) in [
            (true, [(1, false), (2, true), (3, false), (4, true)]),
            (false, [(1, false), (2, false), (3, false), (4, false)]),
        ] {
            let mut events = buffered();
            let taken = take_buffered_publishes(&mut events, acknowledgements)
                .map(|buffered| {
                    (
                        buffered.publish.pkid,
                        buffered.redelivered_if_session_resumes,
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(taken, expected, "acknowledgements={acknowledgements}");
            assert!(
                events.is_empty(),
                "nothing may be left for rumqttc to yield"
            );
        }
    }

    fn tracker(filters: &[&str]) -> SubscriptionTracker {
        let mut tracker = SubscriptionTracker::default();
        tracker.reset(filters.iter().copied());
        tracker
    }

    // A partly rejected SUBSCRIBE must be retried with only the rejected
    // filters: an MQTT 3.1.1 broker redelivers retained messages for every
    // filter in every SUBSCRIBE, so resending granted filters would re-emit
    // their retained messages on each retry of a permanently denied one.
    #[test]
    fn subscription_tracker_retries_only_rejected_filters() {
        use rumqttc::SubscribeReasonCode::{Failure, Success};

        let mut tracker = tracker(&["a", "denied", "c"]);
        let request = tracker.unconfirmed.clone();
        assert_eq!(request, ["a", "denied", "c"]);

        let outcome = tracker.on_suback(
            request,
            &[Success(QoS::AtLeastOnce), Failure, Success(QoS::AtMostOnce)],
        );
        assert_eq!(
            outcome,
            SubAckOutcome {
                granted: vec![
                    ("a".to_owned(), QoS::AtLeastOnce),
                    ("c".to_owned(), QoS::AtMostOnce)
                ],
                rejected: vec!["denied".to_owned()],
                all_confirmed: false,
            }
        );

        // The retry carries only the rejected filter, and its single return
        // code is applied to that filter rather than to the first configured
        // one.
        let request = tracker.unconfirmed.clone();
        assert_eq!(request, ["denied"]);
        let outcome = tracker.on_suback(request, &[Success(QoS::AtLeastOnce)]);
        assert_eq!(
            outcome,
            SubAckOutcome {
                granted: vec![("denied".to_owned(), QoS::AtLeastOnce)],
                rejected: vec![],
                all_confirmed: true,
            }
        );
    }

    // A fresh session holds none of the filters, so everything is resent,
    // including what an earlier SUBACK had granted. A SUBACK is applied to
    // the filters its own SUBSCRIBE carried, so one resolving late (rumqttc
    // replays an unanswered SUBSCRIBE when the session is resumed) cannot be
    // credited to the filters of a different request.
    #[test]
    fn subscription_tracker_follows_the_session() {
        use rumqttc::SubscribeReasonCode::{Failure, Success};

        let mut tracker = tracker(&["a", "b"]);
        let first = tracker.unconfirmed.clone();
        tracker.on_suback(first, &[Success(QoS::AtLeastOnce), Failure]);
        assert_eq!(tracker.unconfirmed, ["b"]);

        tracker.reset(["a", "b"].into_iter());
        assert_eq!(tracker.unconfirmed, ["a", "b"]);

        // A SUBACK for a request that carried only "b".
        let outcome = tracker.on_suback(vec!["b".to_owned()], &[Success(QoS::AtLeastOnce)]);
        assert!(!outcome.all_confirmed);
        assert_eq!(tracker.unconfirmed, ["a"]);
    }

    // A broker may retransmit an unacknowledged publish on the same
    // connection. Processing it would register a second ack for one packet
    // id; once the first is sent, the second is stale -- rumqttc rejects it
    // (tearing down the connection) or, if the broker reused the id, it
    // acknowledges a newer publish early. So a retransmission is refused
    // until the id is released, after which the id denotes a new publish.
    #[test]
    fn in_flight_publishes_refuse_retransmissions_until_released() {
        let mut in_flight = InFlightPublishes::default();

        assert!(in_flight.admit(5));
        assert!(in_flight.admit(6));
        assert!(!in_flight.admit(5), "retransmission while unacked");

        // PUBACK written (QoS 1) or PUBREL received (QoS 2): the id is free.
        in_flight.release(5);
        assert!(in_flight.admit(5), "the broker reused the id");
        assert!(!in_flight.admit(6));

        // A reconnect redelivers everything unacked under fresh state.
        in_flight.clear();
        assert!(in_flight.admit(5));
        assert!(in_flight.admit(6));
    }

    // `InFlightPublishes` releases a packet id on the events a REAL rumqttc
    // state emits, so this pins that library contract: a manual-ack publish
    // is yielded again if the broker retransmits it before the ack, and the
    // ack yields `Outgoing::PubAck` (QoS 1) while the broker's PUBREL is
    // yielded as `Incoming::PubRel` (QoS 2). An upgrade that stops emitting
    // either would leave ids unreleased -- every later publish reusing one
    // would be dropped as a retransmission -- so it must fail here instead.
    #[test]
    fn rumqttc_emits_the_events_that_release_in_flight_packet_ids() {
        let mut options = rumqttc::MqttOptions::new("in-flight-test", ("localhost", 1883));
        options.set_ack_mode(rumqttc::AckMode::Manual);
        let (_client, mut connection) = rumqttc::AsyncClient::builder(options).capacity(8).build();
        let state = &mut connection.state;

        let qos1 = publish(5);
        let mut qos2 = publish(6);
        qos2.qos = QoS::ExactlyOnce;

        // QoS 1: the publish, a retransmission before the ack, then the ack.
        state
            .handle_incoming_packet(Incoming::Publish(qos1.clone()))
            .unwrap();
        state
            .handle_incoming_packet(Incoming::Publish(qos1.clone()))
            .unwrap();
        state
            .handle_outgoing_packet(rumqttc::Request::PubAck(rumqttc::PubAck::new(5)))
            .unwrap();
        // A second ack for the same id is what the retransmission would have
        // queued; rumqttc rejects it, which tears down the connection.
        assert!(
            state
                .handle_outgoing_packet(rumqttc::Request::PubAck(rumqttc::PubAck::new(5)))
                .is_err()
        );

        // QoS 2: the publish, its PUBREC, then the broker's PUBREL.
        state
            .handle_incoming_packet(Incoming::Publish(qos2))
            .unwrap();
        state
            .handle_outgoing_packet(rumqttc::Request::PubRec(rumqttc::PubRec::new(6)))
            .unwrap();
        state
            .handle_incoming_packet(Incoming::PubRel(rumqttc::PubRel::new(6)))
            .unwrap();

        let events = Vec::from(std::mem::take(&mut state.events));
        assert_eq!(
            events
                .iter()
                .filter(|event| **event == MqttEvent::Incoming(Incoming::Publish(qos1.clone())))
                .count(),
            2,
            "the retransmission must reach the source to be dropped there"
        );
        assert!(events.contains(&MqttEvent::Outgoing(rumqttc::Outgoing::PubAck(5))));
        assert!(events.contains(&MqttEvent::Incoming(Incoming::PubRel(
            rumqttc::PubRel::new(6)
        ))));
    }

    // The disconnect scrub exercised end-to-end against a REAL rumqttc
    // `EventLoop` (constructed without a network -- everything it carries
    // across reconnects is reachable), so the seam every recent review
    // finding has probed -- what survives from a dead connection into the
    // next one -- is pinned by deterministic assertions instead of
    // live-broker timing, which cannot reliably reach these states. The
    // expected values are derived from the state inventory documented on
    // `handle_connection_lost`.
    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "One end-to-end scenario: set up every piece of cross-reconnect state, run the handler, assert each postcondition."
    )]
    fn connection_lost_scrubs_the_rumqttc_seam() {
        let publish_with_qos = |pkid, qos| {
            let mut publish = publish(pkid);
            publish.qos = qos;
            publish
        };

        for acknowledgements in [true, false] {
            let source = MqttSource::new(
                MqttConnector::new(rumqttc::MqttOptions::new("seam-test", ("localhost", 1883))),
                Decoder::default(),
                LogNamespace::Legacy,
                MqttSourceConfig::default(),
                acknowledgements,
            )
            .unwrap();
            let (client, mut connection) = rumqttc::AsyncClient::builder(
                rumqttc::MqttOptions::new("seam-test", ("localhost", 1883)),
            )
            .capacity(8)
            .build();

            // What rumqttc can hold at the moment a connection dies: parsed-
            // but-unyielded incoming packets, and queued-but-unsent requests.
            connection.state.events = VecDeque::from([
                MqttEvent::Incoming(Incoming::Publish(publish_with_qos(1, QoS::AtMostOnce))),
                MqttEvent::Incoming(Incoming::Publish(publish_with_qos(2, QoS::AtLeastOnce))),
                MqttEvent::Incoming(Incoming::SubAck(rumqttc::SubAck::new(
                    9,
                    vec![rumqttc::SubscribeReasonCode::Success(QoS::AtLeastOnce)],
                ))),
                MqttEvent::Incoming(Incoming::Publish(publish_with_qos(3, QoS::ExactlyOnce))),
                MqttEvent::Outgoing(rumqttc::Outgoing::PingReq),
            ]);
            // Queued-but-unsent requests: a stale PUBACK and a SUBSCRIBE.
            client
                .try_manual_ack(rumqttc::ManualAck::PubAck(rumqttc::PubAck::new(2)))
                .unwrap();
            let _notice = client
                .try_subscribe_many_tracked([SubscribeFilter::new(
                    "topic".to_owned(),
                    QoS::AtLeastOnce,
                )])
                .unwrap();

            // Vector-side per-connection state, all armed.
            let mut protocol_state = ProtocolState {
                pending_resubscribe: true,
                ..ProtocolState::default()
            };
            protocol_state.on_connack(acknowledgements, false);
            let generation = protocol_state.connection_generation;
            let mut pending_acks = PendingAcks::default();
            pending_acks.push(publish(7));
            let now = tokio::time::Instant::now();
            let mut forced_reconnect = ForcedReconnectSchedule::default();
            forced_reconnect.sent(now);
            forced_reconnect.schedule(now);
            let mut resubscribe_retry_at = Some(now + Duration::from_secs(5));
            let mut buffered_publishes: Vec<BufferedPublish> = Vec::new();
            let mut in_flight_publishes = InFlightPublishes::default();
            in_flight_publishes.admit(2);

            // What `poll()` runs internally before returning a connection
            // error.
            connection.clean();

            let shutdown = ShutdownSignal::noop();
            let (finalizer, _ack_stream) = source.handle_connection_lost(
                &mut connection,
                &mut protocol_state,
                ConnectionAuxState {
                    pending_acks: &mut pending_acks,
                    forced_reconnect: &mut forced_reconnect,
                    resubscribe_retry_at: &mut resubscribe_retry_at,
                    buffered_publishes: &mut buffered_publishes,
                    in_flight_publishes: &mut in_flight_publishes,
                },
                &shutdown,
            );

            // Vector-side state: disconnected, same generation, in-flight
            // packet ids forgotten, every per-connection schedule
            // cancelled -- except the reconnect-rate cooldown, which
            // deliberately survives so a suppression right after the
            // reconnect is still paced.
            assert!(!protocol_state.connected);
            assert_eq!(protocol_state.connection_generation, generation);
            assert!(pending_acks.publishes.is_empty());
            assert_eq!(forced_reconnect.due_at(), None);
            assert!(!forced_reconnect.disconnect_sent());
            assert!(forced_reconnect.cooldown_until.is_some());
            assert_eq!(resubscribe_retry_at, None);
            assert!(in_flight_publishes.packet_ids.is_empty());
            assert_eq!(finalizer.is_some(), acknowledgements);

            // rumqttc's replay queue (private since rumqttc-v4-next, so
            // observed by length): `clean()` must replay the SUBSCRIBE and
            // drop the stale PUBACK, whose packet id belongs to the dead
            // connection. This source relies on that instead of scrubbing
            // the queue itself, so this pins the library contract: an
            // upgrade that starts replaying acks fails here, not in
            // production.
            assert_eq!(connection.pending_len(), 1);

            // rumqttc's buffer of parsed-but-unyielded packets is empty:
            // every publish waits, in arrival order, for the next ConnAck's
            // session outcome, and the stale control traffic is gone. Only
            // manual-ack QoS 1/2 publishes are redelivered by a resumed
            // session; in auto-ack mode every publish may already be acked
            // broker-side.
            assert!(connection.state.events.is_empty());
            assert_eq!(
                buffered_publishes
                    .iter()
                    .map(|buffered| (
                        buffered.publish.pkid,
                        buffered.redelivered_if_session_resumes
                    ))
                    .collect::<Vec<_>>(),
                vec![(1, false), (2, acknowledgements), (3, acknowledgements)]
            );
        }
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

        assert!(state.finalize_publish(BatchStatus::Delivered, 1).should_ack);

        assert!(!state.finalize_publish(BatchStatus::Rejected, 1).should_ack);
        assert!(!state.finalize_publish(BatchStatus::Delivered, 1).should_ack);
        assert!(!state.finalize_publish(BatchStatus::Delivered, 1).should_ack);

        // A reconnect starts a fresh generation, resuming normal acking.
        state.on_connack(true, false);
        assert!(state.finalize_publish(BatchStatus::Delivered, 2).should_ack);
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
        let stale_entry = state.finalize_publish(BatchStatus::Rejected, 1);
        assert!(!stale_entry.should_ack);
        assert!(!stale_entry.just_suppressed);
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

    // Sending the disconnect marks the schedule as draining (nothing more
    // may be queued for the dying event loop) and starts the cooldown; the
    // resulting disconnect -- or any other one -- cancels both the schedule
    // and the draining flag, but deliberately keeps the cooldown.
    #[test]
    fn forced_reconnect_schedule_sent_and_cancel() {
        let mut schedule = ForcedReconnectSchedule::default();
        let t0 = tokio::time::Instant::now();

        assert!(!schedule.disconnect_sent());
        schedule.schedule(t0);
        assert_eq!(schedule.due_at(), Some(t0));
        assert!(!schedule.disconnect_sent());

        schedule.sent(t0);
        assert_eq!(schedule.due_at(), None);
        assert!(schedule.disconnect_sent());
        assert_eq!(
            schedule.cooldown_until,
            Some(t0 + FORCED_RECONNECT_COOLDOWN)
        );

        schedule.cancel();
        assert_eq!(schedule.due_at(), None);
        assert!(!schedule.disconnect_sent());
        assert_eq!(
            schedule.cooldown_until,
            Some(t0 + FORCED_RECONNECT_COOLDOWN)
        );

        // A disconnect that happens before a scheduled reconnect fires
        // supersedes it.
        schedule.schedule(t0 + Duration::from_mins(1));
        schedule.cancel();
        assert_eq!(schedule.due_at(), None);
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

    // ------------------------------------------------------------------
    // Exhaustive protocol-state model check.
    //
    // The tests above sample hand-picked transition sequences; review kept
    // finding holes in the sequences nobody picked (a forced reconnect
    // skipped inside the cooldown with nothing left to re-trigger it, SUBACK
    // failures treated as confirmation, ...). This test explores EVERY
    // reachable abstract state of the protocol machinery instead:
    // `ProtocolState` + `ForcedReconnectSchedule` + the resubscribe retry
    // timer, under every legal interleaving of connection, finalization and
    // timer events, checking on every transition the invariants that past
    // findings violated.
    //
    // The event glue in `Model::apply` mirrors `run()`'s select branches;
    // when changing `run()`, change it to match. Timer state is abstracted
    // to armed/not-armed (which timestamps hold never affects WHETHER a
    // transition is legal, only when it happens), so the abstract state
    // space is finite and small; the BFS runs to fixpoint.
    // ------------------------------------------------------------------

    /// Cap for state-space purposes; behavior only distinguishes
    /// `generation == 0` from `> 0`, so higher counts add nothing.
    const MODEL_MAX_GENERATION: u64 = 2;

    /// Enough to have one suppressed publish plus one still-in-flight.
    const MODEL_MAX_INFLIGHT: u8 = 2;

    #[derive(Clone, Copy, Debug)]
    enum ModelEvent {
        /// `poll()` yielded `ConnAck`; `subscribe_queued` is the outcome of the
        /// `try_subscribe` the handler issues when `actions.resubscribe`.
        ConnAck {
            session_present: bool,
            subscribe_queued: bool,
        },
        /// `poll()` yielded a Publish which was processed and registered
        /// with the finalizer.
        ReceivePublish,
        /// The ack stream yielded a finalization for a current-generation
        /// entry (stale-generation entries cannot be yielded: the finalizer
        /// is recreated on every `ConnAck` and disconnect).
        FinalizePublish { status: BatchStatus },
        /// `poll()` yielded a SUBACK for the outstanding (re)subscribe.
        SubAck { all_granted: bool },
        /// `poll()` returned an error (includes failed reconnect attempts,
        /// so it is legal in any state).
        PollError,
        /// The forced-reconnect timer fired and queued `disconnect_now`
        /// (an unbounded channel, so queueing cannot fail).
        ForcedReconnectDue,
        /// The event loop finished our `DISCONNECT` and returned
        /// `RequestsDone`; `run` scrubs the dead connection and rebuilds
        /// the client and event loop.
        DisconnectCompleted,
        /// The paced resubscribe retry timer fired.
        ResubscribeRetryDue { subscribe_queued: bool },
        /// The top-of-loop resubscribe retry ran (it runs before every
        /// select; modeling it as an optional event covers both orders).
        LoopTopResubscribe { subscribe_queued: bool },
    }

    const MODEL_EVENTS: [ModelEvent; 14] = [
        ModelEvent::ConnAck {
            session_present: false,
            subscribe_queued: false,
        },
        ModelEvent::ConnAck {
            session_present: false,
            subscribe_queued: true,
        },
        ModelEvent::ConnAck {
            session_present: true,
            subscribe_queued: false,
        },
        ModelEvent::ConnAck {
            session_present: true,
            subscribe_queued: true,
        },
        ModelEvent::ReceivePublish,
        ModelEvent::FinalizePublish {
            status: BatchStatus::Delivered,
        },
        ModelEvent::FinalizePublish {
            status: BatchStatus::Rejected,
        },
        ModelEvent::SubAck { all_granted: true },
        ModelEvent::SubAck { all_granted: false },
        ModelEvent::PollError,
        ModelEvent::ForcedReconnectDue,
        ModelEvent::DisconnectCompleted,
        ModelEvent::ResubscribeRetryDue {
            subscribe_queued: true,
        },
        ModelEvent::LoopTopResubscribe {
            subscribe_queued: true,
        },
    ];

    #[derive(Clone)]
    struct Model {
        state: ProtocolState,
        forced: ForcedReconnectSchedule,
        resubscribe_retry_scheduled: bool,
        /// Unfinalized publishes registered with the current finalizer.
        inflight: u8,
        now: tokio::time::Instant,
    }

    /// The abstraction the BFS dedups on: timer instants collapse to
    /// armed/not-armed, the generation saturates.
    type ModelKey = (bool, bool, bool, bool, u64, bool, bool, bool, bool, u8);

    impl Model {
        fn initial() -> Self {
            Self {
                state: ProtocolState {
                    pending_resubscribe: true,
                    ..ProtocolState::default()
                },
                forced: ForcedReconnectSchedule::default(),
                resubscribe_retry_scheduled: false,
                inflight: 0,
                now: tokio::time::Instant::now(),
            }
        }

        fn key(&self) -> ModelKey {
            (
                self.state.connected,
                self.state.pending_resubscribe,
                self.state.resubscribe_awaiting_suback,
                self.state.ack_suppressed,
                self.state.connection_generation.min(MODEL_MAX_GENERATION),
                self.forced.due_at().is_some(),
                self.forced.cooldown_until.is_some(),
                self.forced.disconnect_sent(),
                self.resubscribe_retry_scheduled,
                self.inflight,
            )
        }

        fn legal(&self, event: ModelEvent) -> bool {
            match event {
                // ConnAck is strictly the first event of a connection.
                ModelEvent::ConnAck { .. } => !self.state.connected,
                ModelEvent::ReceivePublish => {
                    self.state.connected && self.inflight < MODEL_MAX_INFLIGHT
                }
                ModelEvent::FinalizePublish { .. } => self.inflight > 0,
                // A SUBACK only arrives for an outstanding SUBSCRIBE.
                ModelEvent::SubAck { .. } => {
                    self.state.connected && self.state.resubscribe_awaiting_suback
                }
                ModelEvent::PollError => true,
                ModelEvent::ForcedReconnectDue => {
                    self.forced.due_at().is_some() && self.state.connected
                }
                ModelEvent::DisconnectCompleted => self.forced.disconnect_sent(),
                ModelEvent::ResubscribeRetryDue { .. } => self.resubscribe_retry_scheduled,
                ModelEvent::LoopTopResubscribe { .. } => {
                    self.state.loop_actions().retry_resubscribe && !self.forced.disconnect_sent()
                }
            }
        }

        /// Mirrors the corresponding handler glue in `run()`.
        fn apply(&mut self, event: ModelEvent, acknowledgements: bool) {
            self.now += Duration::from_secs(1);
            match event {
                ModelEvent::ConnAck {
                    session_present,
                    subscribe_queued,
                } => {
                    let actions = self.state.on_connack(acknowledgements, session_present);
                    // recreate_finalizer drops all registered entries.
                    assert!(actions.recreate_finalizer);
                    self.inflight = 0;
                    if actions.resubscribe {
                        self.state.on_resubscribe_result(subscribe_queued);
                    }
                    self.resubscribe_retry_scheduled = false;
                }
                ModelEvent::ReceivePublish => self.inflight += 1,
                ModelEvent::FinalizePublish { status } => {
                    self.inflight -= 1;
                    let decision = self
                        .state
                        .finalize_publish(status, self.state.connection_generation);
                    if decision.should_ack {
                        // Emitting an ack while a `Disconnect` is unanswered
                        // would violate [MQTT-3.14.4-1]; `disconnect_sent`
                        // implies the generation is suppressed, which implies
                        // `should_ack` is false.
                        assert!(
                            !self.forced.disconnect_sent(),
                            "ack emitted while draining after a Disconnect"
                        );
                    }
                    if decision.just_suppressed {
                        self.forced.schedule(self.now);
                    }
                }
                ModelEvent::SubAck { all_granted } => {
                    if all_granted {
                        self.state.on_suback();
                        self.resubscribe_retry_scheduled = false;
                    } else {
                        self.resubscribe_retry_scheduled = true;
                    }
                }
                // A natural connection error and the end of our own
                // `DISCONNECT` (followed by a fresh event loop) both run
                // `handle_connection_lost`.
                ModelEvent::PollError | ModelEvent::DisconnectCompleted => self.connection_lost(),
                ModelEvent::ForcedReconnectDue => {
                    // Firing the timer while a previous `DISCONNECT` is
                    // still pending would send a second one.
                    assert!(
                        !self.forced.disconnect_sent(),
                        "forced-reconnect timer fired while a Disconnect is already pending"
                    );
                    self.forced.sent(self.now);
                }
                ModelEvent::ResubscribeRetryDue { subscribe_queued } => {
                    self.resubscribe_retry_scheduled = false;
                    if self.state.connected
                        && self.state.pending_resubscribe
                        && !self.forced.disconnect_sent()
                    {
                        self.state.on_resubscribe_result(subscribe_queued);
                    }
                }
                ModelEvent::LoopTopResubscribe { subscribe_queued } => {
                    self.state.on_resubscribe_result(subscribe_queued);
                }
            }
        }

        /// Mirrors `handle_connection_lost`.
        fn connection_lost(&mut self) {
            let actions = self.state.on_disconnect();
            assert!(actions.recreate_finalizer);
            self.inflight = 0;
            self.forced.cancel();
            self.resubscribe_retry_scheduled = false;
        }

        fn check_invariants(&self, trace: &[ModelEvent]) {
            let describe = || {
                format!(
                    "state: {:?}, forced: {:?}, retry_scheduled: {}, inflight: {}\ntrace: {trace:?}",
                    self.state, self.forced, self.resubscribe_retry_scheduled, self.inflight,
                )
            };

            // The invariant behind the cooldown-skip finding: a suppressed,
            // still-connected generation must always have a recovery action
            // armed (the reconnect timer, or a queued `DISCONNECT`, which the
            // event loop is guaranteed to complete with `RequestsDone`), or
            // the source is stuck forever with nothing left to trigger
            // recovery.
            if self.state.connected && self.state.ack_suppressed {
                assert!(
                    self.forced.due_at().is_some() || self.forced.disconnect_sent(),
                    "suppressed generation with no recovery action armed\n{}",
                    describe()
                );
            }

            // Per-connection schedules must die with the connection: a timer
            // surviving a disconnect would fire against the next connection.
            if !self.state.connected {
                assert!(
                    self.forced.due_at().is_none()
                        && !self.forced.disconnect_sent()
                        && !self.resubscribe_retry_scheduled,
                    "per-connection schedule survived a disconnect\n{}",
                    describe()
                );
            }

            // The invariant behind the SUBACK findings: while connected with
            // an unconfirmed subscription, some path toward retrying it must
            // remain live (the loop-top retry, an outstanding SUBACK that
            // will either confirm or reject-and-pace, or the paced retry
            // timer). While draining, the pending flag itself survives to
            // the next connection, whose ConnAck re-issues the subscribe.
            if self.state.connected
                && self.state.pending_resubscribe
                && !self.forced.disconnect_sent()
            {
                assert!(
                    self.state.loop_actions().retry_resubscribe
                        || self.state.resubscribe_awaiting_suback
                        || self.resubscribe_retry_scheduled,
                    "unconfirmed subscription with no retry path armed\n{}",
                    describe()
                );
            }
        }
    }

    #[test]
    fn exhaustive_protocol_state_model_check() {
        for acknowledgements in [true, false] {
            let mut visited = std::collections::HashSet::<ModelKey>::new();
            // Keep a short trace per state for debuggable failures.
            let mut queue = std::collections::VecDeque::<(Model, Vec<ModelEvent>)>::new();

            let initial = Model::initial();
            initial.check_invariants(&[]);
            visited.insert(initial.key());
            queue.push_back((initial, Vec::new()));

            let mut transitions = 0usize;
            let mut reached_suppressed = false;
            let mut reached_draining = false;
            let mut reached_retry_pacing = false;

            while let Some((model, trace)) = queue.pop_front() {
                for event in MODEL_EVENTS {
                    if !model.legal(event) {
                        continue;
                    }
                    let mut next = model.clone();
                    next.apply(event, acknowledgements);
                    transitions += 1;

                    let mut next_trace = trace.clone();
                    next_trace.push(event);
                    next.check_invariants(&next_trace);

                    // Transition-level invariant: `pending_resubscribe` may
                    // only be cleared by a fully granted SUBACK.
                    if model.state.pending_resubscribe && !next.state.pending_resubscribe {
                        assert!(
                            matches!(event, ModelEvent::SubAck { all_granted: true }),
                            "pending_resubscribe cleared by {event:?}, not a granted SUBACK\ntrace: {next_trace:?}"
                        );
                    }

                    reached_suppressed |= next.state.ack_suppressed;
                    reached_draining |= next.forced.disconnect_sent();
                    reached_retry_pacing |= next.resubscribe_retry_scheduled;

                    if visited.insert(next.key()) {
                        queue.push_back((next, next_trace));
                    }
                }
            }

            // Guard against the model check passing vacuously.
            assert!(visited.len() > 50, "suspiciously small state space");
            assert!(transitions > 200, "suspiciously few transitions");
            assert!(reached_suppressed, "never reached a suppressed state");
            assert!(reached_draining, "never reached a draining state");
            assert!(
                reached_retry_pacing,
                "never reached a paced-resubscribe-retry state"
            );
        }
    }
}
