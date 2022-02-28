use futures::{task::AtomicWaker, Sink, Stream, StreamExt};
use indexmap::IndexMap;
use pin_project::pin_project;
use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::mpsc;

use crate::{config::ComponentKey, event::EventArray};

use vector_buffers::topology::channel::BufferSender;

pub enum ControlMessage {
    Add(ComponentKey, BufferSender<EventArray>),
    Remove(ComponentKey),
    /// Will stop accepting events until Some with given id is replaced.
    Replace(ComponentKey, Option<BufferSender<EventArray>>),
}

impl fmt::Debug for ControlMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ControlMessage::")?;
        match self {
            Self::Add(id, _) => write!(f, "Add({:?})", id),
            Self::Remove(id) => write!(f, "Remove({:?})", id),
            Self::Replace(id, _) => write!(f, "Replace({:?})", id),
        }
    }
}

pub type ControlChannel = mpsc::UnboundedSender<ControlMessage>;

pub struct Fanout {
    sinks: IndexMap<ComponentKey, Option<BufferSender<EventArray>>>,
    control_channel: mpsc::UnboundedReceiver<ControlMessage>,
}

impl Fanout {
    pub fn new() -> (Self, ControlChannel) {
        let (control_tx, control_rx) = mpsc::unbounded_channel();

        let fanout = Self {
            sinks: Default::default(),
            control_channel: control_rx,
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
            !self.sinks.contains_key(&id),
            "Adding duplicate output id to fanout: {id}"
        );
        self.sinks.insert(id, Some(sink));
    }

    /// Remove an existing sink as an output.
    ///
    /// # Panics
    ///
    /// Will panic if there is no sink with the given ID.
    fn remove(&mut self, id: &ComponentKey) {
        assert!(
            self.sinks.remove(id).is_some(),
            "Removing non-existent sink from fanout: {id}"
        );
    }

    /// Replace an existing sink as an output.
    ///
    /// If the `sink` passed is `None`, operation of the `Fanout` will be paused until a `Some`
    /// with the same key is received. This allows for cases where the previous version of
    /// a stateful sink must be dropped before the new version can be created.
    ///
    /// # Panics
    ///
    /// Will panic if there is no sink with the given ID.
    fn replace(&mut self, id: &ComponentKey, sink: Option<BufferSender<EventArray>>) {
        if let Some(existing) = self.sinks.get_mut(id) {
            *existing = sink;
        } else {
            panic!("Replacing non-existent sink from fanout: {id}");
        }
    }

    /// Apply a control message directly against this instance.
    ///
    /// This method should not be used if there is an active `SendGroup` being processed.
    fn apply_control_message(&mut self, message: ControlMessage) {
        match message {
            ControlMessage::Add(id, sink) => self.add(id, sink),
            ControlMessage::Remove(id) => self.remove(&id),
            ControlMessage::Replace(id, sink) => self.replace(&id, sink),
        }
    }

    /// If any sink is awaiting replacement (i.e. it was temporarily replaced with `None`), read
    /// and process messages from the control channel until that is no longer true.
    async fn wait_for_replacements(&mut self) {
        while self.sinks.iter().any(|x| x.1.is_none()) {
            if let Some(msg) = self.control_channel.recv().await {
                self.apply_control_message(msg);
            } else {
                // If the control channel is closed, there's nothing else we can do.
            }
        }
    }

    pub async fn send_stream(&mut self, events: impl Stream<Item = EventArray>) {
        tokio::pin!(events);
        while let Some(event_array) = events.next().await {
            self.send(event_array).await;
        }
    }

    /// Send a batch of events to all connected sinks.
    ///
    /// This will block on the resolution of any pending reload before proceeding with the send
    /// operation.
    ///
    /// # Panics
    ///
    /// This method can panic if the fanout receives a control message that violates some invariant
    /// about its current state (e.g. remove a non-existent sink, etc). This would imply a bug in
    /// Vector's config reloading logic.
    pub async fn send(&mut self, events: EventArray) {
        // First, process any available control messages
        while let Ok(message) = self.control_channel.try_recv() {
            self.apply_control_message(message);
        }
        // If we're left in a state with pending changes, wait for those to be completed before
        // initiating the send operation.
        self.wait_for_replacements().await;

        // The call to `wait_for_replacements` above ensures that all replacement operations are
        // complete at this point, and we don't need to worry about any of the sinks being `None`.
        let sink_count = self.sinks.iter().filter(|x| x.1.is_some()).count();
        debug_assert_eq!(sink_count, self.sinks.len());

        if self.sinks.is_empty() {
            return;
        }

        let mut clone_army: Vec<EventArray> = Vec::with_capacity(sink_count);
        for _ in 0..(sink_count - 1) {
            clone_army.push(events.clone());
        }
        clone_army.push(events);

        let sinks = self
            .sinks
            .drain(..)
            .map(|(id, sink)| (id, sink.expect("no replacements in progress")))
            .zip(clone_army);

        let mut send_group = SendGroup::with_capacity(sink_count);

        for ((id, sink), events) in sinks {
            send_group.push(id, sink, events);
        }

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
                        Some(msg) => {
                            route_control_message(msg, self, &mut send_group);
                        },
                        None => {
                            // Control channel is closed, process must be shutting down
                            control_channel_open = false;
                        }
                    }
                }

                sinks = &mut send_group => {
                    // All in-flight sends have completed, so return sinks to the base collection.
                    // We extend instead of assign here because other sinks could have been added
                    // while the send was in-flight.
                    self.sinks.extend(sinks);
                    break;
                }
            }
        }
    }
}

