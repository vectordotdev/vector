use std::{fmt, mem, task::Poll, time::Instant};

use futures::{Stream, StreamExt};
use futures_util::{pending, poll};
use indexmap::IndexMap;
use tokio::sync::mpsc;
use tokio_util::sync::ReusableBoxFuture;
use vector_buffers::topology::channel::BufferSender;

use crate::{
    config::ComponentKey,
    event::{EventArray, EventContainer},
};

pub enum ControlMessage {
    /// Adds a new sink to the fanout.
    Add(ComponentKey, BufferSender<EventArray>),

    /// Removes a sink from the fanout.
    Remove(ComponentKey),

    /// Pauses a sink in the fanout.
    ///
    /// If a fanout has any paused sinks, subsequent sends cannot proceed until all paused sinks
    /// have been replaced.
    Pause(ComponentKey),

    /// Replaces a paused sink with its new sender.
    Replace(ComponentKey, BufferSender<EventArray>),
}

impl fmt::Debug for ControlMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ControlMessage::")?;
        match self {
            Self::Add(id, _) => write!(f, "Add({id:?})"),
            Self::Remove(id) => write!(f, "Remove({id:?})"),
            Self::Pause(id) => write!(f, "Pause({id:?})"),
            Self::Replace(id, _) => write!(f, "Replace({id:?})"),
        }
    }
}

// TODO: We should really wrap this in a custom type that has dedicated methods for each operation
// so that high-lever components don't need to do the raw channel sends, etc.
pub type ControlChannel = mpsc::UnboundedSender<ControlMessage>;

pub struct Fanout {
    senders: IndexMap<ComponentKey, Output>,
    control_channel: mpsc::UnboundedReceiver<ControlMessage>,
    upstream_component: ComponentKey,
}

impl Fanout {
    pub fn new(upstream_component: ComponentKey) -> (Self, ControlChannel) {
        let (control_tx, control_rx) = mpsc::unbounded_channel();

        let fanout = Self {
            senders: IndexMap::default(),
            control_channel: control_rx,
            upstream_component,
        };

        (fanout, control_tx)
    }

    /// Add a new sink as an output.
    ///
    /// # Panics
    ///
    /// Function will panic if a sink with the same ID is already present.
    pub fn add(&mut self, id: ComponentKey, sink: BufferSender<EventArray>) {
        assert!(
            !self.senders.contains_key(&id),
            "Adding duplicate output id to fanout: {id}"
        );

        // A sink added during an in-flight send is idle until the next batch. This send does not
        // have the batch to give it.
        self.senders.insert(
            id,
            Output {
                phase: Phase::Idle,
                sender: Some(Sender::new(sink)),
                send: vacant_send(),
            },
        );
    }

    fn remove(&mut self, id: &ComponentKey) {
        // Detach before dropping the entry so an in-flight send keeps its `Sender` and can finish.
        let Some(mut output) = self.senders.shift_remove(id) else {
            panic!("Removing nonexistent sink from fanout: {id}");
        };
        if matches!(output.phase, Phase::InFlight) {
            spawn_inflight(&mut output);
        }
    }

    fn replace(&mut self, id: &ComponentKey, sink: BufferSender<EventArray>) {
        let Some(output) = self.senders.get_mut(id) else {
            panic!("Replacing unknown sink from fanout: {id}");
        };
        // Only a paused output can be replaced. An in-flight send still owns the previous sender,
        // and finishing that send would write it back over a sender installed here.
        assert!(
            matches!(output.phase, Phase::Paused) && output.sender.is_none(),
            "Replacing existing sink is not valid: {id}"
        );
        output.phase = Phase::Idle;
        output.sender = Some(Sender::new(sink));
    }

    fn pause(&mut self, id: &ComponentKey) {
        let Some(output) = self.senders.get_mut(id) else {
            panic!("Pausing unknown sink from fanout: {id}");
        };
        match output.phase {
            Phase::Paused => panic!("Pausing already-paused sink is invalid: {id}"),
            Phase::Idle => {
                assert!(
                    output.sender.take().is_some(),
                    "Pausing nonexistent sink is not valid: {id}"
                );
                output.phase = Phase::Paused;
            }
            Phase::InFlight => {
                output.phase = Phase::Paused;
                spawn_inflight(output);
            }
        }
    }

