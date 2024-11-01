use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
};

use futures::{future::try_join_all, FutureExt};
use tokio::sync::{
    mpsc as tokio_mpsc,
    mpsc::error::{SendError, TrySendError},
    oneshot,
};
use tracing::{Instrument, Span};
use uuid::Uuid;
use vector_buffers::{topology::builder::TopologyBuilder, WhenFull};
use vector_common::config::ComponentKey;
use vector_core::event::{EventArray, LogArray, MetricArray, TraceArray};
use vector_core::fanout;

use crate::notification::{InvalidMatch, Matched, NotMatched, Notification};
use crate::topology::{TapOutput, TapResource, WatchRx};

/// A tap sender is the control channel used to surface tap payloads to a client.
type TapSender = tokio_mpsc::Sender<TapPayload>;

// Shutdown channel types
type ShutdownTx = oneshot::Sender<()>;
type ShutdownRx = oneshot::Receiver<()>;

const TAP_BUFFER_SIZE: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(100) };

/// Clients can supply glob patterns to find matched topology components.
trait GlobMatcher<T> {
    fn matches_glob(&self, rhs: T) -> bool;
}

impl GlobMatcher<&str> for String {
    fn matches_glob(&self, rhs: &str) -> bool {
        match glob::Pattern::new(self) {
            Ok(pattern) => pattern.matches(rhs),
            _ => false,
        }
    }
}

/// Distinguishing between pattern variants helps us preserve user-friendly tap
/// notifications. Otherwise, after translating an input pattern into relevant
/// output patterns, we'd be unable to send a [`TapPayload::Notification`] with
/// the original user-specified input pattern.
#[derive(Debug, Eq, PartialEq, Hash)]
enum Pattern {
    /// A pattern used to tap into outputs of components
    OutputPattern(glob::Pattern),
    /// A pattern used to tap into inputs of components.
    ///
    /// For a tap user, an input pattern is effectively a shortcut for specifying
    /// one or more output patterns since a component's inputs are other
    /// components' outputs. This variant captures the original user-supplied
    /// pattern alongside the output patterns it's translated into.
    InputPattern(String, Vec<glob::Pattern>),
}

impl GlobMatcher<&str> for Pattern {
    fn matches_glob(&self, rhs: &str) -> bool {
        match self {
            Pattern::OutputPattern(pattern) => pattern.matches(rhs),
            Pattern::InputPattern(_, patterns) => {
                patterns.iter().any(|pattern| pattern.matches(rhs))
            }
        }
    }
}

/// Patterns (glob) used by tap to match against components and access events
/// flowing into (for_inputs) or out of (for_outputs) specified components
#[derive(Debug)]
pub struct TapPatterns {
    pub for_outputs: HashSet<String>,
    pub for_inputs: HashSet<String>,
}

impl TapPatterns {
    pub const fn new(for_outputs: HashSet<String>, for_inputs: HashSet<String>) -> Self {
        Self {
            for_outputs,
            for_inputs,
        }
    }

    /// Get all user-specified patterns
    pub fn all_patterns(&self) -> HashSet<String> {
        self.for_outputs
            .iter()
            .cloned()
            .chain(self.for_inputs.iter().cloned())
            .collect()
    }
}

/// A tap payload contains events or notifications that alert users about the
/// status of the tap request.
#[derive(Debug)]
pub enum TapPayload {
    Log(TapOutput, LogArray),
    Metric(TapOutput, MetricArray),
    Trace(TapOutput, TraceArray),
    Notification(Notification),
}

impl TapPayload {
    /// Raise a `matched` event against the provided pattern.
    pub fn matched<T: Into<String>>(pattern: T) -> Self {
        Self::Notification(Notification::Matched(Matched::new(pattern.into())))
    }

    /// Raise a `not_matched` event against the provided pattern.
    pub fn not_matched<T: Into<String>>(pattern: T) -> Self {
        Self::Notification(Notification::NotMatched(NotMatched::new(pattern.into())))
    }

