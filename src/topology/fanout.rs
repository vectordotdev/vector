use crate::sinks::RouterSink;
use crate::Event;
use futures::compat::Future01CompatExt;
use futures01::sync::mpsc;
use futures01::{future, Async, AsyncSink, Poll, Sink, StartSend, Stream};

pub struct Fanout {
    sinks: Vec<(String, RouterSink)>,
    i: usize,
    control_channel: mpsc::UnboundedReceiver<ControlMessage>,
}

pub enum ControlMessage {
    Add(String, RouterSink),
    Remove(String),
    Replace(String, RouterSink),
}

pub type ControlChannel = mpsc::UnboundedSender<ControlMessage>;

impl Fanout {
    pub fn new() -> (Self, ControlChannel) {
        let (control_tx, control_rx) = mpsc::unbounded();

        let fanout = Self {
            sinks: vec![],
            i: 0,
            control_channel: control_rx,
        };

        (fanout, control_tx)
    }

    pub fn add(&mut self, name: String, sink: RouterSink) {
        assert!(
            !self.sinks.iter().any(|(n, _)| n == &name),
            "Duplicate output name in fanout"
        );

        self.sinks.push((name, sink));
    }

    fn remove(&mut self, name: &str) {
        let i = self.sinks.iter().position(|(n, _)| n == name);
        let i = i.expect("Didn't find output in fanout");

        let (_name, mut removed) = self.sinks.remove(i);

        tokio::spawn(future::poll_fn(move || removed.close()).compat());

        if self.i > i {
            self.i -= 1;
        }
    }

    fn replace(&mut self, name: String, sink: RouterSink) {
        if let Some((_, existing)) = self.sinks.iter_mut().find(|(n, _)| n == &name) {
            *existing = sink
        } else {
            panic!("Tried to replace a sink that's not already present");
        }
    }

    pub fn process_control_messages(&mut self) {
        while let Ok(Async::Ready(Some(message))) = self.control_channel.poll() {
            match message {
                ControlMessage::Add(name, sink) => self.add(name, sink),
                ControlMessage::Remove(name) => self.remove(&name),
                ControlMessage::Replace(name, sink) => self.replace(name, sink),
            }
        }
    }

    fn handle_sink_error(&mut self) -> Result<(), ()> {
        // If there's only one sink, propagate the error to the source ASAP
        // so it stops reading from its input. If there are multiple sinks,
        // keep pushing to the non-errored ones (while the errored sink
        // triggers a more graceful shutdown).
        if self.sinks.len() == 1 {
            Err(())
        } else {
            self.sinks.remove(self.i);
            Ok(())
        }
    }
}

impl Sink for Fanout {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.process_control_messages();

        if self.sinks.is_empty() {
            return Ok(AsyncSink::Ready);
        }

        while self.i < self.sinks.len() - 1 {
            let (_name, sink) = &mut self.sinks[self.i];
            match sink.start_send(item.clone()) {
                Ok(AsyncSink::NotReady(item)) => return Ok(AsyncSink::NotReady(item)),
                Ok(AsyncSink::Ready) => self.i += 1,
                Err(()) => self.handle_sink_error()?,
            }
        }

        let (_name, sink) = &mut self.sinks[self.i];
        match sink.start_send(item) {
            Ok(AsyncSink::NotReady(item)) => return Ok(AsyncSink::NotReady(item)),
            Ok(AsyncSink::Ready) => self.i += 1,
            Err(()) => self.handle_sink_error()?,
        }

        self.i = 0;

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.process_control_messages();

        let mut all_complete = true;

        for i in 0..self.sinks.len() {
            let (_name, sink) = &mut self.sinks[i];
            match sink.poll_complete() {
                Ok(Async::Ready(())) => {}
                Ok(Async::NotReady) => {
                    all_complete = false;
                }
                Err(()) => self.handle_sink_error()?,
            }
        }

        if all_complete {
            Ok(Async::Ready(()))
        } else {
            Ok(Async::NotReady)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ControlMessage, Fanout};
    use crate::{test_util::collect_ready, Event};
    use futures::compat::Future01CompatExt;
    use futures01::{stream, sync::mpsc, Future, Sink, Stream};
    use tokio::time::{delay_for, Duration};

