use std::{collections::HashMap, fmt, task::Poll};

use futures::{Stream, StreamExt};
use futures_util::{pending, poll};
use indexmap::IndexMap;
use tokio::sync::mpsc;
use tokio_util::sync::ReusableBoxFuture;
use vector_buffers::topology::channel::BufferSender;

use crate::{config::ComponentKey, event::EventArray};

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
            Self::Replace(id, sink) => {
                let status = if sink.is_none() {
                    "pausing"
                } else {
                    "unpausing"
                };
                write!(f, "Replace({:?}, {})", id, status)
            }
        }
    }
}

// TODO: We should really wrap this in a custom type that has dedicated methods for each operation
// so that high-lever components don't need to do the raw channel sends, etc.
pub type ControlChannel = mpsc::UnboundedSender<ControlMessage>;

pub struct Fanout {
    senders: IndexMap<ComponentKey, Option<Sender>>,
    control_channel: mpsc::UnboundedReceiver<ControlMessage>,
}

impl Fanout {
    pub fn new() -> (Self, ControlChannel) {
        let (control_tx, control_rx) = mpsc::unbounded_channel();

        let fanout = Self {
            senders: Default::default(),
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
            !self.senders.contains_key(&id),
            "Adding duplicate output id to fanout: {id}"
        );
        self.senders.insert(id, Some(Sender::new(sink)));
    }

    fn remove(&mut self, id: &ComponentKey) {
        assert!(
            self.senders.remove(id).is_some(),
            "Removing non-existent sink from fanout: {id}"
        );
    }

    fn replace(&mut self, id: &ComponentKey, sink: BufferSender<EventArray>) {
        match self.senders.get_mut(id) {
            Some(sender) => {
                // While a sink must be _known_ to be replaced, it must also be empty (previously
                // paused or consumed when the `SendGroup` was created), otherwise an invalid
                // sequence of control operations has been applied.
                assert!(
                    sender.replace(Sender::new(sink)).is_none(),
                    "Replacing existing sink is not valid: {id}"
                );
            }
            None => panic!("Replacing unknown sink from fanout: {id}"),
        }
    }

    fn pause(&mut self, id: &ComponentKey) {
        match self.senders.get_mut(id) {
            Some(sender) => {
                // A sink must be known and present to be replaced, otherwise an invalid sequence of
                // control operations has been applied.
                assert!(
                    sender.take().is_some(),
                    "Pausing non-existent sink is not valid: {id}"
                );
            }
            None => panic!("Pausing unknown sink from fanout: {id}"),
        }
    }

    /// Apply a control message directly against this instance.
    ///
    /// This method should not be used if there is an active `SendGroup` being processed.
    fn apply_control_message(&mut self, message: ControlMessage) {
        trace!("Processing control message outside of send: {:?}", message);

        match message {
            ControlMessage::Add(id, sink) => self.add(id, sink),
            ControlMessage::Remove(id) => self.remove(&id),
            ControlMessage::Replace(id, None) => self.pause(&id),
            ControlMessage::Replace(id, Some(sink)) => self.replace(&id, sink),
        }
    }