    /// Waits for the next control message and applies it.
    ///
    /// Returns `true` if a message was processed, `false` if the control
    /// channel was closed.
    pub async fn recv_control_message(&mut self) -> bool {
        match self.control_channel.recv().await {
            Some(msg) => {
                self.apply_control_message(msg);
                true
            }
            None => false,
        }
    }

    /// Apply a control message directly against this instance.
    fn apply_control_message(&mut self, message: ControlMessage) {
        trace!(?message, "Processing control message");

        match message {
            ControlMessage::Add(id, sink) => self.add(id, sink),
            ControlMessage::Remove(id) => self.remove(&id),
            ControlMessage::Pause(id) => self.pause(&id),
            ControlMessage::Replace(id, sink) => self.replace(&id, sink),
        }
    }

    /// Waits for all paused sinks to be replaced.
    ///
    /// Control messages are processed until all senders have been replaced, so it is guaranteed
    /// that when this method returns, all senders are ready for the next send to be triggered.
    async fn wait_for_replacements(&mut self) {
        while self
            .senders
            .values()
            .any(|output| matches!(output.phase, Phase::Paused))
        {
            if let Some(msg) = self.control_channel.recv().await {
                self.apply_control_message(msg);
            } else {
                // If the control channel is closed, there's nothing else we can do.

                // TODO: It _seems_ like we should probably panic here, or at least return.
                //
                // Essentially, we should only land here if the control channel is closed but we
                // haven't yet replaced all of the paused sinks... and we shouldn't have any paused
                // sinks if Vector is stopping normally/gracefully, so like... we'd only get
                // here during a configuration reload where we panicked in another thread due to
                // an error of some sort, and the control channel got dropped, closed itself, and
                // we're never going to be able to recover.
                //
                // The flipside is that by leaving it as-is, in the above hypothesized scenario,
                // we'd avoid emitting additional panics/error logging when the root cause error was
                // already doing so, like there's little value in knowing the fanout also hit an
                // unrecoverable state if the whole process is about to come crashing down
                // anyways... but it still does feel weird to have that encoded here by virtue of
                // only a comment, and not an actual terminating expression. *shrug*
            }
        }
    }

    /// Send a stream of events to all connected sinks.
    ///
    /// This function will send events until the provided stream finishes. It will also block on the
    /// resolution of any pending reload before proceeding with a send operation, similar to `send`.
    ///
    /// # Panics
    ///
    /// This method can panic if the fanout receives a control message that violates some invariant
    /// about its current state (e.g. remove a nonexistent sink, etc.). This would imply a bug in
    /// Vector's config reloading logic.
    ///
    /// # Errors
    ///
    /// If an error occurs while sending events to any of the connected sinks, an error variant will be
    /// returned detailing the cause.
    pub async fn send_stream(
        &mut self,
        events: impl Stream<Item = (EventArray, Instant)>,
    ) -> crate::Result<()> {
        tokio::pin!(events);
        while let Some((event_array, send_reference)) = events.next().await {
            self.send(event_array, Some(send_reference)).await?;
        }
        Ok(())
    }

    /// Send a batch of events to all connected sinks.
    ///
    /// This will block on the resolution of any pending reload before proceeding with the send
    /// operation.
    ///
    /// # Panics
    ///
    /// This method can panic if the fanout receives a control message that violates some invariant
    /// about its current state (e.g. remove a nonexistent sink, etc). This would imply a bug in
    /// Vector's config reloading logic.
    ///
    /// # Errors
    ///
    /// If an error occurs while sending events to any of the connected sinks, an error variant will be
    /// returned detailing the cause.
    ///
    /// # Cancellation
    ///
    /// If this future is dropped while a downstream buffer is still pending, the batch stays in the
    /// fanout and is delivered by the next call to `send`. `recv_control_message` does not drive
    /// it. Removing or pausing an output while that batch is still in flight detaches the send onto
    /// a background task so the batch is not dropped.
    pub async fn send(
        &mut self,
        events: EventArray,
        send_reference: Option<Instant>,
    ) -> crate::Result<()> {
        // First, process any available control messages in a non-blocking fashion.
        while let Ok(message) = self.control_channel.try_recv() {
            self.apply_control_message(message);
        }

        // Drop empty event batches before they reach any downstream buffer, this is technically
        // programmer error. In debug/test builds the `debug_assert!` makes the underlying bug fail
        // loudly.
        debug_assert!(
            !events.is_empty(),
            "Fanout received empty event batch from upstream component '{}'",
            self.upstream_component,
        );
        // TODO: Wrap the conditional below with `std::hint::unlikely` once it stabilizes. This is an
        // applicable situation to use it in since the following conditional should never evaluate to
        // true.
        #[cfg(not(debug_assertions))]
        if events.is_empty() {
            warn!(
                message = "Dropping empty event batch emitted by upstream component. This is likely a bug in that component.",
                component_id = %self.upstream_component,
                downstream_count = self.senders.len(),
            );
            return Ok(());
        }

        // Resume a send that was dropped while a downstream buffer was still pending, before
        // waiting on a reload. The future stays in the output entry, so this resumes it instead of
        // holding the batch until an unrelated pause is replaced. Skip this when nothing is in
        // flight: `try_recv` already drained the control channel, and `drive` would only select
        // on it again.
        if self
            .senders
            .values()
            .any(|output| matches!(output.phase, Phase::InFlight))
        {
            self.drive().await?;
        }
        self.wait_for_replacements().await;

        // Nothing to send if we have no sender. A dropped send cannot have left an in-flight
        // future behind either: removing the last output detaches that future.
        if self.senders.is_empty() {
            trace!("No senders present.");
            return Ok(());
        }

        self.arm_sends(events, send_reference);
        self.drive().await?;
        trace!("Sent item to fanout.");
        Ok(())
    }

