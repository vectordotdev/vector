use crate::sinks::RouterSink;
use crate::Record;
use futures::sync::mpsc;
use futures::{future, Async, AsyncSink, Poll, Sink, StartSend, Stream};

pub struct Fanout {
    sinks: Vec<(String, RouterSink)>,
    i: usize,
    control_channel: mpsc::UnboundedReceiver<ControlMessage>,
}

pub enum ControlMessage {
    Add(String, RouterSink),
    #[allow(dead_code)]
    Remove(String),
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

        tokio::spawn(future::poll_fn(move || removed.poll_complete()));

        if self.i > i {
            self.i -= 1;
        }
    }

    pub fn process_control_messages(&mut self) {
        while let Ok(Async::Ready(Some(message))) = self.control_channel.poll() {
            match message {
                ControlMessage::Add(name, sink) => self.add(name, sink),
                ControlMessage::Remove(name) => self.remove(&name),
            }
        }
    }
}

impl Sink for Fanout {
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.process_control_messages();

        while self.i < self.sinks.len() {
            let (_name, sink) = &mut self.sinks[self.i];
            if let AsyncSink::NotReady(item) = sink.start_send(item.clone())? {
                return Ok(AsyncSink::NotReady(item));
            } else {
                self.i += 1;
            }
        }

        self.i = 0;

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.process_control_messages();

        let mut all_complete = true;

        for i in 0..self.sinks.len() {
            let (_name, sink) = &mut self.sinks[i];
            if sink.poll_complete()? != Async::Ready(()) {
                all_complete = false;
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
    use crate::test_util::{self, CollectCurrent};
    use crate::Record;
    use futures::sync::mpsc;
    use futures::{stream, Future, Sink, Stream};

    #[test]
    fn fanout_writes_to_all() {
        let (tx_a, rx_a) = mpsc::unbounded();
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::unbounded();
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));

        let mut fanout = Fanout::new().0;

        fanout.add("a".to_string(), tx_a);
        fanout.add("b".to_string(), tx_b);

        let rec1 = Record::from("line 1".to_string());
        let rec2 = Record::from("line 2".to_string());

        let fanout = fanout.send(rec1.clone()).wait().unwrap();
        let _fanout = fanout.send(rec2.clone()).wait().unwrap();

        assert_eq!(
            CollectCurrent::new(rx_a).wait().unwrap().1,
            vec![rec1.clone(), rec2.clone()]
        );
        assert_eq!(
            CollectCurrent::new(rx_b).wait().unwrap().1,
            vec![rec1.clone(), rec2.clone()]
        );
    }

    #[test]
    fn fanout_notready() {
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

        let rec1 = Record::from("line 1".to_string());
        let rec2 = Record::from("line 2".to_string());
        let rec3 = Record::from("line 3".to_string());

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let recs = vec![rec1.clone(), rec2.clone(), rec3.clone()];
        let send = fanout.send_all(stream::iter_ok(recs.clone()));
        rt.spawn(send.map(|_| ()));

        std::thread::sleep(std::time::Duration::from_millis(50));
        // The send_all task will be blocked on sending rec2 to b right now.

        let collect_a = futures::sync::oneshot::spawn(rx_a.collect(), &rt.executor());
        let collect_b = futures::sync::oneshot::spawn(rx_b.collect(), &rt.executor());
        let collect_c = futures::sync::oneshot::spawn(rx_c.collect(), &rt.executor());

        assert_eq!(collect_a.wait().unwrap(), recs.clone());
        assert_eq!(collect_b.wait().unwrap(), recs.clone());
        assert_eq!(collect_c.wait().unwrap(), recs.clone());
    }

