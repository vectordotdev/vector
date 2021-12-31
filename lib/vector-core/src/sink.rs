use std::fmt;

use async_trait::async_trait;
use futures::{stream::BoxStream, Sink, Stream, StreamExt};

use crate::event::Event;

pub enum VectorSink {
    Sink(Box<dyn Sink<Event, Error = ()> + Send + Unpin>),
    Stream(Box<dyn StreamSink + Send>),
}

impl VectorSink {
    /// Run the `VectorSink`
    ///
    /// # Errors
    ///
    /// It is unclear under what conditions this function will error.
    pub async fn run<S>(self, input: S) -> Result<(), ()>
    where
        S: Stream<Item = Event> + Send,
    {
        match self {
            Self::Sink(sink) => input.map(Ok).forward(sink).await,
            Self::Stream(s) => s.run(Box::pin(input)).await,
        }
    }

    /// Converts `VectorSink` into a `futures::Sink`
    ///
    /// # Panics
    ///
    /// This function will panic if the self instance is not `VectorSink::Sink`.
    pub fn into_sink(self) -> Box<dyn Sink<Event, Error = ()> + Send + Unpin> {
        match self {
            Self::Sink(sink) => sink,
            _ => panic!("Failed type coercion, {:?} is not a Sink", self),
        }
    }

    /// Converts `VectorSink` into a `StreamSink`
    ///
    /// # Panics
    ///
    /// This function will panic if the self instance is not `VectorSink::Stream`.
    pub fn into_stream(self) -> Box<dyn StreamSink + Send> {
        match self {
            Self::Stream(stream) => stream,
            _ => panic!("Failed type coercion, {:?} is not a Stream", self),
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
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()>;
}