    /// If any sink is awaiting replacement (i.e. it was temporarily replaced with `None`), read
    /// and process messages from the control channel until that is no longer true.
    async fn wait_for_replacements(&mut self) {
        while self.senders.values().any(Option::is_none) {
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
        // First, process any available control messages in a non-blocking fashion.  If any of our
        // senders were replaced, we additionally wait until they're replaced.
        while let Ok(message) = self.control_channel.try_recv() {
            self.apply_control_message(message);
        }

        self.wait_for_replacements().await;

        // Nothing to send if we have no sender.
        if self.senders.is_empty() {
            trace!("No senders present.");
            return;
        }

        // Keep track of whether the control channel has returned `Ready(None)`, and stop polling
        // it once it has. If we don't do this check, it will continue to return `Ready(None)` any
        // time it is polled, which can lead to a busy loop below.
        //
        // In real life this is likely a non-issue, but it can lead to strange behavior in tests if
        // left unhandled.
        let mut control_channel_open = true;

        // Create our send group which arms all senders to send the given events, and handles
        // adding/removing/replacing senders while the send is in-flight.
        let mut send_group = SendGroup::new(&mut self.senders, events);

        loop {
            tokio::select! {
                // Semantically, it's not hugely important that this select is biased. It does,
                // however, make testing simpler when you can count on control messages being
                // processed first.
                biased;

                maybe_msg = self.control_channel.recv(), if control_channel_open => {
                    trace!("Processing control message inside of send: {:?}", maybe_msg);

                    // During a send operation, control messages must be applied via the
                    // `SendGroup`, since it has exclusive access to the senders.
                    match maybe_msg {
                        Some(ControlMessage::Add(id, sink)) => {
                            send_group.add(id, sink);
                        },
                        Some(ControlMessage::Remove(id)) => {
                            send_group.remove(&id);
                        },
                        Some(ControlMessage::Replace(id, Some(sink))) => {
                            send_group.replace(&id, Sender::new(sink));
                        },
                        Some(ControlMessage::Replace(id, None)) => {
                            send_group.pause(&id);
                        },
                        None => {
                            // Control channel is closed, process must be shutting down
                            control_channel_open = false;
                        }
                    }
                }

                () = send_group.send() => {
                    // All in-flight sends have completed, so return sinks to the base collection.
                    // We extend instead of assign here because other sinks could have been added
                    // while the send was in-flight.
                    trace!("Sent item to fanout.");
                    break;
                }
            }
        }
    }
}

struct SendGroup<'a> {
    senders: &'a mut IndexMap<ComponentKey, Option<Sender>>,
    sends: HashMap<ComponentKey, ReusableBoxFuture<'static, Sender>>,
}

impl<'a> SendGroup<'a> {
    fn new(senders: &'a mut IndexMap<ComponentKey, Option<Sender>>, events: EventArray) -> Self {
        // If we don't have a valid `Sender` for all sinks, then something went wrong in our logic
        // to ensure we were starting with all valid/idle senders prior to initiating the send.
        debug_assert!(senders.values().all(Option::is_some));

        let last_sender_idx = senders.len().saturating_sub(1);
        let mut events = Some(events);

        // We generate a send future for each sender we have, which arms them with the events to
        // send but also takes ownership of the sender itself, which we give back when the sender completes.
        let mut sends = HashMap::new();
        for (i, (key, sender)) in senders.iter_mut().enumerate() {
            let mut sender = sender
                .take()
                .expect("sender must be present to initialize SendGroup");

            // First, arm each sender with the item to actually send.
            if i == last_sender_idx {
                sender.input = events.take();
            } else {
                sender.input = events.clone();
            }

            // Now generate a send for that sender which we'll drive to completion.
            let send = async move {
                sender.flush().await;
                sender
            };

            sends.insert(key.clone(), ReusableBoxFuture::new(send));
        }

        Self { senders, sends }
    }