    /// Raise an `invalid_match` event against the provided input pattern.
    pub fn invalid_input_pattern_match<T: Into<String>>(
        pattern: T,
        invalid_matches: Vec<String>,
    ) -> Self {
        let pattern = pattern.into();
        let message = format!("[tap] Warning: source inputs cannot be tapped. Input pattern '{}' matches sources {:?}", pattern, invalid_matches);
        Self::Notification(Notification::InvalidMatch(InvalidMatch::new(
            message,
            pattern,
            invalid_matches,
        )))
    }

    /// Raise an `invalid_match` event against the provided output pattern.
    pub fn invalid_output_pattern_match<T: Into<String>>(
        pattern: T,
        invalid_matches: Vec<String>,
    ) -> Self {
        let pattern = pattern.into();
        let message = format!(
            "[tap] Warning: sink outputs cannot be tapped. Output pattern '{}' matches sinks {:?}",
            pattern, invalid_matches
        );
        Self::Notification(Notification::InvalidMatch(InvalidMatch::new(
            message,
            pattern,
            invalid_matches,
        )))
    }
}

/// A `TapTransformer` transforms raw events and ships them to the global tap receiver.
#[derive(Clone)]
pub struct TapTransformer {
    tap_tx: TapSender,
    output: TapOutput,
}

impl TapTransformer {
    pub const fn new(tap_tx: TapSender, output: TapOutput) -> Self {
        Self { tap_tx, output }
    }

    pub fn try_send(&mut self, events: EventArray) {
        let payload = match events {
            EventArray::Logs(logs) => TapPayload::Log(self.output.clone(), logs),
            EventArray::Metrics(metrics) => TapPayload::Metric(self.output.clone(), metrics),
            EventArray::Traces(traces) => TapPayload::Trace(self.output.clone(), traces),
        };

        if let Err(TrySendError::Closed(payload)) = self.tap_tx.try_send(payload) {
            debug!(
                message = "Couldn't send event.",
                payload = ?payload,
                component_id = ?self.output.output_id,
            );
        }
    }
}

/// A tap sink spawns a process for listening for topology changes. If topology changes,
/// sinks are rewired to accommodate matched/unmatched patterns.
#[derive(Debug)]
pub struct TapController {
    _shutdown: ShutdownTx,
}

impl TapController {
    /// Creates a new tap sink, and spawns a handler for watching for topology changes
    /// and a separate inner handler for events. Uses a oneshot channel to trigger shutdown
    /// of handlers when the `TapSink` drops out of scope.
    pub fn new(watch_rx: WatchRx, tap_tx: TapSender, patterns: TapPatterns) -> Self {
        let (_shutdown, shutdown_rx) = oneshot::channel();

        tokio::spawn(
            tap_handler(patterns, tap_tx, watch_rx, shutdown_rx).instrument(error_span!(
                "tap_handler",
                component_kind = "sink",
                component_id = "_tap", // It isn't clear what the component_id should be here other than "_tap"
                component_type = "tap",
            )),
        );

        Self { _shutdown }
    }
}

/// Provides a `ShutdownTx` that disconnects a component sink when it drops out of scope.
fn shutdown_trigger(control_tx: fanout::ControlChannel, sink_id: ComponentKey) -> ShutdownTx {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    tokio::spawn(async move {
        _ = shutdown_rx.await;
        if control_tx
            .send(fanout::ControlMessage::Remove(sink_id.clone()))
            .is_err()
        {
            debug!(message = "Couldn't disconnect sink.", ?sink_id);
        } else {
            debug!(message = "Disconnected sink.", ?sink_id);
        }
    });

    shutdown_tx
}

/// Sends a 'matched' tap payload.
async fn send_matched(tx: TapSender, pattern: String) -> Result<(), SendError<TapPayload>> {
    debug!(message = "Sending matched notification.", pattern = ?pattern);
    tx.send(TapPayload::matched(pattern)).await
}

/// Sends a 'not matched' tap payload.
async fn send_not_matched(tx: TapSender, pattern: String) -> Result<(), SendError<TapPayload>> {
    debug!(message = "Sending not matched notification.", pattern = ?pattern);
    tx.send(TapPayload::not_matched(pattern)).await
}

/// Sends an 'invalid input pattern match' tap payload.
async fn send_invalid_input_pattern_match(
    tx: TapSender,
    pattern: String,
    invalid_matches: Vec<String>,
) -> Result<(), SendError<TapPayload>> {
    debug!(message = "Sending invalid input pattern match notification.", pattern = ?pattern, invalid_matches = ?invalid_matches);
    tx.send(TapPayload::invalid_input_pattern_match(
        pattern,
        invalid_matches,
    ))
    .await
}

