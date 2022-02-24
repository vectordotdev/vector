use crate::{config::ComponentKey, event::Event};
use futures::Sink;
use futures_util::SinkExt;
use std::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;

type GenericEventSink = Pin<Box<dyn Sink<Event, Error = ()> + Send>>;

pub enum ControlMessage {
    Add(ComponentKey, GenericEventSink),
    Remove(ComponentKey),
    /// Will stop accepting events until Some with given id is replaced.
    Replace(ComponentKey, Option<GenericEventSink>),
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
    sinks: Vec<(ComponentKey, Option<GenericEventSink>)>,
    i: usize,
    control_channel: mpsc::UnboundedReceiver<ControlMessage>,
}

impl Fanout {
    pub fn new() -> (Self, ControlChannel) {
        let (control_tx, control_rx) = mpsc::unbounded_channel();

        let fanout = Self {
            sinks: vec![],
            i: 0,
            control_channel: control_rx,
        };

        (fanout, control_tx)
    }

    /// Add a new sink as an output.
    ///
    /// # Panics
    ///
    /// Function will panic if a sink with the same ID is already present.
    pub fn add(&mut self, id: ComponentKey, sink: GenericEventSink) {
        assert!(
            !self.sinks.iter().any(|(n, _)| n == &id),
            "Duplicate output id in fanout"
        );

        self.sinks.push((id, Some(sink)));
    }

    fn remove(&mut self, id: &ComponentKey) {
        let i = self.sinks.iter().position(|(n, _)| n == id);
        let i = i.expect("Didn't find output in fanout");

        let (_id, removed) = self.sinks.remove(i);

        if let Some(mut removed) = removed {
            tokio::spawn(async move { removed.close().await });
        }

        if self.i > i {
            self.i -= 1;
        }
    }

    fn replace(&mut self, id: &ComponentKey, sink: Option<GenericEventSink>) {
        if let Some((_, existing)) = self.sinks.iter_mut().find(|(n, _)| n == id) {
            *existing = sink;
        } else {
            panic!("Tried to replace a sink that's not already present");
        }
    }

    pub fn process_control_messages(&mut self, cx: &mut Context<'_>) {
        while let Poll::Ready(Some(message)) = self.control_channel.poll_recv(cx) {
            match message {
                ControlMessage::Add(id, sink) => self.add(id, sink),
                ControlMessage::Remove(id) => self.remove(&id),
                ControlMessage::Replace(id, sink) => self.replace(&id, sink),
            }
        }
    }

    #[inline]
    fn handle_sink_error(&mut self, index: usize) -> Result<(), ()> {
        // If there's only one sink, propagate the error to the source ASAP
        // so it stops reading from its input. If there are multiple sinks,
        // keep pushing to the non-errored ones (while the errored sink
        // triggers a more graceful shutdown).
        if self.sinks.len() == 1 {
            Err(())
        } else {
            self.sinks.remove(index);
            Ok(())
        }
    }

    fn poll_sinks<F>(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        poll: F,
    ) -> Poll<Result<(), ()>>
    where
        F: Fn(
            Pin<&mut (dyn Sink<Event, Error = ()> + Send)>,
            &mut Context<'_>,
        ) -> Poll<Result<(), ()>>,
    {
        self.process_control_messages(cx);

        let mut poll_result = Poll::Ready(Ok(()));

        let mut i = 0;
        while let Some((_, sink)) = self.sinks.get_mut(i) {
            if let Some(sink) = sink {
                match poll(sink.as_mut(), cx) {
                    Poll::Pending => poll_result = Poll::Pending,
                    Poll::Ready(Ok(())) => (),
                    Poll::Ready(Err(())) => {
                        self.handle_sink_error(i)?;
                        continue;
                    }
                }
            }
            i += 1;
        }

        poll_result
    }
}

impl Sink<Event> for Fanout {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), ()>> {
        let this = self.get_mut();

        this.process_control_messages(cx);