/// Apply a given control message to either the active `SendGroup` or the `Fanout` itself.
///
/// Given the way that send operations move their respective sinks into the `SendOp` future, it can
/// be complex to apply control messages while sends are in-flight in a way that upholds
/// invariants. This functions handles the task of conditionally applying messages to the
/// `SendGroup` and falling back to applying them to the `Fanout` if the return value indicates
/// they were not applicable to the `SendGroup`.
///
/// # Panics
///
/// If a message is received that violates an invariant (e.g. adding a duplicate sink, removing one
/// that doesn't exist, etc), then this function will panic. Generally, the invariant itself is
/// upheld by the modification methods on `Fanout`, which will panic if incorrectly used. We
/// inherit that behavior here by attempting to apply the `SendGroup` methods first (which use
/// return values to indicate failure rather than panicking) and then falling back to the `Fanout`
/// equivalent methods.
fn route_control_message(message: ControlMessage, fanout: &mut Fanout, send_group: &mut SendGroup) {
    match message {
        ControlMessage::Add(id, sink) => {
            // Ensure we don't already have a sink with the same id as part of the send operation
            assert!(!send_group.contains(&id));
            fanout.add(id, sink);
        }
        ControlMessage::Remove(id) => {
            if !send_group.remove(&id) {
                fanout.remove(&id);
            }
        }
        ControlMessage::Replace(id, None) => {
            if !send_group.pause(&id) {
                fanout.replace(&id, None);
            }
        }
        ControlMessage::Replace(id, Some(sink)) => {
            if let Err(sink) = send_group.replace(&id, sink) {
                fanout.replace(&id, Some(sink));
            }
        }
    }
}

#[pin_project]
struct SendGroup {
    #[pin]
    sends: IndexMap<ComponentKey, SendOp>,
    waker: Arc<AtomicWaker>,
}

impl SendGroup {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            sends: IndexMap::with_capacity(capacity),
            waker: Arc::new(AtomicWaker::new()),
        }
    }

    fn push(&mut self, id: ComponentKey, sink: BufferSender<EventArray>, input: EventArray) {
        let send = SendOp::new(sink, input, Arc::clone(&self.waker));
        self.sends.insert(id, send);
    }

    fn contains(&mut self, id: &ComponentKey) -> bool {
        self.sends.contains_key(id)
    }

    fn remove(&mut self, id: &ComponentKey) -> bool {
        self.sends.remove(id).is_some()
    }

    fn replace(
        &mut self,
        id: &ComponentKey,
        sink: BufferSender<EventArray>,
    ) -> Result<(), BufferSender<EventArray>> {
        if let Some(send) = self.sends.get_mut(id) {
            send.replace(sink);
            // This may have unpaused a send operation, so make sure it is woken up.
            self.waker.wake();
            Ok(())
        } else {
            Err(sink)
        }
    }

    fn pause(&mut self, id: &ComponentKey) -> bool {
        if let Some(send) = self.sends.get_mut(id) {
            send.pause();
            true
        } else {
            false
        }
    }
}

impl Future for SendGroup {
    type Output = Vec<(ComponentKey, Option<BufferSender<EventArray>>)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut pending = false;
        let this = self.as_mut().project();
        for (_key, send) in this.sends.get_mut() {
            let send = Pin::new(send);
            if send.poll(cx).is_pending() {
                pending = true;
            }
        }

        if pending {
            Poll::Pending
        } else {
            Poll::Ready(
                self.sends
                    .drain(..)
                    .map(|(id, send)| (id, Some(send.take())))
                    .collect(),
            )
        }
    }
}

