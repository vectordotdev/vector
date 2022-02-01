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

struct Store<K, V> {
    index: usize,
    inner: Vec<(K, V)>,
}

impl<K, V> Store<K, V>
where
    K: PartialEq,
{
    fn with_capacity(capacity: usize) -> Self {
        Self {
            index: 0,
            inner: Vec::with_capacity(capacity),
        }
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    /// # Panics
    ///
    /// Function will panic if the key is duplicate
    fn push(&mut self, key: K, val: V) {
        assert!(
            !self.inner.iter().any(|(k, _)| k == &key),
            "Duplicate output id in fanout"
        );

        self.inner.push((key, val));
    }

    /// Remove an stored element by its key
    ///
    /// This function will remove an element from the store by its key.
    ///
    /// This function is O(n).
    ///
    /// # Panics
    ///
    /// Function will panic if the inner cursor is in any state but initial.
    fn remove_by_key(&mut self, key: &K) -> Option<V> {
        assert!(
            self.index == 0,
            "Cursor is not in its initial state, cannot remove by key safely"
        );

        match self.inner.iter().position(|(k, _)| k == key) {
            Some(idx) => Some(self.inner.swap_remove(idx).1),
            None => None,
        }
    }

    /// Removes the element at the current cursor
    ///
    /// This function will remove the element at the current cursor
    /// position. The element at the end of the inner store will be swaped to
    /// the current cursor position and, so, the cursor SHOULD NOT be advanced
    /// after calling `remove`. Doing so may cause elements to be skipped by
    /// calls to `get_mut`.
    ///
    /// This function is O(1).
    ///
    fn remove(&mut self) {
        self.inner.swap_remove(self.index);
    }

    fn replace(&mut self, key: &K, val: V) {
        // NOTE if we added K: Ord we could use a binary search, although pushes
        // would no longer be O(1). This function should be called rarely in
        // practice so it kinda doesn't matter.
        if let Some((_, existing)) = self.inner.iter_mut().find(|(k, _)| k == key) {
            *existing = val;
        } else {
            panic!("Tried to replace a sink that's not already present");
        }
    }

    /// Return a mutable reference to the current cursor, None if we've gone
    /// past the end
    fn get_mut(&mut self) -> Option<&mut V> {
        self.inner.get_mut(self.index).map(|(_, v)| v)
    }

    /// Advance the internal cursor
    ///
    /// Returns None if the inner store is empty or we've wrapped around to the
    /// start of the store again.
    fn advance(&mut self) {
        self.index += 1;
    }

    /// Reset the internal cursor
    fn reset_cursor(&mut self) {
        self.index = 0;
    }
}

pub struct Fanout {
    sinks: Store<ComponentKey, Option<GenericEventSink>>,
    control_channel: mpsc::UnboundedReceiver<ControlMessage>,
}

impl Fanout {
    pub fn new() -> (Self, ControlChannel) {
        let (control_tx, control_rx) = mpsc::unbounded_channel();

        let fanout = Self {
            sinks: Store::with_capacity(16), // arbitrary smallish value
            control_channel: control_rx,
        };

        (fanout, control_tx)
    }

    /// Add a new sink as an output.
    ///
    /// # Panics
    ///
    /// Function will panic if a sink with the same ID is already present.
    fn add(&mut self, id: ComponentKey, sink: GenericEventSink) {
        self.sinks.push(id, Some(sink));
    }

    fn remove(&mut self, id: &ComponentKey) {
        let removed = self
            .sinks
            .remove_by_key(id)
            .expect("Didn't find output in fanout");

        if let Some(mut removed) = removed {
            tokio::spawn(async move { removed.close().await });
        }
    }

    fn replace(&mut self, id: &ComponentKey, sink: Option<GenericEventSink>) {
        self.sinks.replace(id, sink);
    }

    fn process_control_messages(&mut self, cx: &mut Context<'_>) {
        while let Poll::Ready(Some(message)) = self.control_channel.poll_recv(cx) {
            match message {
                ControlMessage::Add(id, sink) => self.add(id, sink),
                ControlMessage::Remove(id) => self.remove(&id),
                ControlMessage::Replace(id, sink) => self.replace(&id, sink),
            }
        }
    }

    #[inline]
    /// Handles an errored sink by removing it from fanout rotation.
    ///
    /// # Errors
    ///
    /// This function will error-out if the underlying sink store has only a
    /// single remaining member.
    fn handle_sink_error(&mut self) -> Result<(), ()> {
        // If there's only one sink, propagate the error to the source ASAP
        // so it stops reading from its input. If there are multiple sinks,
        // keep pushing to the non-errored ones (while the errored sink
        // triggers a more graceful shutdown).
        if self.sinks.len() == 1 {
            Err(())
        } else {
            self.sinks.remove();
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
        self.sinks.reset_cursor();
        self.process_control_messages(cx);

        while let Some(sink) = self.sinks.get_mut() {
            if let Some(sink) = sink {
                match poll(sink.as_mut(), cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Ok(())) => self.sinks.advance(),
                    Poll::Ready(Err(())) => self.handle_sink_error()?,
                }
            }
            // TODO it's not clear to me why we wouldn't return Pending if the
            // value is None, like we do in `poll_ready`.
        }

        Poll::Ready(Ok(()))
    }
}

impl Sink<Event> for Fanout {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), ()>> {
        let this = self.get_mut();

        this.sinks.reset_cursor();
        this.process_control_messages(cx);

        while let Some(sink) = this.sinks.get_mut() {
            match sink {
                Some(sink) => match sink.as_mut().poll_ready(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Ok(())) => this.sinks.advance(),
                    Poll::Ready(Err(())) => this.handle_sink_error()?,
                },
                // process_control_messages ended because control channel returned
                // Pending so it's fine to return Pending here since the control
                // channel will notify current task when it receives a message.
                None => return Poll::Pending,
            }
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Event) -> Result<(), ()> {
        let mut items = vec![item; self.sinks.len()];

        self.sinks.reset_cursor();
        while let Some(sink) = self.sinks.get_mut() {
            if let Some(sink) = sink.as_mut() {
                let item = items.pop().unwrap();
                if sink.as_mut().start_send(item).is_err() {
                    self.handle_sink_error()?;
                    continue;
                }
            }
            self.sinks.advance();
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
        pin::Pin,
        task::{Context, Poll},
    };

    use futures::{stream, FutureExt, Sink, SinkExt, StreamExt};
    use tokio::time::{sleep, Duration};
    use vector_buffers::{
        topology::{
            builder::TopologyBuilder,
            channel::{BufferSender, SenderAdapter},
        },
        WhenFull,
    };

    use super::{ControlMessage, Fanout};
    use crate::{config::ComponentKey, event::Event, test_util::collect_ready};

    #[tokio::test]
    async fn fanout_writes_to_all() {
        let (tx_a, rx_a) = TopologyBuilder::memory(4, WhenFull::Block).await;
        let (tx_b, rx_b) = TopologyBuilder::memory(4, WhenFull::Block).await;

        let (mut fanout, _fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), Box::pin(tx_a));
        fanout.add(ComponentKey::from("b"), Box::pin(tx_b));

        let recs = make_events(2);
        let send = stream::iter(recs.clone()).map(Ok).forward(fanout);
        send.await.unwrap();

        assert_eq!(collect_ready(rx_a), recs);
        assert_eq!(collect_ready(rx_b), recs);
    }

    #[tokio::test]
    async fn fanout_notready() {
        let (tx_a, rx_a) = TopologyBuilder::memory(1, WhenFull::Block).await;
        let (tx_b, rx_b) = TopologyBuilder::memory(0, WhenFull::Block).await;
        let (tx_c, rx_c) = TopologyBuilder::memory(1, WhenFull::Block).await;

        let (mut fanout, _fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), Box::pin(tx_a));
        fanout.add(ComponentKey::from("b"), Box::pin(tx_b));
        fanout.add(ComponentKey::from("c"), Box::pin(tx_c));

        let recs = make_events(3);
        let send = stream::iter(recs.clone()).map(Ok).forward(fanout);
        tokio::spawn(send);

        sleep(Duration::from_millis(50)).await;
        // The send_all task will be blocked on sending rec1 because of b right now.

        let collect_a = tokio::spawn(rx_a.collect::<Vec<_>>());
        let collect_b = tokio::spawn(rx_b.collect::<Vec<_>>());
        let collect_c = tokio::spawn(rx_c.collect::<Vec<_>>());

        assert_eq!(collect_a.await.unwrap(), recs);
        assert_eq!(collect_b.await.unwrap(), recs);
        assert_eq!(collect_c.await.unwrap(), recs);
    }

    #[tokio::test]
    async fn fanout_grow() {
        let (tx_a, rx_a) = TopologyBuilder::memory(4, WhenFull::Block).await;
        let (tx_b, rx_b) = TopologyBuilder::memory(4, WhenFull::Block).await;

        let (mut fanout, _fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), Box::pin(tx_a));
        fanout.add(ComponentKey::from("b"), Box::pin(tx_b));

        let recs = make_events(3);

        fanout.send(recs[0].clone()).await.unwrap();
        fanout.send(recs[1].clone()).await.unwrap();

        let (tx_c, rx_c) = TopologyBuilder::memory(4, WhenFull::Block).await;
        fanout.add(ComponentKey::from("c"), Box::pin(tx_c));

        fanout.send(recs[2].clone()).await.unwrap();

        assert_eq!(collect_ready(rx_a), recs);
        assert_eq!(collect_ready(rx_b), recs);
        assert_eq!(collect_ready(rx_c), &recs[2..]);
    }

    #[tokio::test]
    async fn fanout_shrink() {
        let (tx_a, rx_a) = TopologyBuilder::memory(4, WhenFull::Block).await;
        let (tx_b, rx_b) = TopologyBuilder::memory(4, WhenFull::Block).await;

        let (mut fanout, fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), Box::pin(tx_a));
        fanout.add(ComponentKey::from("b"), Box::pin(tx_b));

        let recs = make_events(3);

        fanout.send(recs[0].clone()).await.unwrap();
        fanout.send(recs[1].clone()).await.unwrap();

        fanout_control
            .send(ControlMessage::Remove(ComponentKey::from("b")))
            .unwrap();

        fanout.send(recs[2].clone()).await.unwrap();

        assert_eq!(collect_ready(rx_a), recs);
        assert_eq!(collect_ready(rx_b), &recs[..2]);
    }

    #[tokio::test]
    async fn fanout_shrink_after_notready() {
        let (tx_a, rx_a) = TopologyBuilder::memory(1, WhenFull::Block).await;
        let (tx_b, rx_b) = TopologyBuilder::memory(0, WhenFull::Block).await;
        let (tx_c, rx_c) = TopologyBuilder::memory(1, WhenFull::Block).await;

        let (mut fanout, fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), Box::pin(tx_a));
        fanout.add(ComponentKey::from("b"), Box::pin(tx_b));
        fanout.add(ComponentKey::from("c"), Box::pin(tx_c));

        let recs = make_events(3);
        let send = stream::iter(recs.clone()).map(Ok).forward(fanout);
        tokio::spawn(send);

        sleep(Duration::from_millis(50)).await;
        // The send_all task will be blocked on sending rec1 because of b right now.
        fanout_control
            .send(ControlMessage::Remove(ComponentKey::from("c")))
            .unwrap();

        let collect_a = tokio::spawn(rx_a.collect::<Vec<_>>());
        let collect_b = tokio::spawn(rx_b.collect::<Vec<_>>());
        let collect_c = tokio::spawn(rx_c.collect::<Vec<_>>());

        assert_eq!(collect_a.await.unwrap(), recs);
        assert_eq!(collect_b.await.unwrap(), recs);
        assert_eq!(collect_c.await.unwrap(), &recs[..1]);
    }

    #[tokio::test]
    async fn fanout_shrink_at_notready() {
        let (tx_a, rx_a) = TopologyBuilder::memory(1, WhenFull::Block).await;
        let (tx_b, rx_b) = TopologyBuilder::memory(0, WhenFull::Block).await;
        let (tx_c, rx_c) = TopologyBuilder::memory(1, WhenFull::Block).await;

        let (mut fanout, fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), Box::pin(tx_a));
        fanout.add(ComponentKey::from("b"), Box::pin(tx_b));
        fanout.add(ComponentKey::from("c"), Box::pin(tx_c));

        let recs = make_events(3);
        let send = stream::iter(recs.clone()).map(Ok).forward(fanout);
        tokio::spawn(send);

        sleep(Duration::from_millis(50)).await;
        // The send_all task will be blocked on sending rec1 because of b right now.
        fanout_control
            .send(ControlMessage::Remove(ComponentKey::from("b")))
            .unwrap();

        let collect_a = tokio::spawn(rx_a.collect::<Vec<_>>());
        let collect_b = tokio::spawn(rx_b.collect::<Vec<_>>());
        let collect_c = tokio::spawn(rx_c.collect::<Vec<_>>());

        assert_eq!(collect_a.await.unwrap(), recs);
        assert_eq!(collect_b.await.unwrap(), &recs[..1]);
        assert_eq!(collect_c.await.unwrap(), recs);
    }

    #[tokio::test]
    async fn fanout_shrink_before_notready() {
        let (tx_a, rx_a) = TopologyBuilder::memory(1, WhenFull::Block).await;
        let (tx_b, rx_b) = TopologyBuilder::memory(0, WhenFull::Block).await;
        let (tx_c, rx_c) = TopologyBuilder::memory(1, WhenFull::Block).await;

        let (mut fanout, fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), Box::pin(tx_a));
        fanout.add(ComponentKey::from("b"), Box::pin(tx_b));
        fanout.add(ComponentKey::from("c"), Box::pin(tx_c));

        let recs = make_events(3);
        let send = stream::iter(recs.clone()).map(Ok).forward(fanout);
        tokio::spawn(send);

        sleep(Duration::from_millis(50)).await;
        // The send_all task will be blocked on sending rec1 because of b right now.

        fanout_control
            .send(ControlMessage::Remove(ComponentKey::from("a")))
            .unwrap();

        let collect_a = tokio::spawn(rx_a.collect::<Vec<_>>());
        let collect_b = tokio::spawn(rx_b.collect::<Vec<_>>());
        let collect_c = tokio::spawn(rx_c.collect::<Vec<_>>());

        assert_eq!(collect_a.await.unwrap(), &recs[..1]);
        assert_eq!(collect_b.await.unwrap(), recs);
        assert_eq!(collect_c.await.unwrap(), recs);
    }

    #[tokio::test]
    async fn fanout_no_sinks() {
        let (mut fanout, _fanout_control) = Fanout::new();

        let recs = make_events(2);

        fanout.send(recs[0].clone()).await.unwrap();
        fanout.send(recs[1].clone()).await.unwrap();
    }

    #[tokio::test]
    async fn fanout_replace() {
        let (tx_a1, rx_a1) = TopologyBuilder::memory(4, WhenFull::Block).await;
        let (tx_b, rx_b) = TopologyBuilder::memory(4, WhenFull::Block).await;

        let (mut fanout, _fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), Box::pin(tx_a1));
        fanout.add(ComponentKey::from("b"), Box::pin(tx_b));

        let recs = make_events(3);

        fanout.send(recs[0].clone()).await.unwrap();
        fanout.send(recs[1].clone()).await.unwrap();

        let (tx_a2, rx_a2) = TopologyBuilder::memory(4, WhenFull::Block).await;
        fanout.replace(&ComponentKey::from("a"), Some(Box::pin(tx_a2)));

        fanout.send(recs[2].clone()).await.unwrap();

        assert_eq!(collect_ready(rx_a1), &recs[..2]);
        assert_eq!(collect_ready(rx_b), recs);
        assert_eq!(collect_ready(rx_a2), &recs[2..]);
    }

    #[tokio::test]
    async fn fanout_wait() {
        let (tx_a1, rx_a1) = TopologyBuilder::memory(4, WhenFull::Block).await;
        let (tx_b, rx_b) = TopologyBuilder::memory(4, WhenFull::Block).await;

        let (mut fanout, fanout_control) = Fanout::new();

        fanout.add(ComponentKey::from("a"), Box::pin(tx_a1));
        fanout.add(ComponentKey::from("b"), Box::pin(tx_b));

        let recs = make_events(3);

        fanout.send(recs[0].clone()).await.unwrap();
        fanout.send(recs[1].clone()).await.unwrap();

        let (tx_a2, rx_a2) = TopologyBuilder::memory(4, WhenFull::Block).await;
        fanout.replace(&ComponentKey::from("a"), None);

        futures::join!(
            async {
                sleep(Duration::from_millis(100)).await;
                fanout_control
                    .send(ControlMessage::Replace(
                        ComponentKey::from("a"),
                        Some(Box::pin(tx_a2)),
                    ))
                    .unwrap();
            },
            fanout.send(recs[2].clone()).map(|_| ())
        );

        assert_eq!(collect_ready(rx_a1), &recs[..2]);
        assert_eq!(collect_ready(rx_b), recs);
        assert_eq!(collect_ready(rx_a2), &recs[2..]);
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
        let (mut fanout, _fanout_control) = Fanout::new();
        let mut rx_channels = vec![];

        for (i, mode) in modes.iter().enumerate() {
            let id = ComponentKey::from(format!("{}", i));
            if let Some(when) = *mode {
                let tx = AlwaysErrors { when };
                let tx = SenderAdapter::opaque(tx.sink_map_err(|_| ()));
                let tx = BufferSender::new(tx, WhenFull::Block);

                fanout.add(id, Box::pin(tx));
            } else {
                let (tx, rx) = TopologyBuilder::memory(0, WhenFull::Block).await;
                fanout.add(id, Box::pin(tx));
                rx_channels.push(rx);
            }
        }

        let recs = make_events(3);
        let send = stream::iter(recs.clone()).map(Ok).forward(fanout);
        tokio::spawn(send);

        sleep(Duration::from_millis(50)).await;

        // Start collecting from all at once
        let collectors = rx_channels
            .into_iter()
            .map(|rx| tokio::spawn(rx.collect::<Vec<_>>()))
            .collect::<Vec<_>>();

        for collect in collectors {
            assert_eq!(collect.await.unwrap(), recs);
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