    fn arm_sends(&mut self, events: EventArray, send_reference: Option<Instant>) {
        debug_assert!(
            self.senders
                .values()
                .all(|output| matches!(output.phase, Phase::Idle) && output.sender.is_some())
        );

        let last_sender_idx = self.senders.len().saturating_sub(1);
        let mut events = Some(events);
        for (i, output) in self.senders.values_mut().enumerate() {
            // `send` finishes in-flight work and waits until nothing is paused before calling this.
            // Idle is the only phase left, and that phase holds the sender.
            let Some(mut sender) = output.sender.take() else {
                unreachable!("sender must be idle to arm a fanout send");
            };
            output.phase = Phase::InFlight;
            if i == last_sender_idx {
                sender.input = events.take();
            } else {
                sender.input.clone_from(&events);
            }
            sender.send_reference = send_reference;
            output.send.set(async move {
                sender.flush().await?;
                Ok(sender)
            });
        }
    }

    /// Drive in-flight sends to completion, applying control messages first.
    async fn drive(&mut self) -> crate::Result<()> {
        // Keep track of whether the control channel has returned `Ready(None)`, and stop polling
        // it once it has. If we don't do this check, it will continue to return `Ready(None)` any
        // time it is polled, which can lead to a busy loop below.
        //
        // In real life this is likely a non-issue, but it can lead to strange behavior in tests if
        // left unhandled.
        let mut control_channel_open = true;

        loop {
            tokio::select! {
                // Semantically, it's not hugely important that this select is biased. It does,
                // however, make testing simpler when you can count on control messages being
                // processed first.
                biased;

                maybe_msg = self.control_channel.recv(), if control_channel_open => {
                    match maybe_msg {
                        Some(message) => self.apply_control_message(message),
                        // Control channel is closed, which means Vector is shutting down.
                        None => control_channel_open = false,
                    }
                }

                // `poll_inflight` returns as soon as nothing is in flight, which ends the drive.
                result = poll_inflight(&mut self.senders) => return result,
            }
        }
    }
}

/// Per-output state stored directly in the fanout map.
///
/// The boxed send future lives for the life of the output and is reused across batches. In-flight
/// sends are not tracked by index: `IndexMap::shift_remove` renumbers later entries.
struct Output {
    phase: Phase,
    /// Present only while `phase` is [`Phase::Idle`]. Stored beside the phase so `Phase` is not a
    /// large enum: `Sender` is hundreds of bytes, and `clippy::large_enum_variant` rejects
    /// `Phase::Idle(Sender)`. Boxing the sender would allocate on every completed send.
    sender: Option<Sender>,
    send: ReusableBoxFuture<'static, crate::Result<Sender>>,
}

enum Phase {
    /// Waiting for the next batch. `sender` owns the [`Sender`].
    Idle,
    /// `send` owns the `Sender` and the batch it is flushing.
    InFlight,
    /// Waiting for [`ControlMessage::Replace`]. Any in-flight send was detached.
    Paused,
}

