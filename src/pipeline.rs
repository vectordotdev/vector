use crate::Event;
use futures01::{
    sync::mpsc::{channel, Receiver, SendError, Sender},
    AsyncSink, Poll, Sink,
};

#[derive(Debug, Clone)]
pub struct Pipeline {
    inner: Sender<Event>,
}

impl Sink for Pipeline {
    type SinkItem = Event;
    type SinkError = SendError<Self::SinkItem>;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        self.inner.start_send(item)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.inner.poll_complete()
    }
}

impl Pipeline {
    pub fn new_test() -> (Self, Receiver<Event>) {
        Self::new_with_buffer(100)
    }

    pub fn new_with_buffer(n: usize) -> (Self, Receiver<Event>) {
        let (tx, rx) = channel(n);
        (Self::from_sender(tx), rx)
    }

    pub fn from_sender(inner: Sender<Event>) -> Self {
        Self { inner }
    }

    pub fn poll_ready(&mut self) -> Poll<(), SendError<()>> {
        self.inner.poll_ready()
    }
}