    fn try_detach_send(&mut self, id: &ComponentKey) {
        if let Some(send) = self.sends.remove(id) {
            tokio::spawn(async move {
                send.await;
            });
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn add(&mut self, id: ComponentKey, sink: BufferSender<EventArray>) {
        // When we're in the middle of a send, we can only keep track of the new sink, but can't
        // actually send to it, as we don't have the item to send... so only add it to `senders`.
        assert!(
            self.senders
                .insert(id.clone(), Some(Sender::new(sink)))
                .is_none(),
            "Adding duplicate output id to fanout: {id}"
        );
    }

    fn remove(&mut self, id: &ComponentKey) {
        // We may or may not be removing a sender that we're try to drive a send against, so we have
        // to also detach the send future for the sender if it exists, otherwise we'd be hanging
        // around still trying to send to it.
        assert!(
            self.senders.remove(id).is_some(),
            "Removing non-existent sink from fanout: {id}"
        );

        // Now try and detach the in-flight send, if it exists.
        self.try_detach_send(id);
    }

    fn replace(&mut self, id: &ComponentKey, sink: Sender) {
        match self.senders.get_mut(id) {
            Some(sender) => {
                // While a sink must be _known_ to be replaced, it must also be empty (previously
                // paused or consumed when the `SendGroup` was created), otherwise an invalid
                // sequence of control operations has been applied.
                assert!(
                    sender.replace(sink).is_none(),
                    "Replacing existing sink is not valid: {id}"
                );
            }
            None => panic!("Replacing unknown sink from fanout: {id}"),
        }
    }

    fn pause(&mut self, id: &ComponentKey) {
        match self.senders.get_mut(id) {
            Some(sender) => {
                // A sink must be known and present to be replaced, otherwise an invalid sequence of
                // control operations has been applied.
                assert!(
                    sender.take().is_some(),
                    "Pausing non-existent sink is not valid: {id}"
                );
            }
            None => panic!("Pausing unknown sink from fanout: {id}"),
        }
    }

    async fn send(&mut self) {
        // Right now, we do a linear scan of all sends, polling each send once in order to avoid
        // waiting forever, such that we can let our control messages get picked up while sends are
        // waiting.
        loop {
            if self.sends.is_empty() {
                break;
            }

            let mut done = Vec::new();
            for (key, send) in &mut self.sends {
                if let Poll::Ready(sender) = poll!(send.get_pin()) {
                    // The send completed, so we restore the sender and mark ourselves so that this
                    // future gets dropped.
                    done.push((key.clone(), sender));
                }
            }

            for (key, sender) in done {
                self.sends.remove(&key);
                self.replace(&key, sender);
            }

            if !self.sends.is_empty() {
                // We manually yield ourselves because we've polled all of the sends at this point,
                // so if any are left, then we're scheduled for a wake-up... this is a really poor
                // approximation of what `FuturesUnordered` is doing.
                pending!();
            }
        }
    }
}

struct Sender {
    inner: BufferSender<EventArray>,
    input: Option<EventArray>,
}

impl Sender {
    fn new(inner: BufferSender<EventArray>) -> Self {
        Self { inner, input: None }
    }

    async fn flush(&mut self) {
        if let Some(input) = self.input.take() {
            self.inner.send(input).await.expect("unit error");
            self.inner.flush().await.expect("unit error");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem;
    use std::num::NonZeroUsize;

    use futures::poll;
    use tokio::sync::mpsc::UnboundedSender;
    use tokio_test::{assert_pending, assert_ready, task::spawn};
    use value::Value;
    use vector_buffers::{
        topology::{
            builder::TopologyBuilder,
            channel::{BufferReceiver, BufferSender},
        },
        WhenFull,
    };

    use super::{ControlMessage, Fanout};
    use crate::event::{Event, EventArray, LogEvent};
    use crate::test_util::{collect_ready, collect_ready_events};
    use crate::{config::ComponentKey, event::EventContainer};

    async fn build_sender_pair(
        capacity: usize,
    ) -> (BufferSender<EventArray>, BufferReceiver<EventArray>) {
        TopologyBuilder::standalone_memory(
            NonZeroUsize::new(capacity).expect("capacity must be nonzero"),
            WhenFull::Block,
        )
        .await
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
        receivers: &mut [BufferReceiver<EventArray>],
        sender_id: usize,
        capacity: usize,
    ) -> BufferReceiver<EventArray> {
        let (sender, receiver) = build_sender_pair(capacity).await;
        let old_receiver = mem::replace(&mut receivers[sender_id], receiver);

        control
            .send(ControlMessage::Replace(
                ComponentKey::from(sender_id.to_string()),
                None,
            ))
            .expect("sending control message should not fail");

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
        receivers: &mut [BufferReceiver<EventArray>],
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
            .get("message")
            .and_then(Value::as_bytes)
            .and_then(|b| String::from_utf8(b.to_vec()).ok())
            .expect("must be valid log event with `message` field")
    }

    #[tokio::test]
    async fn fanout_writes_to_all() {
        let (mut fanout, _, receivers) = fanout_from_senders(&[2, 2]).await;
        let events = make_event_array(2);

        let clones = events.clone();
        fanout.send(clones).await;

        for receiver in receivers {
            assert_eq!(collect_ready(receiver.into_stream()), &[events.clone()]);
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
        let (mut fanout, control, receivers) = fanout_from_senders(&[4, 4]).await;
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
            let (mut fanout, control, mut receivers) = fanout_from_senders(&[2, 1, 2]).await;

            // First send should immediately complete because all senders have capacity:
            let mut first_send = spawn(fanout.send(events[0].clone().into()));
            assert_ready!(first_send.poll());
            drop(first_send);

            // Second send should return pending because sender B is now full:
            let mut second_send = spawn(fanout.send(events[1].clone().into()));
            assert_pending!(second_send.poll());

            // Now drop our chosen sender and assert that polling the second send behaves as expected:
            remove_sender_from_fanout(&control, sender_id);

            if should_complete {
                assert_ready!(second_send.poll());
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