fn vacant_send() -> ReusableBoxFuture<'static, crate::Result<Sender>> {
    // Polled only while an output is in flight. Arming installs the real future before that phase
    // is observable, and detach installs this placeholder only after the output is paused or removed.
    ReusableBoxFuture::new(async { unreachable!("polled a fanout send that was not armed") })
}

/// Hand an in-flight send to the runtime so pause or removal does not drop its batch.
///
/// The future owns the `Sender`, so it stays `'static` after the output entry lets go of it. The
/// replacement box is allocated only on this path.
fn spawn_inflight(output: &mut Output) {
    // Deliberately not instrumented with the current span: this drains a send to a sink that has
    // just been detached from the topology, so it is unrelated to the upstream component that owns
    // this fanout. Attaching the current span would mis-tag this task's logs with the upstream
    // component's identity rather than the detached sink's.
    let send = mem::replace(&mut output.send, vacant_send());
    tokio::spawn(async move {
        if let Err(cause) = send.await {
            warn!(
                %cause,
                message = "Encountered error writing to component after detaching from topology.",
            );
        }
    });
}

// Free function so `drive` can borrow `senders` while `select!` also borrows `control_channel`.
async fn poll_inflight(senders: &mut IndexMap<ComponentKey, Output>) -> crate::Result<()> {
    // Poll each in-flight send once, then yield, so control messages are observed while a buffer
    // is pending. `FuturesUnordered` would wake only the futures that become ready, but each
    // future has to stay in its map entry: pause, remove, and replace look that entry up and may
    // detach the send. Moving the futures into another collection would split them from that state.
    loop {
        let mut waiting = false;
        for output in senders.values_mut() {
            if !matches!(output.phase, Phase::InFlight) {
                continue;
            }
            match poll!(output.send.get_pin()) {
                Poll::Ready(Ok(sender)) => {
                    output.sender = Some(sender);
                    output.phase = Phase::Idle;
                }
                // The future dropped the `Sender` on its way out. Leave the output paused so the
                // next send cannot arm a sender that no longer exists.
                Poll::Ready(Err(error)) => {
                    output.phase = Phase::Paused;
                    return Err(error);
                }
                Poll::Pending => waiting = true,
            }
        }
        if !waiting {
            return Ok(());
        }
        pending!();
    }
}

struct Sender {
    inner: BufferSender<EventArray>,
    input: Option<EventArray>,
    send_reference: Option<Instant>,
}

impl Sender {
    fn new(inner: BufferSender<EventArray>) -> Self {
        Self {
            inner,
            input: None,
            send_reference: None,
        }
    }