    #[test]
    fn fanout_grow() {
        let (tx_a, rx_a) = mpsc::unbounded();
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::unbounded();
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));

        let mut fanout = Fanout::new().0;

        fanout.add("a".to_string(), tx_a);
        fanout.add("b".to_string(), tx_b);

        let rec1 = Record::from("line 1".to_string());
        let rec2 = Record::from("line 2".to_string());

        let fanout = fanout.send(rec1.clone()).wait().unwrap();
        let mut fanout = fanout.send(rec2.clone()).wait().unwrap();

        let (tx_c, rx_c) = mpsc::unbounded();
        let tx_c = Box::new(tx_c.sink_map_err(|_| unreachable!()));
        fanout.add("c".to_string(), tx_c);

        let rec3 = Record::from("line 3".to_string());
        let _fanout = fanout.send(rec3.clone()).wait().unwrap();

        assert_eq!(
            CollectCurrent::new(rx_a).wait().unwrap().1,
            vec![rec1.clone(), rec2.clone(), rec3.clone()]
        );
        assert_eq!(
            CollectCurrent::new(rx_b).wait().unwrap().1,
            vec![rec1.clone(), rec2.clone(), rec3.clone()]
        );
        assert_eq!(
            CollectCurrent::new(rx_c).wait().unwrap().1,
            vec![rec3.clone()]
        );
    }

    #[test]
    fn fanout_shrink() {
        let (tx_a, rx_a) = mpsc::unbounded();
        let tx_a = Box::new(tx_a.sink_map_err(|_| unreachable!()));
        let (tx_b, rx_b) = mpsc::unbounded();
        let tx_b = Box::new(tx_b.sink_map_err(|_| unreachable!()));

        let (mut fanout, fanout_control) = Fanout::new();

        fanout.add("a".to_string(), tx_a);
        fanout.add("b".to_string(), tx_b);

        let rec1 = Record::from("line 1".to_string());
        let rec2 = Record::from("line 2".to_string());

        let fanout = fanout.send(rec1.clone()).wait().unwrap();
        let fanout = fanout.send(rec2.clone()).wait().unwrap();

        fanout_control
            .unbounded_send(ControlMessage::Remove("b".to_string()))
            .unwrap();

        let rec3 = Record::from("line 3".to_string());
        let _fanout = test_util::block_on(fanout.send(rec3.clone())).unwrap();

        assert_eq!(
            CollectCurrent::new(rx_a).wait().unwrap().1,
            vec![rec1.clone(), rec2.clone(), rec3.clone()]
        );
        assert_eq!(
            CollectCurrent::new(rx_b).wait().unwrap().1,
            vec![rec1.clone(), rec2.clone()]
        );
    }

    #[test]
    fn fanout_shrink_after_notready() {
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

        let rec1 = Record::from("line 1".to_string());
        let rec2 = Record::from("line 2".to_string());
        let rec3 = Record::from("line 3".to_string());

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let recs = vec![rec1.clone(), rec2.clone(), rec3.clone()];
        let send = fanout.send_all(stream::iter_ok(recs.clone()));
        rt.spawn(send.map(|_| ()));

        std::thread::sleep(std::time::Duration::from_millis(50));
        // The send_all task will be blocked on sending rec2 to b right now.
        fanout_control
            .unbounded_send(ControlMessage::Remove("c".to_string()))
            .unwrap();

        let collect_a = futures::sync::oneshot::spawn(rx_a.collect(), &rt.executor());
        let collect_b = futures::sync::oneshot::spawn(rx_b.collect(), &rt.executor());
        let collect_c = futures::sync::oneshot::spawn(rx_c.collect(), &rt.executor());

        assert_eq!(collect_a.wait().unwrap(), recs.clone());
        assert_eq!(collect_b.wait().unwrap(), recs.clone());
        assert_eq!(collect_c.wait().unwrap(), vec![rec1.clone()]);
    }

    #[test]
    fn fanout_shrink_at_notready() {
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

        let rec1 = Record::from("line 1".to_string());
        let rec2 = Record::from("line 2".to_string());
        let rec3 = Record::from("line 3".to_string());

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let recs = vec![rec1.clone(), rec2.clone(), rec3.clone()];
        let send = fanout.send_all(stream::iter_ok(recs.clone()));
        rt.spawn(send.map(|_| ()));

        std::thread::sleep(std::time::Duration::from_millis(50));
        // The send_all task will be blocked on sending rec2 to b right now.
        fanout_control
            .unbounded_send(ControlMessage::Remove("b".to_string()))
            .unwrap();

        let collect_a = futures::sync::oneshot::spawn(rx_a.collect(), &rt.executor());
        let collect_b = futures::sync::oneshot::spawn(rx_b.collect(), &rt.executor());
        let collect_c = futures::sync::oneshot::spawn(rx_c.collect(), &rt.executor());

        assert_eq!(collect_a.wait().unwrap(), recs.clone());
        assert_eq!(collect_b.wait().unwrap(), vec![rec1.clone()]);
        assert_eq!(collect_c.wait().unwrap(), recs.clone());
    }

    #[test]
    fn fanout_shrink_before_notready() {
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

        let rec1 = Record::from("line 1".to_string());
        let rec2 = Record::from("line 2".to_string());
        let rec3 = Record::from("line 3".to_string());

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let recs = vec![rec1.clone(), rec2.clone(), rec3.clone()];
        let send = fanout.send_all(stream::iter_ok(recs.clone()));
        rt.spawn(send.map(|_| ()));

        std::thread::sleep(std::time::Duration::from_millis(50));
        // The send_all task will be blocked on sending rec2 to b right now.

        fanout_control
            .unbounded_send(ControlMessage::Remove("a".to_string()))
            .unwrap();

        let collect_a = futures::sync::oneshot::spawn(rx_a.collect(), &rt.executor());
        let collect_b = futures::sync::oneshot::spawn(rx_b.collect(), &rt.executor());
        let collect_c = futures::sync::oneshot::spawn(rx_c.collect(), &rt.executor());

        assert_eq!(collect_a.wait().unwrap(), [rec1.clone(), rec2.clone()]);
        assert_eq!(collect_b.wait().unwrap(), recs.clone());
        assert_eq!(collect_c.wait().unwrap(), recs.clone());
    }

    #[test]
    fn fanout_no_sinks() {
        let fanout = Fanout::new().0;

        let rec1 = Record::from("line 1".to_string());
        let rec2 = Record::from("line 2".to_string());

        let fanout = fanout.send(rec1.clone()).wait().unwrap();
        let _fanout = fanout.send(rec2.clone()).wait().unwrap();
    }
}