#[pin_project]
struct SendOp {
    #[pin]
    state: SendState<BufferSender<EventArray>>,
    slot: Option<EventArray>,
    waker: Arc<AtomicWaker>,
}

#[pin_project(project = SendStateProj)]
enum SendState<T> {
    Active(#[pin] T),
    Paused,
}

impl SendOp {
    fn new(sink: BufferSender<EventArray>, input: EventArray, waker: Arc<AtomicWaker>) -> Self {
        Self {
            state: SendState::Active(sink),
            slot: Some(input),
            waker,
        }
    }

    fn replace(&mut self, sink: BufferSender<EventArray>) {
        self.state = SendState::Active(sink);
    }

    fn pause(&mut self) {
        self.state = SendState::Paused;
    }

    fn take(self) -> BufferSender<EventArray> {
        match self.state {
            SendState::Active(sink) => sink,
            SendState::Paused => panic!("attempting to take a paused sink"),
        }
    }
}

impl Future for SendOp {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        loop {
            match this.state.as_mut().project() {
                SendStateProj::Active(mut sink) => {
                    if let Some(event_array) = this.slot.take() {
                        match sink.as_mut().poll_ready(cx) {
                            Poll::Ready(Ok(())) => {
                                sink.start_send(event_array).expect("unit error");
                            }
                            Poll::Ready(Err(())) => {
                                panic!("unit error");
                            }
                            Poll::Pending => {
                                *this.slot = Some(event_array);
                                return Poll::Pending;
                            }
                        }
                    } else {
                        return match sink.as_mut().poll_flush(cx) {
                            Poll::Ready(Ok(())) => Poll::Ready(()),
                            Poll::Ready(Err(())) => panic!("unit error"),
                            Poll::Pending => Poll::Pending,
                        };
                    }
                }
                SendStateProj::Paused => {
                    // This likely isn't strictly necessary given how this future is used right now
                    // (i.e. only a single task, gets polled in the same select loop that wakes
                    // it), but it would be a bit of a footgun to leave out this part of the
                    // `Future` contract. Basically, this ensure that even if the future is spawned
                    // some other way, we'll get woken up to make progress when the sink is added
                    // back.
                    this.waker.register(cx.waker());
                    return Poll::Pending;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem;

    use futures::{poll, StreamExt};
    use tokio::sync::mpsc::UnboundedSender;
    use tokio_test::{assert_pending, assert_ready, task::spawn};
    use vector_buffers::{
        topology::{
            builder::TopologyBuilder,
            channel::{BufferReceiver, BufferSender},
        },
        WhenFull,
    };

    use super::{ControlMessage, Fanout};
    use crate::config::ComponentKey;
    use crate::event::{Event, EventArray, LogEvent};
    use crate::test_util::{collect_ready, collect_ready_events};

    async fn build_sender_pair(
        capacity: usize,
    ) -> (BufferSender<EventArray>, BufferReceiver<EventArray>) {
        TopologyBuilder::standalone_memory(capacity, WhenFull::Block).await
    }

    async fn build_sender_pairs(
        capacities: &[usize],
    ) -> Vec<(BufferSender<EventArray>, BufferReceiver<EventArray>)> {
        let mut pairs = Vec::new();
        for capacity in capacities {
            pairs.push(build_sender_pair(*capacity).await);
        }
        pairs
    }

    async fn fanout_from_senders(
        capacities: &[usize],
    ) -> (
        Fanout,
        UnboundedSender<ControlMessage>,
        Vec<BufferReceiver<EventArray>>,
    ) {
        let (mut fanout, control) = Fanout::new();
        let pairs = build_sender_pairs(capacities).await;

        let mut receivers = Vec::new();
        for (i, (sender, receiver)) in pairs.into_iter().enumerate() {
            fanout.add(ComponentKey::from(i.to_string()), sender);
            receivers.push(receiver);
        }

        (fanout, control, receivers)
    }

    async fn add_sender_to_fanout(
        fanout: &mut Fanout,
        receivers: &mut Vec<BufferReceiver<EventArray>>,
        sender_id: usize,
        capacity: usize,
    ) {
        let (sender, receiver) = build_sender_pair(capacity).await;
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

    async fn replace_sender_in_fanout(
        control: &UnboundedSender<ControlMessage>,
        receivers: &mut Vec<BufferReceiver<EventArray>>,
        sender_id: usize,
        capacity: usize,
    ) -> BufferReceiver<EventArray> {
        let (sender, receiver) = build_sender_pair(capacity).await;
        let old_receiver = mem::replace(&mut receivers[sender_id], receiver);

        control
            .send(ControlMessage::Replace(
                ComponentKey::from(sender_id.to_string()),
                Some(sender),
            ))
            .expect("sending control message should not fail");

        old_receiver
    }

    async fn start_sender_replace(
        control: &UnboundedSender<ControlMessage>,
        receivers: &mut Vec<BufferReceiver<EventArray>>,
        sender_id: usize,
        capacity: usize,
    ) -> (BufferReceiver<EventArray>, BufferSender<EventArray>) {
        let (sender, receiver) = build_sender_pair(capacity).await;
        let old_receiver = mem::replace(&mut receivers[sender_id], receiver);

        control
            .send(ControlMessage::Replace(
                ComponentKey::from(sender_id.to_string()),
                None,
            ))
            .expect("sending control message should not fail");

        (old_receiver, sender)
    }

    fn finish_sender_replace(
        control: &UnboundedSender<ControlMessage>,
        sender_id: usize,
        sender: BufferSender<EventArray>,
    ) {
        control
            .send(ControlMessage::Replace(
                ComponentKey::from(sender_id.to_string()),
                Some(sender),
            ))
            .expect("sending control message should not fail");
    }

    #[tokio::test]
    async fn fanout_writes_to_all() {
        let (mut fanout, _, receivers) = fanout_from_senders(&[2, 2]).await;
        let events = make_event_array(2);

        let clones = events.clone();
        fanout.send(clones.into()).await;

        for receiver in receivers {
            assert_eq!(collect_ready(receiver), &[events.clone()]);
        }
    }

    #[tokio::test]
    async fn fanout_notready() {
        let (mut fanout, _, mut receivers) = fanout_from_senders(&[2, 1, 2]).await;
        let events = make_events(2);

        // First send should immediately complete because all senders have capacity:
        let mut first_send = spawn(fanout.send(events[0].clone().into()));
        assert_ready!(first_send.poll());
        drop(first_send);

        // Second send should return pending because sender B is now full:
        let mut second_send = spawn(fanout.send(events[1].clone().into()));
        assert_pending!(second_send.poll());

        // Now read an item from each receiver to free up capacity for the second sender:
        for receiver in &mut receivers {
            assert_eq!(Some(events[0].clone().into()), receiver.next().await);
        }

        // Now our second send should actually be able to complete:
        assert_ready!(second_send.poll());
        drop(second_send);

        // And make sure the second item comes through:
        for receiver in &mut receivers {
            assert_eq!(Some(events[1].clone().into()), receiver.next().await);
        }
    }

    #[tokio::test]
    async fn fanout_grow() {
        let (mut fanout, _, mut receivers) = fanout_from_senders(&[4, 4]).await;
        let events = make_events(3);

        // Send in the first two events to our initial two senders:
        fanout.send(events[0].clone().into()).await;
        fanout.send(events[1].clone().into()).await;

        // Now add a third sender:
        add_sender_to_fanout(&mut fanout, &mut receivers, 2, 4).await;

        // Send in the last event which all three senders will now get:
        fanout.send(events[2].clone().into()).await;

        // Make sure the first two senders got all three events, but the third sender only got the
        // last event:
        assert_eq!(collect_ready_events(&mut receivers[0]), &events[..]);
        assert_eq!(collect_ready_events(&mut receivers[1]), &events[..]);
        assert_eq!(collect_ready_events(&mut receivers[2]), &events[2..]);
    }

    #[tokio::test]
    async fn fanout_shrink() {
        let (mut fanout, control, mut receivers) = fanout_from_senders(&[4, 4]).await;
        let events = make_events(3);

        // Send in the first two events to our initial two senders:
        fanout.send(events[0].clone().into()).await;
        fanout.send(events[1].clone().into()).await;

        // Now remove the second sender:
        remove_sender_from_fanout(&control, 1);

        // Send in the last event which only the first sender will get:
        fanout.send(events[2].clone().into()).await;

        // Make sure the first sender got all three events, but the second sender only got the first two:
        let expected_events = [&events, &events[..2]];
        for (i, receiver) in receivers.iter_mut().enumerate() {
            assert_eq!(collect_ready_events(receiver), expected_events[i]);
        }
    }

    #[tokio::test]
    async fn fanout_shrink_when_notready() {
        // This test exercises that when we're waiting for a send to complete, we can correctly
        // remove a sink whether or not it is the one that the send operation is still waiting on.
        let events = make_events(2);
        let mut results: Vec<Vec<Option<()>>> = Vec::new();

        for sender_id in [0, 1, 2] {
            let (mut fanout, control, mut receivers) = fanout_from_senders(&[2, 1, 2]).await;
            let events = events.clone();

            // First send should immediately complete because all senders have capacity:
            let mut first_send = spawn(fanout.send(events[0].clone().into()));
            assert_ready!(first_send.poll());
            drop(first_send);

            // Second send should return pending because sender B is now full:
            let mut second_send = spawn(fanout.send(events[1].clone().into()));
            assert_pending!(second_send.poll());

            // Now read an item from each receiver to free up capacity:
            for receiver in &mut receivers {
                assert_eq!(Some(events[0].clone().into()), receiver.next().await);
            }

            // Drop the given sender before polling again:
            remove_sender_from_fanout(&control, sender_id);

            // Now our second send should actually be able to complete.  We'll assert that whichever
            // sender we removed does not get the next event:
            assert_ready!(second_send.poll());
            drop(second_send);

            let mut scenario_results = Vec::new();
            for receiver in &mut receivers {
                scenario_results.push(receiver.next().await.map(|_| ()));
            }
            results.push(scenario_results);
        }

        let expected = [
            // When we remove the first sender, it will still receive the event because it had
            // capacity at the time the send was initiated.
            [Some(()), Some(()), Some(())],
            // When we remove the second sender, it will not receive the event because it was
            // full when the send was initiated and removed before it could progress.
            [Some(()), None, Some(())],
            // Same as with the first, the third sender receives the event when removed because it
            // had space when the send was initiated.
            [Some(()), Some(()), Some(())],
        ];
        assert_eq!(results, expected);
    }

    #[tokio::test]
    async fn fanout_no_sinks() {
        let (mut fanout, _) = Fanout::new();
        let events = make_events(2);

        fanout.send(events[0].clone().into()).await;
        fanout.send(events[1].clone().into()).await;
    }

    #[tokio::test]
    async fn fanout_replace() {
        let (mut fanout, control, mut receivers) = fanout_from_senders(&[4, 4, 4]).await;
        let events = make_events(3);

        // First two sends should immediately complete because all senders have capacity:
        fanout.send(events[0].clone().into()).await;
        fanout.send(events[1].clone().into()).await;

        // Replace the first sender with a brand new one before polling again:
        let old_first_receiver = replace_sender_in_fanout(&control, &mut receivers, 0, 4).await;

        // And do the third send which should also complete since all senders still have capacity:
        fanout.send(events[2].clone().into()).await;

        // Now make sure that the new "first" sender only got the third event, but that the second and
        // third sender got all three events:
        let expected_events = [&events[2..], &events, &events];
        for (i, receiver) in receivers.iter_mut().enumerate() {
            assert_eq!(collect_ready_events(receiver), expected_events[i]);
        }

        // And make sure our original "first" sender got the first two events:
        assert_eq!(collect_ready_events(old_first_receiver), &events[..2]);
    }

    #[tokio::test]
    async fn fanout_wait() {
        let (mut fanout, control, mut receivers) = fanout_from_senders(&[4, 4]).await;
        let events = make_events(3);

        // First two sends should immediately complete because all senders have capacity:
        let send1 = Box::pin(fanout.send(events[0].clone().into()));
        assert_ready!(poll!(send1));
        let send2 = Box::pin(fanout.send(events[1].clone().into()));
        assert_ready!(poll!(send2));

        // Now do an empty replace on the second sender, which we'll test to make sure that `Fanout`
        // doesn't let any writes through until we replace it properly.  We get back the receiver
        // we've replaced, but also the sender that we want to eventually install:
        let (old_first_receiver, new_first_sender) =
            start_sender_replace(&control, &mut receivers, 0, 4).await;

        // Third send should return pending because now we have an in-flight replacement:
        let mut third_send = spawn(fanout.send(events[2].clone().into()));
        assert_pending!(third_send.poll());

        // Finish our sender replacement, which should wake up the third send and allow it to
        // actually complete:
        finish_sender_replace(&control, 0, new_first_sender);
        assert!(third_send.is_woken());
        assert_ready!(third_send.poll());

        // Make sure the original first sender got the first two events, the new first sender got
        // the last event, and the second sender got all three:
        assert_eq!(collect_ready_events(old_first_receiver), &events[0..2]);
        assert_eq!(collect_ready_events(&mut receivers[0]), &events[2..]);
        assert_eq!(collect_ready_events(&mut receivers[1]), &events[..]);
    }

    fn _make_events(count: usize) -> impl Iterator<Item = LogEvent> {
        (0..count).map(|i| LogEvent::from(format!("line {}", i)))
    }

    fn make_events(count: usize) -> Vec<Event> {
        _make_events(count).map(Into::into).collect()
    }

    fn make_event_array(count: usize) -> EventArray {
        _make_events(count).collect::<Vec<_>>().into()
    }
}
