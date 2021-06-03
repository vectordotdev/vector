use crate::event::Event;
use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::{Sink, Stream, StreamExt};
use std::fmt;

pub enum VectorSink {
    Sink(Box<dyn Sink<Event, Error = ()> + Send + Unpin>),
    Stream(Box<dyn StreamSink + Send>),
}

impl VectorSink {
    pub async fn run<S>(mut self, input: S) -> Result<(), ()>
    where
        S: Stream<Item = Event> + Send,
    {
        match self {
            Self::Sink(sink) => input.map(Ok).forward(sink).await,
            Self::Stream(ref mut s) => s.run(Box::pin(input)).await,
        }
    }

    pub fn into_sink(self) -> Box<dyn Sink<Event, Error = ()> + Send + Unpin> {
        match self {
            Self::Sink(sink) => sink,
            _ => panic!("Failed type coercion, {:?} is not a Sink", self),
        }
    }
}

impl fmt::Debug for VectorSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VectorSink").finish()
    }
}

// === StreamSink ===

#[async_trait]
pub trait StreamSink {
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()>;
}
