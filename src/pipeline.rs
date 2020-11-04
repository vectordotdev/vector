use crate::Event;
use futures01::{
    sync::mpsc::{channel, Receiver, SendError, Sender},
    AsyncSink, Poll, Sink,
};
use crate::transforms::FunctionTransform;

#[derive(Derivative, Clone)]
#[derivative(Debug)]
pub struct Pipeline {
    inner: Sender<Event>,
    #[derivative(Debug="ignore")]
    inlines: Vec<Box<dyn FunctionTransform>>
}

impl Sink for Pipeline {
    type SinkItem = Event;
    type SinkError = SendError<Self::SinkItem>;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> { self.inner.start_send(item)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.inner.poll_complete()
    }
}

impl Pipeline {
    #[cfg(test)]
    pub fn new_test(inlines: Vec<Box<dyn FunctionTransform>>) -> (Self, Receiver<Event>) {
        Self::new_with_buffer(100, inlines)
    }

    pub fn new_with_buffer(n: usize, inlines: Vec<Box<dyn FunctionTransform>>) -> (Self, Receiver<Event>) {
        let (tx, rx) = channel(n);
        (Self::from_sender(tx, inlines), rx)
    }

    pub fn from_sender(inner: Sender<Event>, inlines: Vec<Box<dyn FunctionTransform>>) -> Self {
        Self {
            inner,
            inlines,
        }
    }

    pub fn poll_ready(&mut self) -> Poll<(), SendError<()>> {
        self.inner.poll_ready()
    }
}
