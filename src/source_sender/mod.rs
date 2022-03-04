use std::collections::HashMap;

use futures::{SinkExt, Stream, StreamExt};
use vector_buffers::topology::channel::{self, LimitedReceiver, LimitedSender};
#[cfg(test)]
use vector_core::event::{into_event_stream, EventStatus};
use vector_core::{
    config::Output,
    event::{array, Event, EventArray, EventContainer},
    internal_event::{EventsSent, DEFAULT_OUTPUT},
    ByteSizeOf,
};

mod errors;

pub use errors::{ClosedError, StreamSendError};

const CHUNK_SIZE: usize = 1000;

#[derive(Debug)]
pub struct Builder {
    buf_size: usize,
    inner: Option<Inner>,
    named_inners: HashMap<String, Inner>,
}

impl Builder {
    // https://github.com/rust-lang/rust/issues/73255
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_buffer(self, n: usize) -> Self {
        Self {
            buf_size: n,
            inner: self.inner,
            named_inners: self.named_inners,
        }
    }

    pub fn add_output(&mut self, output: Output) -> LimitedReceiver<EventArray> {
        match output.port {
            None => {
                let (inner, rx) = Inner::new_with_buffer(self.buf_size, DEFAULT_OUTPUT.to_owned());
                self.inner = Some(inner);
                rx
            }
            Some(name) => {
                let (inner, rx) = Inner::new_with_buffer(self.buf_size, name.clone());
                self.named_inners.insert(name, inner);
                rx
            }
        }
    }

    // https://github.com/rust-lang/rust/issues/73255
    #[allow(clippy::missing_const_for_fn)]
    pub fn build(self) -> SourceSender {
        SourceSender {
            inner: self.inner,
            named_inners: self.named_inners,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceSender {
    inner: Option<Inner>,
    named_inners: HashMap<String, Inner>,
}

impl SourceSender {
    pub fn builder() -> Builder {
        Builder {
            buf_size: CHUNK_SIZE,
            inner: None,
            named_inners: Default::default(),
        }
    }

    pub fn new_with_buffer(n: usize) -> (Self, LimitedReceiver<EventArray>) {
        let (inner, rx) = Inner::new_with_buffer(n, DEFAULT_OUTPUT.to_owned());
        (
            Self {
                inner: Some(inner),
                named_inners: Default::default(),
            },
            rx,
        )
    }

    #[cfg(test)]
    pub fn new_test() -> (Self, impl Stream<Item = Event> + Unpin) {
        let (pipe, recv) = Self::new_with_buffer(100);
        let recv = recv.flat_map(into_event_stream);
        (pipe, recv)
    }

    #[cfg(test)]
    pub fn new_test_finalize(status: EventStatus) -> (Self, impl Stream<Item = Event> + Unpin) {
        let (pipe, recv) = Self::new_with_buffer(100);
        // In a source test pipeline, there is no sink to acknowledge
        // events, so we have to add a map to the receiver to handle the
        // finalization.
        let recv = recv.flat_map(move |mut events| {
            events.for_each_event(|mut event| {
                let metadata = event.metadata_mut();
                metadata.update_status(status);
                metadata.update_sources();
            });
            into_event_stream(events)
        });
        (pipe, recv)
    }

    #[cfg(test)]
    pub fn add_outputs(
        &mut self,
        status: EventStatus,
        name: String,
    ) -> impl Stream<Item = EventArray> + Unpin {
        let (inner, recv) = Inner::new_with_buffer(100, name.clone());
        let recv = recv.map(move |mut events| {
            events.for_each_event(|mut event| {
                let metadata = event.metadata_mut();
                metadata.update_status(status);
                metadata.update_sources();
            });
            events
        });
        self.named_inners.insert(name, inner);
        recv
    }

    pub async fn send_event(&mut self, event: impl Into<EventArray>) -> Result<(), ClosedError> {
        self.inner
            .as_mut()
            .expect("no default output")
            .send_event(event)
            .await
    }

    pub async fn send_event_stream<S, E>(&mut self, events: S) -> Result<(), ClosedError>
    where
        S: Stream<Item = E> + Unpin,
        E: Into<Event> + ByteSizeOf,
    {
        self.inner
            .as_mut()
            .expect("no default output")
            .send_event_stream(events)
            .await
    }

    pub async fn send_batch<I, E>(&mut self, events: I) -> Result<(), ClosedError>
    where
        E: Into<Event> + ByteSizeOf,
        I: IntoIterator<Item = E>,
    {
        self.inner
            .as_mut()
            .expect("no default output")
            .send_batch(events)
            .await
    }

    pub async fn send_batch_named<I, E>(&mut self, name: &str, events: I) -> Result<(), ClosedError>
    where
        E: Into<Event> + ByteSizeOf,
        I: IntoIterator<Item = E>,
    {
        self.named_inners
            .get_mut(name)
            .expect("unknown output")
            .send_batch(events)
            .await
    }
}

#[derive(Debug, Clone)]
struct Inner {
    inner: LimitedSender<EventArray>,
    output: String,
}

impl Inner {
    fn new_with_buffer(n: usize, output: String) -> (Self, LimitedReceiver<EventArray>) {
        let (tx, rx) = channel::limited(n);
        (Self { inner: tx, output }, rx)
    }

    async fn send(&mut self, events: EventArray) -> Result<(), ClosedError> {
        let byte_size = events.size_of();
        let count = events.len();
        self.inner.send(events).await?;
        emit!(&EventsSent {
            count,
            byte_size,
            output: Some(self.output.as_ref()),
        });
        Ok(())
    }

    async fn send_event(&mut self, event: impl Into<EventArray>) -> Result<(), ClosedError> {
        self.send(event.into()).await
    }

    async fn send_event_stream<S, E>(&mut self, events: S) -> Result<(), ClosedError>
    where
        S: Stream<Item = E> + Unpin,
        E: Into<Event> + ByteSizeOf,
    {
        let mut stream = events.ready_chunks(CHUNK_SIZE);
        while let Some(events) = stream.next().await {
            self.send_batch(events.into_iter()).await?;
        }
        Ok(())
    }

    async fn send_batch<I, E>(&mut self, events: I) -> Result<(), ClosedError>
    where
        E: Into<Event> + ByteSizeOf,
        I: IntoIterator<Item = E>,
    {
        let mut count = 0;
        let mut byte_size = 0;

        let events = events.into_iter().map(Into::into);
        for events in array::events_into_arrays(events, Some(CHUNK_SIZE)) {
            let this_count = events.len();
            let this_size = events.size_of();
            match self.inner.send(events).await {
                Ok(()) => {
                    count += this_count;
                    byte_size += this_size;
                }
                Err(error) => {
                    emit!(&EventsSent {
                        count,
                        byte_size,
                        output: Some(self.output.as_ref()),
                    });
                    return Err(error.into());
                }
            }
        }

        emit!(&EventsSent {
            count,
            byte_size,
            output: Some(self.output.as_ref()),
        });

        Ok(())
    }
}