        while let Some((_, sink)) = this.sinks.get_mut(this.i) {
            match sink {
                Some(sink) => match sink.as_mut().poll_ready(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Ok(())) => this.i += 1,
                    Poll::Ready(Err(())) => this.handle_sink_error(this.i)?,
                },
                // process_control_messages ended because control channel returned
                // Pending so it's fine to return Pending here since the control
                // channel will notify current task when it receives a message.
                None => return Poll::Pending,
            }
        }

        this.i = 0;

        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Event) -> Result<(), ()> {
        let mut items = vec![item; self.sinks.len()];

        let mut i = 1;
        while let Some((_, sink)) = self.sinks.get_mut(i) {
            if let Some(sink) = sink.as_mut() {
                let item = items.pop().unwrap();
                if sink.as_mut().start_send(item).is_err() {
                    self.handle_sink_error(i)?;
                    continue;
                }
            }
            i += 1;
        }

        if let Some((_, sink)) = self.sinks.first_mut() {
            if let Some(sink) = sink.as_mut() {
                let item = items.pop().unwrap();
                if sink.as_mut().start_send(item).is_err() {
                    self.handle_sink_error(0)?;
                }
            }
        }

        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), ()>> {
        self.poll_sinks(cx, |sink, cx| sink.poll_flush(cx))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), ()>> {
        self.poll_sinks(cx, |sink, cx| sink.poll_close(cx))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        mem,
        pin::Pin,
        task::{Context, Poll},
    };

    use futures::{stream, Sink, SinkExt, StreamExt};
    use tokio::sync::mpsc::UnboundedSender;
    use tokio_test::{assert_pending, assert_ready, task::spawn};
    use vector_buffers::{
        topology::{
            builder::TopologyBuilder,
            channel::{BufferReceiver, BufferSender, SenderAdapter},
        },
        WhenFull,
    };

    use super::{ControlMessage, Fanout};
    use crate::{config::ComponentKey, event::Event, test_util::collect_ready};

    async fn build_sender_pair(capacity: usize) -> (BufferSender<Event>, BufferReceiver<Event>) {
        TopologyBuilder::standalone_memory(capacity, WhenFull::Block).await
    }

    async fn build_sender_pairs(
        capacities: &[usize],
    ) -> Vec<(BufferSender<Event>, BufferReceiver<Event>)> {
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
        Vec<BufferReceiver<Event>>,
    ) {
        let (mut fanout, control) = Fanout::new();
        let pairs = build_sender_pairs(capacities).await;

        let mut receivers = Vec::new();
        for (i, (sender, receiver)) in pairs.into_iter().enumerate() {
            fanout.add(ComponentKey::from(i.to_string()), Box::pin(sender));
            receivers.push(receiver);
        }

        (fanout, control, receivers)
    }

    async fn add_sender_to_fanout(
        fanout: &mut Fanout,
        receivers: &mut Vec<BufferReceiver<Event>>,
        sender_id: usize,
        capacity: usize,
    ) {
        let (sender, receiver) = build_sender_pair(capacity).await;
        receivers.push(receiver);

        fanout.add(ComponentKey::from(sender_id.to_string()), Box::pin(sender));
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
        receivers: &mut Vec<BufferReceiver<Event>>,
        sender_id: usize,
        capacity: usize,
    ) -> BufferReceiver<Event> {
        let (sender, receiver) = build_sender_pair(capacity).await;
        let old_receiver = mem::replace(&mut receivers[sender_id], receiver);

        control
            .send(ControlMessage::Replace(
                ComponentKey::from(sender_id.to_string()),
                Some(Box::pin(sender)),
            ))
            .expect("sending control message should not fail");

        old_receiver
    }

    async fn start_sender_replace(
        control: &UnboundedSender<ControlMessage>,
        receivers: &mut Vec<BufferReceiver<Event>>,
        sender_id: usize,
        capacity: usize,
    ) -> (BufferReceiver<Event>, BufferSender<Event>) {
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
        sender: BufferSender<Event>,
    ) {
        control
            .send(ControlMessage::Replace(
                ComponentKey::from(sender_id.to_string()),
                Some(Box::pin(sender)),
            ))
            .expect("sending control message should not fail");
    }

    #[tokio::test]
    async fn fanout_writes_to_all() {
        let (fanout, _, receivers) = fanout_from_senders(&[2, 2]).await;
        let events = make_events(2);

        stream::iter(events.clone())
            .map(Ok)
            .forward(fanout)
            .await
            .expect("forward should not fail");

        for receiver in receivers {
            assert_eq!(collect_ready(receiver), events);
        }
    }

    #[tokio::test]
    async fn fanout_notready() {
        let (mut fanout, _, mut receivers) = fanout_from_senders(&[2, 1, 2]).await;
        let events = make_events(2);

        // First send should immediately complete because all senders have capacity:
        let mut first_send = spawn(fanout.send(events[0].clone()));
        let first_send_result = assert_ready!(first_send.poll());
        assert!(first_send_result.is_ok());
        drop(first_send);

        // Second send should return pending because sender B is now full:
        let mut second_send = spawn(fanout.send(events[1].clone()));
        assert_pending!(second_send.poll());

        // Now read an item from each receiver to free up capacity for the second sender:
        for receiver in &mut receivers {
            assert_eq!(Some(events[0].clone()), receiver.next().await);
        }

        // Now our second send should actually be able to complete:
        let second_send_result = assert_ready!(second_send.poll());
        assert!(second_send_result.is_ok());
        drop(second_send);

        // And make sure the second item comes through:
        for receiver in &mut receivers {
            assert_eq!(Some(events[1].clone()), receiver.next().await);
        }
    }

    #[tokio::test]
    async fn fanout_grow() {
        let (mut fanout, _, mut receivers) = fanout_from_senders(&[4, 4]).await;
        let events = make_events(3);

        // Send in the first two events to our initial two senders:
        fanout
            .send(events[0].clone())
            .await
            .expect("send should not fail");
        fanout
            .send(events[1].clone())
            .await
            .expect("send should not fail");

        // Now add a third sender:
        add_sender_to_fanout(&mut fanout, &mut receivers, 2, 4).await;

        // Send in the last event which all three senders will now get:
        fanout
            .send(events[2].clone())
            .await
            .expect("send should not fail");

        // Make sure the first two senders got all three events, but the third sender only got the
        // last event:
        let expected_events = [&events, &events, &events[2..]];
        for (i, receiver) in receivers.iter_mut().enumerate() {
            assert_eq!(collect_ready(receiver), expected_events[i]);
        }
    }

    #[tokio::test]
    async fn fanout_shrink() {
        let (mut fanout, control, mut receivers) = fanout_from_senders(&[4, 4]).await;
        let events = make_events(3);

        // Send in the first two events to our initial two senders:
        fanout
            .send(events[0].clone())
            .await
            .expect("send should not fail");
        fanout
            .send(events[1].clone())
            .await
            .expect("send should not fail");

        // Now remove the second sender:
        remove_sender_from_fanout(&control, 1);

        // Send in the last event which only the first sender will get:
        fanout
            .send(events[2].clone())
            .await
            .expect("send should not fail");

        // Make sure the first sender got all three events, but the second sender only got the first two:
        let expected_events = [&events, &events[..2]];
        for (i, receiver) in receivers.iter_mut().enumerate() {
            assert_eq!(collect_ready(receiver), expected_events[i]);
        }
    }

    #[tokio::test]
    async fn fanout_shrink_when_notready() {
        // This test exercises that when we're waiting for all sinks to become ready for a send
        // before actually doing it, we can still correctly remove a sender that was already ready, or
        // a sender which itself was the cause of not yet being ready, or a sender which has not yet
        // been polled for readiness.
        for sender_id in [0, 1, 2] {
            let (mut fanout, control, mut receivers) = fanout_from_senders(&[2, 1, 2]).await;
            let events = make_events(2);

            // First send should immediately complete because all senders have capacity:
            let mut first_send = spawn(fanout.send(events[0].clone()));
            let first_send_result = assert_ready!(first_send.poll());
            assert!(first_send_result.is_ok());
            drop(first_send);

            // Second send should return pending because sender B is now full:
            let mut second_send = spawn(fanout.send(events[1].clone()));
            assert_pending!(second_send.poll());

            // Now read an item from each receiver to free up capacity:
            for receiver in &mut receivers {
                assert_eq!(Some(events[0].clone()), receiver.next().await);
            }

            // Drop the given sender before polling again:
            remove_sender_from_fanout(&control, sender_id);

            // Now our second send should actually be able to complete.  We'll assert that whichever
            // sender we removed does not get the next event:
            let second_send_result = assert_ready!(second_send.poll());
            assert!(second_send_result.is_ok());
            drop(second_send);

            let mut expected_next = [
                Some(events[1].clone()),
                Some(events[1].clone()),
                Some(events[1].clone()),
            ];
            expected_next[sender_id] = None;

            for (i, receiver) in receivers.iter_mut().enumerate() {
                assert_eq!(expected_next[i], receiver.next().await);
            }
        }
    }

    #[tokio::test]
    async fn fanout_no_sinks() {
        let (mut fanout, _) = Fanout::new();
        let events = make_events(2);

        fanout
            .send(events[0].clone())
            .await
            .expect("send should not fail");
        fanout
            .send(events[1].clone())
            .await
            .expect("send should not fail");
    }

    #[tokio::test]
    async fn fanout_replace() {
        let (mut fanout, control, mut receivers) = fanout_from_senders(&[4, 4, 4]).await;
        let events = make_events(3);

        // First two sends should immediately complete because all senders have capacity:
        fanout
            .send(events[0].clone())
            .await
            .expect("send should not fail");
        fanout
            .send(events[1].clone())
            .await
            .expect("send should not fail");

        // Replace the first sender with a brand new one before polling again:
        let old_first_receiver = replace_sender_in_fanout(&control, &mut receivers, 0, 4).await;

        // And do the third send which should also complete since all senders still have capacity:
        fanout
            .send(events[2].clone())
            .await
            .expect("send should not fail");

        // Now make sure that the new "first" sender only got the third event, but that the second and
        // third sender got all three events:
        let expected_events = [&events[2..], &events, &events];
        for (i, receiver) in receivers.iter_mut().enumerate() {
            assert_eq!(collect_ready(receiver), expected_events[i]);
        }

        // And make sure our original "first" sender got the first two events:
        assert_eq!(collect_ready(old_first_receiver), &events[..2]);
    }

    #[tokio::test]
    async fn fanout_wait() {
        let (mut fanout, control, mut receivers) = fanout_from_senders(&[4, 4]).await;
        let events = make_events(3);

        // First two sends should immediately complete because all senders have capacity:
        fanout
            .send(events[0].clone())
            .await
            .expect("send should not fail");
        fanout
            .send(events[1].clone())
            .await
            .expect("send should not fail");

        // Now do an empty replace on the second sender, which we'll test to make sure that `Fanout`
        // doesn't let any writes through until we replace it properly.  We get back the receiver
        // we've replaced, but also the sender that we want to eventually install:
        let (old_first_receiver, new_first_sender) =
            start_sender_replace(&control, &mut receivers, 0, 4).await;

        // Third send should return pending because now we have an in-flight replacement:
        let mut third_send = spawn(fanout.send(events[2].clone()));
        assert_pending!(third_send.poll());

        // Finish our sender replacement, which should wake up the third send and allow it to
        // actually complete:
        finish_sender_replace(&control, 0, new_first_sender);
        assert!(third_send.is_woken());
        let third_send_result = assert_ready!(third_send.poll());
        assert!(third_send_result.is_ok());

        // Make sure the original first sender got the first two events, the new first sender got
        // the last event, and the second sender got all three:
        let expected_events = [&events[2..], &events];
        for (i, receiver) in receivers.iter_mut().enumerate() {
            assert_eq!(collect_ready(receiver), expected_events[i]);
        }

        assert_eq!(collect_ready(old_first_receiver), &events[..2]);
    }

    #[tokio::test]
    async fn fanout_error_poll_first() {
        fanout_error(&[Some(ErrorWhen::Poll), None, None]).await;
    }

    #[tokio::test]
    async fn fanout_error_poll_middle() {
        fanout_error(&[None, Some(ErrorWhen::Poll), None]).await;
    }

    #[tokio::test]
    async fn fanout_error_poll_last() {
        fanout_error(&[None, None, Some(ErrorWhen::Poll)]).await;
    }

    #[tokio::test]
    async fn fanout_error_poll_not_middle() {
        fanout_error(&[Some(ErrorWhen::Poll), None, Some(ErrorWhen::Poll)]).await;
    }

    #[tokio::test]
    async fn fanout_error_send_first() {
        fanout_error(&[Some(ErrorWhen::Send), None, None]).await;
    }

    #[tokio::test]
    async fn fanout_error_send_middle() {
        fanout_error(&[None, Some(ErrorWhen::Send), None]).await;
    }

    #[tokio::test]
    async fn fanout_error_send_last() {
        fanout_error(&[None, None, Some(ErrorWhen::Send)]).await;
    }

    #[tokio::test]
    async fn fanout_error_send_not_middle() {
        fanout_error(&[Some(ErrorWhen::Send), None, Some(ErrorWhen::Send)]).await;
    }

    async fn fanout_error(modes: &[Option<ErrorWhen>]) {
        let (mut fanout, _) = Fanout::new();
        let mut receivers = Vec::new();

        for (i, mode) in modes.iter().enumerate() {
            let id = ComponentKey::from(format!("{}", i));
            let tx = if let Some(when) = *mode {
                let tx = AlwaysErrors { when };
                let tx = SenderAdapter::opaque(tx.sink_map_err(|_| ()));
                BufferSender::new(tx, WhenFull::Block)
            } else {
                let (tx, rx) = TopologyBuilder::standalone_memory(1, WhenFull::Block).await;
                receivers.push(rx);
                tx
            };
            fanout.add(id, Box::pin(tx));
        }

        // Spawn a task to send the events into the `Fanout`.  We spawn a task so that we can await
        // the receivers while the forward task drives itself to completion:
        let events = make_events(3);
        let send = stream::iter(events.clone()).map(Ok).forward(fanout);
        tokio::spawn(send);

        // Wait for all of our receivers for non-erroring-senders to complete, and make sure they
        // got all of the events we sent in.  We also spawn these as tasks so they can empty
        // themselves and allow more events in, since we have to drive them all or we might get
        // stuck receiving everything from one while the others need to be drained to make progress:
        let collectors = receivers
            .into_iter()
            .map(|rx| tokio::spawn(rx.collect::<Vec<_>>()))
            .collect::<Vec<_>>();

        for collector in collectors {
            assert_eq!(collector.await.unwrap(), events);
        }
    }

    #[derive(Clone, Copy)]
    enum ErrorWhen {
        Send,
        Poll,
    }

    #[derive(Clone)]
    struct AlwaysErrors {
        when: ErrorWhen,
    }

    impl Sink<Event> for AlwaysErrors {
        type Error = crate::Error;

        fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(match self.when {
                ErrorWhen::Poll => Err("Something failed".into()),
                _ => Ok(()),
            })
        }

        fn start_send(self: Pin<&mut Self>, _: Event) -> Result<(), Self::Error> {
            match self.when {
                ErrorWhen::Poll => Err("Something failed".into()),
                _ => Ok(()),
            }
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(match self.when {
                ErrorWhen::Poll => Err("Something failed".into()),
                _ => Ok(()),
            })
        }

        fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(match self.when {
                ErrorWhen::Poll => Err("Something failed".into()),
                _ => Ok(()),
            })
        }
    }

    fn make_events(count: usize) -> Vec<Event> {
        (0..count)
            .map(|i| Event::from(format!("line {}", i)))
            .collect()
    }
}