    async fn flush(&mut self) -> crate::Result<()> {
        let send_reference = self.send_reference.take();
        if let Some(input) = self.input.take() {
            self.inner.send(input, send_reference).await?;
            self.inner.flush().await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{mem, num::NonZeroUsize};

    use futures_util::poll;
    use tokio::sync::mpsc::UnboundedSender;
    use tokio_test::{assert_pending, assert_ready, task::spawn};
    use tracing::Span;
    use vector_buffers::{
        WhenFull,
        topology::{
            builder::TopologyBuilder,
            channel::{BufferReceiver, BufferSender},
        },
    };
    use vrl::{event_path, value::Value};

    use super::{ControlMessage, Fanout};
    use crate::{
        config::ComponentKey,
        event::{Event, EventArray, EventContainer, LogEvent},
        test_util::{collect_ready, collect_ready_events},
    };

    fn build_sender_pair(
        capacity: usize,
    ) -> (BufferSender<EventArray>, BufferReceiver<EventArray>) {
        TopologyBuilder::standalone_memory(
            NonZeroUsize::new(capacity).expect("capacity must be nonzero"),
            WhenFull::Block,
            &Span::current(),
            None,
            None,
        )
    }

    fn build_sender_pairs(
        capacities: &[usize],
    ) -> Vec<(BufferSender<EventArray>, BufferReceiver<EventArray>)> {
        let mut pairs = Vec::new();
        for capacity in capacities {
            pairs.push(build_sender_pair(*capacity));
        }
        pairs
    }

    fn fanout_from_senders(
        capacities: &[usize],
    ) -> (
        Fanout,
        UnboundedSender<ControlMessage>,
        Vec<BufferReceiver<EventArray>>,
    ) {
        let (mut fanout, control) = Fanout::new(ComponentKey::from("test_upstream"));
        let pairs = build_sender_pairs(capacities);

        let mut receivers = Vec::new();
        for (i, (sender, receiver)) in pairs.into_iter().enumerate() {
            fanout.add(ComponentKey::from(i.to_string()), sender);
            receivers.push(receiver);
        }

        (fanout, control, receivers)
    }

    fn add_sender_to_fanout(
        fanout: &mut Fanout,
        receivers: &mut Vec<BufferReceiver<EventArray>>,
        sender_id: usize,
        capacity: usize,
    ) {
        let (sender, receiver) = build_sender_pair(capacity);
        receivers.push(receiver);

        fanout.add(ComponentKey::from(sender_id.to_string()), sender);
    }

    fn remove_sender_from_fanout(control: &UnboundedSender<ControlMessage>, sender_id: usize) {
        control
            .send(ControlMessage::Remove(ComponentKey::from(
                sender_id.to_string(),
            )))
            .expect("sending control message should not fail");
    }

    fn replace_sender_in_fanout(
        control: &UnboundedSender<ControlMessage>,
        receivers: &mut [BufferReceiver<EventArray>],
        sender_id: usize,
        capacity: usize,
    ) -> BufferReceiver<EventArray> {
        let (sender, receiver) = build_sender_pair(capacity);
        let old_receiver = mem::replace(&mut receivers[sender_id], receiver);

        control
            .send(ControlMessage::Pause(ComponentKey::from(
                sender_id.to_string(),
            )))
            .expect("sending control message should not fail");

        control
            .send(ControlMessage::Replace(
                ComponentKey::from(sender_id.to_string()),
                sender,
            ))
            .expect("sending control message should not fail");

        old_receiver
    }

    fn start_sender_replace(
        control: &UnboundedSender<ControlMessage>,
        receivers: &mut [BufferReceiver<EventArray>],
        sender_id: usize,
        capacity: usize,
    ) -> (BufferReceiver<EventArray>, BufferSender<EventArray>) {
        let (sender, receiver) = build_sender_pair(capacity);
        let old_receiver = mem::replace(&mut receivers[sender_id], receiver);

        control
            .send(ControlMessage::Pause(ComponentKey::from(
                sender_id.to_string(),
            )))
            .expect("sending control message should not fail");

        (old_receiver, sender)
    }

    fn finish_sender_resume(
        control: &UnboundedSender<ControlMessage>,
        sender_id: usize,
        sender: BufferSender<EventArray>,
    ) {
        control
            .send(ControlMessage::Replace(
                ComponentKey::from(sender_id.to_string()),
                sender,
            ))
            .expect("sending control message should not fail");
    }

    fn unwrap_log_event_message<E>(event: E) -> String
    where
        E: EventContainer,
    {
        let event = event
            .into_events()
            .next()
            .expect("must have at least one event");
        let event = event.into_log();
        event
            .get(event_path!("message"))
            .and_then(Value::as_bytes)
            .and_then(|b| String::from_utf8(b.to_vec()).ok())
            .expect("must be valid log event with `message` field")
    }

    #[tokio::test]
    async fn fanout_writes_to_all() {
        let (mut fanout, _, receivers) = fanout_from_senders(&[2, 2]);
        let events = make_event_array(2);

        let clones = events.clone();
        fanout.send(clones, None).await.expect("should not fail");

        for receiver in receivers {
            assert_eq!(
                collect_ready(receiver.into_stream()),
                std::slice::from_ref(&events)
            );
        }
    }

    #[tokio::test]
    async fn fanout_notready() {
        let (mut fanout, _, mut receivers) = fanout_from_senders(&[2, 1, 2]);
        let events = make_events(2);

        // First send should immediately complete because all senders have capacity:
        let mut first_send = spawn(fanout.send(events[0].clone().into(), None));
        assert_ready!(first_send.poll()).expect("should not fail");
        drop(first_send);

        // Second send should return pending because sender B is now full:
        let mut second_send = spawn(fanout.send(events[1].clone().into(), None));
        assert_pending!(second_send.poll());

        // Now read an item from each receiver to free up capacity for the second sender:
        for receiver in &mut receivers {
            assert_eq!(Some(events[0].clone().into()), receiver.next().await);
        }

        // Now our second send should actually be able to complete:
        assert_ready!(second_send.poll()).expect("should not fail");
        drop(second_send);

        // And make sure the second item comes through:
        for receiver in &mut receivers {
            assert_eq!(Some(events[1].clone().into()), receiver.next().await);
        }
    }

    /// Two capacity-1 outputs. Event 0 is buffered, and event 1 is left in flight by a dropped send.
    async fn drop_blocked_fanout_send() -> (
        Fanout,
        UnboundedSender<ControlMessage>,
        Vec<BufferReceiver<EventArray>>,
        Vec<Event>,
    ) {
        let (mut fanout, control, receivers) = fanout_from_senders(&[1, 1]);
        let events = make_events(3);

        fanout
            .send(events[0].clone().into(), None)
            .await
            .expect("should not fail");

        let mut blocked = spawn(fanout.send(events[1].clone().into(), None));
        assert_pending!(blocked.poll());
        drop(blocked);

        (fanout, control, receivers, events)
    }

    async fn assert_next(receiver: &mut BufferReceiver<EventArray>, expected: &Event) {
        assert_eq!(Some(expected.clone().into()), receiver.next().await);
    }

    /// The event is already buffered, so this must complete without waiting.
    fn assert_next_ready(receiver: &mut BufferReceiver<EventArray>, expected: &Event) {
        let mut next = spawn(receiver.next());
        assert_eq!(assert_ready!(next.poll()), Some(expected.clone().into()));
    }

    #[tokio::test]
    async fn fanout_resumes_send_dropped_while_pending() {
        // Dropping `Fanout::send` while a sender is blocked must leave that send in the fanout.
        // The next send resumes it instead of dropping the event or the sender.
        let (mut fanout, _, mut receivers, events) = drop_blocked_fanout_send().await;

        for receiver in &mut receivers {
            assert_next(receiver, &events[0]).await;
        }

        // The resumed send flushes the dropped batch first. That fills the
        // one-slot buffers again, so the new batch stays pending until it is read.
        let mut resumed = spawn(fanout.send(events[2].clone().into(), None));
        assert_pending!(resumed.poll());

        for receiver in &mut receivers {
            assert_next_ready(receiver, &events[1]);
        }

        assert_ready!(resumed.poll()).expect("should not fail");
        drop(resumed);

        for receiver in &mut receivers {
            assert_next_ready(receiver, &events[2]);
        }
    }

    #[tokio::test]
    async fn fanout_detaches_dropped_send_on_remove() {
        // A control message applied at the start of the next send, outside `drive`, must detach a
        // batch left behind by a dropped send. Dropping that future instead would lose the batch.
        let (mut fanout, control, mut receivers, events) = drop_blocked_fanout_send().await;

        remove_sender_from_fanout(&control, 0);
        for receiver in &mut receivers {
            assert_next(receiver, &events[0]).await;
        }

        let mut resumed = spawn(fanout.send(events[2].clone().into(), None));
        assert_pending!(resumed.poll());
        assert_next(&mut receivers[1], &events[1]).await;
        assert_ready!(resumed.poll()).expect("should not fail");
        drop(resumed);
        assert_next(&mut receivers[1], &events[2]).await;

        // The detached task still delivers the dropped batch to the removed output.
        assert_next(&mut receivers[0], &events[1]).await;
        assert_eq!(None, receivers[0].next().await);
    }

    #[tokio::test]
    async fn fanout_detaches_dropped_send_on_pause() {
        let (mut fanout, control, mut receivers, events) = drop_blocked_fanout_send().await;

        let (mut old_receiver, new_sender) = start_sender_replace(&control, &mut receivers, 0, 1);
        assert_next(&mut old_receiver, &events[0]).await;
        assert_next(&mut receivers[1], &events[0]).await;

        // Pause detaches output 0, then the send waits until that output is replaced.
        let mut resumed = spawn(fanout.send(events[2].clone().into(), None));
        assert_pending!(resumed.poll());
        assert_next(&mut old_receiver, &events[1]).await;

        finish_sender_resume(&control, 0, new_sender);
        assert_pending!(resumed.poll());
        assert_next(&mut receivers[1], &events[1]).await;
        assert_ready!(resumed.poll()).expect("should not fail");
        drop(resumed);

        assert_next(&mut receivers[0], &events[2]).await;
        assert_next(&mut receivers[1], &events[2]).await;
    }

    #[tokio::test]
    #[should_panic(expected = "Replacing existing sink is not valid")]
    async fn fanout_panics_on_replace_while_in_flight() {
        let (mut fanout, control, _receivers, events) = drop_blocked_fanout_send().await;
        let (replacement, _replacement_receiver) = build_sender_pair(1);

        control
            .send(ControlMessage::Replace(
                ComponentKey::from("0"),
                replacement,
            ))
            .expect("sending control message should not fail");

        fanout
            .send(events[2].clone().into(), None)
            .await
            .expect("should not fail");
    }

    #[tokio::test]
    async fn fanout_grow() {
        let (mut fanout, _, mut receivers) = fanout_from_senders(&[4, 4]);
        let events = make_events(3);

        // Send in the first two events to our initial two senders:
        fanout
            .send(events[0].clone().into(), None)
            .await
            .expect("should not fail");
        fanout
            .send(events[1].clone().into(), None)
            .await
            .expect("should not fail");

        // Now add a third sender:
        add_sender_to_fanout(&mut fanout, &mut receivers, 2, 4);

        // Send in the last event which all three senders will now get:
        fanout
            .send(events[2].clone().into(), None)
            .await
            .expect("should not fail");

        // Make sure the first two senders got all three events, but the third sender only got the
        // last event:
        let expected_events = [&events, &events, &events[2..]];
        for (i, receiver) in receivers.into_iter().enumerate() {
            assert_eq!(
                collect_ready_events(receiver.into_stream()),
                expected_events[i]
            );
        }
    }

    #[tokio::test]
    async fn fanout_shrink() {
        let (mut fanout, control, receivers) = fanout_from_senders(&[4, 4]);
        let events = make_events(3);

        // Send in the first two events to our initial two senders:
        fanout
            .send(events[0].clone().into(), None)
            .await
            .expect("should not fail");
        fanout
            .send(events[1].clone().into(), None)
            .await
            .expect("should not fail");

        // Now remove the second sender:
        remove_sender_from_fanout(&control, 1);

        // Send in the last event which only the first sender will get:
        fanout
            .send(events[2].clone().into(), None)
            .await
            .expect("should not fail");

        // Make sure the first sender got all three events, but the second sender only got the first two:
        let expected_events = [&events, &events[..2]];
        for (i, receiver) in receivers.into_iter().enumerate() {
            assert_eq!(
                collect_ready_events(receiver.into_stream()),
                expected_events[i]
            );
        }
    }

    #[tokio::test]
    async fn fanout_shrink_when_notready() {
        // This test exercises that when we're waiting for a send to complete, we can correctly
        // remove a sink whether or not it is the one that the send operation is still waiting on.
        //
        // This means that if we remove a sink that a current send is blocked on, we should be able
        // to immediately proceed.
        let events = make_events(2);
        let expected_first_event = unwrap_log_event_message(events[0].clone());
        let expected_second_event = unwrap_log_event_message(events[1].clone());

        let cases = [
            // Sender ID to drop, whether the second send should succeed after dropping, and the
            // final "last event" a receiver should see after the second send:
            (
                0,
                false,
                [
                    expected_second_event.clone(),
                    expected_first_event.clone(),
                    expected_second_event.clone(),
                ],
            ),
            (
                1,
                true,
                [
                    expected_second_event.clone(),
                    expected_second_event.clone(),
                    expected_second_event.clone(),
                ],
            ),
            (
                2,
                false,
                [
                    expected_second_event.clone(),
                    expected_first_event.clone(),
                    expected_second_event.clone(),
                ],
            ),
        ];

        for (sender_id, should_complete, expected_last_seen) in cases {
            let (mut fanout, control, mut receivers) = fanout_from_senders(&[2, 1, 2]);

            // First send should immediately complete because all senders have capacity:
            let mut first_send = spawn(fanout.send(events[0].clone().into(), None));
            assert_ready!(first_send.poll()).expect("should not fail");
            drop(first_send);

            // Second send should return pending because sender B is now full:
            let mut second_send = spawn(fanout.send(events[1].clone().into(), None));
            assert_pending!(second_send.poll());

            // Now drop our chosen sender and assert that polling the second send behaves as expected:
            remove_sender_from_fanout(&control, sender_id);

            if should_complete {
                assert_ready!(second_send.poll()).expect("should not fail");
            } else {
                assert_pending!(second_send.poll());
            }
            drop(second_send);

            // Now grab the last value available to each receiver and assert it's the second event.
            drop(fanout);

            let mut last_seen = Vec::new();
            for receiver in &mut receivers {
                let mut events = Vec::new();
                while let Some(event) = receiver.next().await {
                    events.insert(0, event);
                }

                last_seen.push(unwrap_log_event_message(events.remove(0)));
            }

            assert_eq!(&expected_last_seen[..], &last_seen);
        }
    }

    #[tokio::test]
    async fn fanout_no_sinks() {
        let (mut fanout, _) = Fanout::new(ComponentKey::from("test_upstream"));
        let events = make_events(2);

        fanout
            .send(events[0].clone().into(), None)
            .await
            .expect("should not fail");
        fanout
            .send(events[1].clone().into(), None)
            .await
            .expect("should not fail");
    }

    #[cfg(debug_assertions)]
    #[tokio::test]
    #[should_panic(expected = "Fanout received empty event batch from upstream component")]
    async fn fanout_panics_on_empty_event_array_in_debug_builds() {
        let (mut fanout, _, _receivers) = fanout_from_senders(&[2, 2]);
        let empty: EventArray = Vec::<LogEvent>::new().into();

        _ = fanout.send(empty, None).await;
    }

    #[tokio::test]
    async fn fanout_replace() {
        let (mut fanout, control, mut receivers) = fanout_from_senders(&[4, 4, 4]);
        let events = make_events(3);

        // First two sends should immediately complete because all senders have capacity:
        fanout
            .send(events[0].clone().into(), None)
            .await
            .expect("should not fail");
        fanout
            .send(events[1].clone().into(), None)
            .await
            .expect("should not fail");

        // Replace the first sender with a brand new one before polling again:
        let old_first_receiver = replace_sender_in_fanout(&control, &mut receivers, 0, 4);

        // And do the third send which should also complete since all senders still have capacity:
        fanout
            .send(events[2].clone().into(), None)
            .await
            .expect("should not fail");

        // Now make sure that the new "first" sender only got the third event, but that the second and
        // third sender got all three events:
        let expected_events = [&events[2..], &events, &events];
        for (i, receiver) in receivers.into_iter().enumerate() {
            assert_eq!(
                collect_ready_events(receiver.into_stream()),
                expected_events[i]
            );
        }

        // And make sure our original "first" sender got the first two events:
        assert_eq!(
            collect_ready_events(old_first_receiver.into_stream()),
            &events[..2]
        );
    }

    #[tokio::test]
    async fn fanout_wait() {
        let (mut fanout, control, mut receivers) = fanout_from_senders(&[4, 4]);
        let events = make_events(3);

        // First two sends should immediately complete because all senders have capacity:
        let send1 = Box::pin(fanout.send(events[0].clone().into(), None));
        assert_ready!(poll!(send1)).expect("should not fail");
        let send2 = Box::pin(fanout.send(events[1].clone().into(), None));
        assert_ready!(poll!(send2)).expect("should not fail");

        // Now do an empty replace on the second sender, which we'll test to make sure that `Fanout`
        // doesn't let any writes through until we replace it properly.  We get back the receiver
        // we've replaced, but also the sender that we want to eventually install:
        let (old_first_receiver, new_first_sender) =
            start_sender_replace(&control, &mut receivers, 0, 4);

        // Third send should return pending because now we have an in-flight replacement:
        let mut third_send = spawn(fanout.send(events[2].clone().into(), None));
        assert_pending!(third_send.poll());

        // Finish our sender replacement, which should wake up the third send and allow it to
        // actually complete:
        finish_sender_resume(&control, 0, new_first_sender);
        assert!(third_send.is_woken());
        assert_ready!(third_send.poll()).expect("should not fail");

        // Make sure the original first sender got the first two events, the new first sender got
        // the last event, and the second sender got all three:
        assert_eq!(
            collect_ready_events(old_first_receiver.into_stream()),
            &events[0..2]
        );

        let expected_events = [&events[2..], &events];
        for (i, receiver) in receivers.into_iter().enumerate() {
            assert_eq!(
                collect_ready_events(receiver.into_stream()),
                expected_events[i]
            );
        }
    }

    fn make_events_inner(count: usize) -> impl Iterator<Item = LogEvent> {
        (0..count).map(|i| LogEvent::from(format!("line {i}")))
    }

    fn make_events(count: usize) -> Vec<Event> {
        make_events_inner(count).map(Into::into).collect()
    }

    fn make_event_array(count: usize) -> EventArray {
        make_events_inner(count).collect::<Vec<_>>().into()
    }
}
