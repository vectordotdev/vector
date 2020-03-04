//! StreamingSink

use crate::{Event, Result};
use async_trait::async_trait;
use futures::channel::mpsc::{channel, Receiver, Sender};
use futures::compat::CompatSink;
use futures01::{Poll as Poll01, Sink as Sink01, StartSend as StartSend01};

#[async_trait]
pub trait StreamingSink: Send + Sync + 'static {
    async fn run(&mut self, input: Receiver<Event>) -> Result<()>;
}

pub struct StreamingSinkAsSink01<T> {
    sink: Option<(Receiver<Event>, T)>,
    inner: CompatSink<Sender<Event>, Event>,
}

impl<T: StreamingSink> StreamingSinkAsSink01<T> {
    pub fn new(inner: T) -> Self {
        let (tx, rx) = channel(0);
        Self {
            sink: Some((rx, inner)),
            inner: CompatSink::new(tx),
        }
    }

    pub fn new_box(inner: T) -> crate::sinks::RouterSink {
        Box::new(Self::new(inner))
    }
}

impl<T: StreamingSink> Sink01 for StreamingSinkAsSink01<T> {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend01<Self::SinkItem, Self::SinkError> {
        if let Some((rx, mut sink)) = self.sink.take() {
            tokio02::spawn(async move {
                if let Err(error) = sink.run(rx).await {
                    error!(message = "Unexpected sink failure.", %error);
                }
            });
        }

        self.inner.start_send(item).map_err(drop)
    }

    fn poll_complete(&mut self) -> Poll01<(), Self::SinkError> {
        self.inner.poll_complete().map_err(drop)
    }
}