/// Sends an 'invalid output pattern match' tap payload.
async fn send_invalid_output_pattern_match(
    tx: TapSender,
    pattern: String,
    invalid_matches: Vec<String>,
) -> Result<(), SendError<TapPayload>> {
    debug!(message = "Sending invalid output pattern match notification.", pattern = ?pattern, invalid_matches = ?invalid_matches);
    tx.send(TapPayload::invalid_output_pattern_match(
        pattern,
        invalid_matches,
    ))
    .await
}

/// Returns a tap handler that listens for topology changes, and connects sinks to observe
/// `LogEvent`s` when a component matches one or more of the provided patterns.
async fn tap_handler(
    patterns: TapPatterns,
    tx: TapSender,
    mut watch_rx: WatchRx,
    mut shutdown_rx: ShutdownRx,
) {
    debug!(message = "Started tap.", outputs_patterns = ?patterns.for_outputs, inputs_patterns = ?patterns.for_inputs);

    // Sinks register for the current tap. Contains the id of the matched component, and
    // a shutdown trigger for sending a remove control message when matching sinks change.
    let mut sinks: HashMap<ComponentKey, _> = HashMap::new();

    // Recording user-provided patterns for later use in sending notifications
    // (determining patterns which did not match)
    let user_provided_patterns = patterns.all_patterns();

    // The patterns that matched on the last iteration, to compare with the latest
    // round of matches when sending notifications.
    let mut last_matches = HashSet::new();

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => break,
            Ok(_) = watch_rx.changed() => {
                // Cache of matched patterns. A `HashSet` is used here to ignore repetition.
                let mut matched = HashSet::new();

                // Borrow and clone the latest resources to register sinks. Since this blocks the
                // watch channel and the returned ref isn't `Send`, this requires a clone.
                let TapResource {
                    outputs,
                    inputs,
                    source_keys,
                    sink_keys,
                    removals,
                } = watch_rx.borrow().clone();

                // Remove tap sinks from components that have gone away/can no longer match.
                let output_keys = outputs.keys().map(|output| output.output_id.component.clone()).collect::<HashSet<_>>();
                sinks.retain(|key, _| {
                    !removals.contains(key) && output_keys.contains(key) || {
                        debug!(message = "Removing component.", component_id = %key);
                        false
                    }
                });

                let mut component_id_patterns = patterns.for_outputs.iter()
                                                                    .filter_map(|p| glob::Pattern::new(p).ok())
                                                                    .map(Pattern::OutputPattern).collect::<HashSet<_>>();

                // Matching an input pattern is equivalent to matching the outputs of the component's inputs
                for pattern in patterns.for_inputs.iter() {
                    if let Ok(glob) = glob::Pattern::new(pattern) {
                        match inputs.iter().filter(|(key, _)|
                            glob.matches(&key.to_string())
                        ).flat_map(|(_, related_inputs)| related_inputs.iter().map(|id| id.to_string()).collect::<Vec<_>>()).collect::<HashSet<_>>() {
                            found if !found.is_empty() => {
                                component_id_patterns.insert(Pattern::InputPattern(pattern.clone(), found.into_iter()
                                                                                                         .filter_map(|p| glob::Pattern::new(&p).ok()).collect::<Vec<_>>()));
                            }
                            _ => {
                                debug!(message="Input pattern not expanded: no matching components.", ?pattern);
                            }
                        }
                    }
                }

                // Loop over all outputs, and connect sinks for the components that match one
                // or more patterns.
                for (output, control_tx) in outputs.iter() {
                    match component_id_patterns
                        .iter()
                        .filter(|pattern| pattern.matches_glob(&output.output_id.to_string()))
                        .collect::<Vec<_>>()
                    {
                        found if !found.is_empty() => {
                            debug!(
                                message="Component matched.",
                                ?output.output_id, ?component_id_patterns, matched = ?found
                            );

                            // Build a new intermediate buffer pair that we can insert as a sink
                            // target for the component, and spawn our transformer task which will
                            // wrap each event payload with the necessary metadata before forwarding
                            // it to our global tap receiver.
                            let (tap_buffer_tx, mut tap_buffer_rx) = TopologyBuilder::standalone_memory(TAP_BUFFER_SIZE, WhenFull::DropNewest, &Span::current()).await;
                            let mut tap_transformer = TapTransformer::new(tx.clone(), output.clone());

                            tokio::spawn(async move {
                                while let Some(events) = tap_buffer_rx.next().await {
                                    tap_transformer.try_send(events);
                                }
                            });

                            // Attempt to connect the sink.
                            //
                            // This is necessary because a sink may be reconfigured with the same id
                            // as a previous, and we are not getting involved in config diffing at
                            // this point.
                            let sink_id = Uuid::new_v4().to_string();
                            match control_tx
                                .send(fanout::ControlMessage::Add(ComponentKey::from(sink_id.as_str()), tap_buffer_tx))
                            {
                                Ok(_) => {
                                    debug!(
                                        message = "Sink connected.", ?sink_id, ?output.output_id,
                                    );

                                    // Create a sink shutdown trigger to remove the sink
                                    // when matched components change.
                                    sinks.entry(output.output_id.component.clone()).or_insert_with(Vec::new).push(
                                        shutdown_trigger(control_tx.clone(), ComponentKey::from(sink_id.as_str()))
                                    );
                                }
                                Err(error) => {
                                    error!(
                                        message = "Couldn't connect sink.",
                                        ?error,
                                        ?output.output_id,
                                        ?sink_id,
                                    );
                                }
                            }

                            matched.extend(found.iter().map(|pattern| {
                                match pattern {
                                    Pattern::OutputPattern(p) => p.to_string(),
                                    Pattern::InputPattern(p, _) => p.to_owned(),
                                }
                            }));
                        }
                        _ => {
                            debug!(
                                message="Component not matched.", ?output.output_id, ?component_id_patterns
                            );
                        }
                    }
                }

                // Notifications to send to the client.
                let mut notifications = Vec::new();

                // Matched notifications.
                for pattern in matched.difference(&last_matches) {
                    notifications.push(send_matched(tx.clone(), pattern.clone()).boxed());
                }

                // Not matched notifications.
                for pattern in user_provided_patterns.difference(&matched) {
                    notifications.push(send_not_matched(tx.clone(), pattern.clone()).boxed());
                }

                // Warnings on invalid matches.
                for pattern in patterns.for_inputs.iter() {
                    if let Ok(glob) = glob::Pattern::new(pattern) {
                        let invalid_matches = source_keys.iter().filter(|key| glob.matches(key)).cloned().collect::<Vec<_>>();
                        if !invalid_matches.is_empty() {
                            notifications.push(send_invalid_input_pattern_match(tx.clone(), pattern.clone(), invalid_matches).boxed())
                        }
                    }
                }
                for pattern in patterns.for_outputs.iter() {
                    if let Ok(glob) = glob::Pattern::new(pattern) {
                        let invalid_matches = sink_keys.iter().filter(|key| glob.matches(key)).cloned().collect::<Vec<_>>();
                        if !invalid_matches.is_empty() {
                            notifications.push(send_invalid_output_pattern_match(tx.clone(), pattern.clone(), invalid_matches).boxed())
                        }
                    }
                }

                last_matches = matched;

                // Send all events. If any event returns an error, this means the client
                // channel has gone away, so we can break the loop.
                if try_join_all(notifications).await.is_err() {
                    debug!("Couldn't send notification(s); tap gone away.");
                    break;
                }
            }
        }
    }

    debug!(message = "Stopped tap.", outputs_patterns = ?patterns.for_outputs, inputs_patterns = ?patterns.for_inputs);
}

mod tests {
    #[test]
    /// Patterns should accept globbing.
    fn matches() {
        use super::GlobMatcher;

        let patterns = ["ab*", "12?", "xy?"];

        // Should find.
        for id in &["abc", "123", "xyz"] {
            assert!(patterns.iter().any(|p| p.to_string().matches_glob(id)));
        }

        // Should not find.
        for id in &["xzy", "ad*", "1234"] {
            assert!(!patterns.iter().any(|p| p.to_string().matches_glob(id)));
        }
    }
}