    #[tokio::test]
    async fn fanout_writes_to_all() {
        let (tx_a, rx_a) = mpsc::unbounded();
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::unbounded();
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));

        let mut fanout = Fanout::new().0;

        fanout.add("a".to_string(), tx_a);
        fanout.add("b".to_string(), tx_b);

        let rec1 = Event::from("line 1".to_string());
        let rec2 = Event::from("line 2".to_string());

        let fanout = fanout.send(rec1.clone()).compat().await.unwrap();
        let _fanout = fanout.send(rec2.clone()).compat().await.unwrap();

        assert_eq!(
            collect_ready(rx_a).await.unwrap(),
            vec![rec1.clone(), rec2.clone()]
        );
        assert_eq!(collect_ready(rx_b).await.unwrap(), vec![rec1, rec2]);
    }

    #[tokio::test]
    async fn fanout_notready() {
        let (tx_a, rx_a) = mpsc::channel(1);
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::channel(0);
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));
        let (tx_c, rx_c) = mpsc::channel(1);
        let tx_c = Box::new(tx_c.sink_map_err(|_| unreachable!()));

        let mut fanout = Fanout::new().0;

        fanout.add("a".to_string(), tx_a);
        fanout.add("b".to_string(), tx_b);
        fanout.add("c".to_string(), tx_c);

        let rec1 = Event::from("line 1".to_string());
        let rec2 = Event::from("line 2".to_string());
        let rec3 = Event::from("line 3".to_string());

        let recs = vec![rec1, rec2, rec3];
        let send = fanout.send_all(stream::iter_ok(recs.clone()));
        tokio::spawn(send.map(|_| ()).compat());

        delay_for(Duration::from_millis(50)).await;
        // The send_all task will be blocked on sending rec2 to b right now.

        let collect_a = tokio::spawn(rx_a.collect().compat());
        let collect_b = tokio::spawn(rx_b.collect().compat());
        let collect_c = tokio::spawn(rx_c.collect().compat());

        assert_eq!(collect_a.await.unwrap().unwrap(), recs);
        assert_eq!(collect_b.await.unwrap().unwrap(), recs);
        assert_eq!(collect_c.await.unwrap().unwrap(), recs);
    }

    #[tokio::test]
    async fn fanout_grow() {
        let (tx_a, rx_a) = mpsc::unbounded();
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::unbounded();
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));

        let mut fanout = Fanout::new().0;

        fanout.add("a".to_string(), tx_a);
        fanout.add("b".to_string(), tx_b);

        let rec1 = Event::from("line 1".to_string());
        let rec2 = Event::from("line 2".to_string());

        let fanout = fanout.send(rec1.clone()).compat().await.unwrap();
        let mut fanout = fanout.send(rec2.clone()).compat().await.unwrap();

        let (tx_c, rx_c) = mpsc::unbounded();
        let tx_c = Box::new(tx_c.sink_map_err(|_| unreachable!()));
        fanout.add("c".to_string(), tx_c);

        let rec3 = Event::from("line 3".to_string());
        let _fanout = fanout.send(rec3.clone()).compat().await.unwrap();

        assert_eq!(
            collect_ready(rx_a).await.unwrap(),
            vec![rec1.clone(), rec2.clone(), rec3.clone()]
        );
        assert_eq!(
            collect_ready(rx_b).await.unwrap(),
            vec![rec1, rec2, rec3.clone()]
        );
        assert_eq!(collect_ready(rx_c).await.unwrap(), vec![rec3]);
    }

    #[tokio::test]
    async fn fanout_shrink() {
        let (tx_a, rx_a) = mpsc::unbounded();
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::unbounded();
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));

        let (mut fanout, fanout_control) = Fanout::new();

        fanout.add("a".to_string(), tx_a);
        fanout.add("b".to_string(), tx_b);

        let rec1 = Event::from("line 1".to_string());
        let rec2 = Event::from("line 2".to_string());

        let fanout = fanout.send(rec1.clone()).compat().await.unwrap();
        let fanout = fanout.send(rec2.clone()).compat().await.unwrap();

        fanout_control
            .unbounded_send(ControlMessage::Remove("b".to_string()))
            .unwrap();

        let rec3 = Event::from("line 3".to_string());
        use futures::compat::Future01CompatExt;
        let _fanout = fanout.send(rec3.clone()).compat().await.unwrap();

        assert_eq!(
            collect_ready(rx_a).await.unwrap(),
            vec![rec1.clone(), rec2.clone(), rec3]
        );
        assert_eq!(collect_ready(rx_b).await.unwrap(), vec![rec1, rec2]);
    }

    #[tokio::test]
    async fn fanout_shrink_after_notready() {
        let (tx_a, rx_a) = mpsc::channel(1);
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::channel(0);
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));
        let (tx_c, rx_c) = mpsc::channel(1);
        let tx_c = Box::new(tx_c.sink_map_err(|_| unreachable!()));

        let (mut fanout, fanout_control) = Fanout::new();

        fanout.add("a".to_string(), tx_a);
        fanout.add("b".to_string(), tx_b);
        fanout.add("c".to_string(), tx_c);

        let rec1 = Event::from("line 1".to_string());
        let rec2 = Event::from("line 2".to_string());
        let rec3 = Event::from("line 3".to_string());

        let recs = vec![rec1.clone(), rec2, rec3];
        let send = fanout.send_all(stream::iter_ok(recs.clone()));
        tokio::spawn(send.map(|_| ()).compat());

        delay_for(Duration::from_millis(50)).await;
        // The send_all task will be blocked on sending rec2 to b right now.
        fanout_control
            .unbounded_send(ControlMessage::Remove("c".to_string()))
            .unwrap();

        let collect_a = tokio::spawn(rx_a.collect().compat());
        let collect_b = tokio::spawn(rx_b.collect().compat());
        let collect_c = tokio::spawn(rx_c.collect().compat());

        assert_eq!(collect_b.await.unwrap().unwrap(), recs);
        assert_eq!(collect_a.await.unwrap().unwrap(), recs);
        assert_eq!(collect_c.await.unwrap().unwrap(), vec![rec1]);
    }

    #[tokio::test]
    async fn fanout_shrink_at_notready() {
        let (tx_a, rx_a) = mpsc::channel(1);
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::channel(0);
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));
        let (tx_c, rx_c) = mpsc::channel(1);
        let tx_c = Box::new(tx_c.sink_map_err(|_| unreachable!()));

        let (mut fanout, fanout_control) = Fanout::new();

        fanout.add("a".to_string(), tx_a);
        fanout.add("b".to_string(), tx_b);
        fanout.add("c".to_string(), tx_c);

        let rec1 = Event::from("line 1".to_string());
        let rec2 = Event::from("line 2".to_string());
        let rec3 = Event::from("line 3".to_string());

        let recs = vec![rec1.clone(), rec2, rec3];
        let send = fanout.send_all(stream::iter_ok(recs.clone()));
        tokio::spawn(send.map(|_| ()).compat());

        delay_for(Duration::from_millis(50)).await;
        // The send_all task will be blocked on sending rec2 to b right now.
        fanout_control
            .unbounded_send(ControlMessage::Remove("b".to_string()))
            .unwrap();

        let collect_a = tokio::spawn(rx_a.collect().compat());
        let collect_b = tokio::spawn(rx_b.collect().compat());
        let collect_c = tokio::spawn(rx_c.collect().compat());

        assert_eq!(collect_a.await.unwrap().unwrap(), recs);
        assert_eq!(collect_b.await.unwrap().unwrap(), vec![rec1]);
        assert_eq!(collect_c.await.unwrap().unwrap(), recs);
    }

    #[tokio::test]
    async fn fanout_shrink_before_notready() {
        let (tx_a, rx_a) = mpsc::channel(1);
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::channel(0);
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));
        let (tx_c, rx_c) = mpsc::channel(1);
        let tx_c = Box::new(tx_c.sink_map_err(|_| unreachable!()));

        let (mut fanout, fanout_control) = Fanout::new();

        fanout.add("a".to_string(), tx_a);
        fanout.add("b".to_string(), tx_b);
        fanout.add("c".to_string(), tx_c);

        let rec1 = Event::from("line 1".to_string());
        let rec2 = Event::from("line 2".to_string());
        let rec3 = Event::from("line 3".to_string());

        let recs = vec![rec1.clone(), rec2.clone(), rec3];
        let send = fanout.send_all(stream::iter_ok(recs.clone()));
        tokio::spawn(send.map(|_| ()).compat());

        delay_for(Duration::from_millis(50)).await;
        // The send_all task will be blocked on sending rec2 to b right now.

        fanout_control
            .unbounded_send(ControlMessage::Remove("a".to_string()))
            .unwrap();

        let collect_a = tokio::spawn(rx_a.collect().compat());
        let collect_b = tokio::spawn(rx_b.collect().compat());
        let collect_c = tokio::spawn(rx_c.collect().compat());

        assert_eq!(collect_a.await.unwrap().unwrap(), [rec1, rec2]);
        assert_eq!(collect_b.await.unwrap().unwrap(), recs);
        assert_eq!(collect_c.await.unwrap().unwrap(), recs);
    }

    #[tokio::test]
    async fn fanout_no_sinks() {
        let fanout = Fanout::new().0;

        let rec1 = Event::from("line 1".to_string());
        let rec2 = Event::from("line 2".to_string());

        let fanout = fanout.send(rec1).compat().await.unwrap();
        let _fanout = fanout.send(rec2).compat().await.unwrap();
    }

    #[tokio::test]
    async fn fanout_replace() {
        let (tx_a1, rx_a1) = mpsc::unbounded();
        let tx_a1 = Box::new(tx_a1.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::unbounded();
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));

        let mut fanout = Fanout::new().0;

        fanout.add("a".to_string(), tx_a1);
        fanout.add("b".to_string(), tx_b);

        let rec1 = Event::from("line 1".to_string());
        let rec2 = Event::from("line 2".to_string());

        let fanout = fanout.send(rec1.clone()).compat().await.unwrap();
        let mut fanout = fanout.send(rec2.clone()).compat().await.unwrap();

        let (tx_a2, rx_a2) = mpsc::unbounded();
        let tx_a2 = Box::new(tx_a2.sink_map_err(|_| unreachable!()));
        fanout.replace("a".to_string(), tx_a2);

        let rec3 = Event::from("line 3".to_string());
        let _fanout = fanout.send(rec3.clone()).compat().await.unwrap();

        assert_eq!(
            collect_ready(rx_a1).await.unwrap(),
            vec![rec1.clone(), rec2.clone()]
        );
        assert_eq!(
            collect_ready(rx_b).await.unwrap(),
            vec![rec1, rec2, rec3.clone()]
        );
        assert_eq!(collect_ready(rx_a2).await.unwrap(), vec![rec3]);
    }
}
