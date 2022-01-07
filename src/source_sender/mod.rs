use std::{collections::HashMap, pin::Pin};

use futures::{
    channel::mpsc,
    task::{Context, Poll},
    SinkExt, Stream, StreamExt,
};
use pin_project::pin_project;
#[cfg(test)]
use vector_core::event::EventStatus;
use vector_core::{config::Output, event::Event, internal_event::EventsSent, ByteSizeOf};

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

    pub fn add_output(&mut self, output: Output) -> ReceiverStream<Event> {
        let (inner, rx) = Inner::new_with_buffer(self.buf_size);
        match output.port {
            None => {
                self.inner = Some(inner);
            }
            Some(name) => {
                self.named_inners.insert(name, inner);
            }
        }
        rx
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

    pub fn new_with_buffer(n: usize) -> (Self, ReceiverStream<Event>) {
        let (inner, rx) = Inner::new_with_buffer(n);
        (
            Self {
                inner: Some(inner),
                named_inners: Default::default(),
            },
            rx,
        )
    }

    #[cfg(test)]
    pub fn new_test() -> (Self, ReceiverStream<Event>) {
        Self::new_with_buffer(100)
    }

    #[cfg(test)]
    pub fn new_test_finalize(status: EventStatus) -> (Self, impl Stream<Item = Event> + Unpin) {
        let (pipe, recv) = Self::new_with_buffer(100);
        // In a source test pipeline, there is no sink to acknowledge
        // events, so we have to add a map to the receiver to handle the
        // finalization.
        let recv = recv.map(move |mut event| {
            let metadata = event.metadata_mut();
            metadata.update_status(status);
            metadata.update_sources();
            event
        });
        (pipe, recv)
    }

    pub async fn send(&mut self, event: Event) -> Result<(), ClosedError> {
        self.inner
            .as_mut()
            .expect("no default output")
            .send(event)
            .await
    }

    pub async fn send_named(&mut self, name: &str, event: Event) -> Result<(), ClosedError> {
        self.named_inners
            .get_mut(name)
            .expect("unknown output")
            .send(event)
            .await
    }

    pub async fn send_all(
        &mut self,
        events: impl Stream<Item = Event> + Unpin,
    ) -> Result<(), ClosedError> {
        self.inner
            .as_mut()
            .expect("no default output")
            .send_all(events)
            .await
    }

    pub async fn send_batch(&mut self, events: Vec<Event>) -> Result<(), ClosedError> {
        self.inner
            .as_mut()
            .expect("no default output")
            .send_batch(events)
            .await
    }

    pub async fn send_result_stream<E>(
        &mut self,
        stream: impl Stream<Item = Result<Event, E>> + Unpin,
    ) -> Result<(), StreamSendError<E>> {
        self.inner
            .as_mut()
            .expect("no default output")
            .send_result_stream(stream)
            .await
    }
}

#[derive(Debug, Clone)]
struct Inner {
    inner: mpsc::Sender<Event>,
}

impl Inner {
    fn new_with_buffer(n: usize) -> (Self, ReceiverStream<Event>) {
        let (tx, rx) = mpsc::channel(n);
        (Self { inner: tx }, ReceiverStream::new(rx))
    }

    async fn send(&mut self, event: Event) -> Result<(), ClosedError> {
        let byte_size = event.size_of();
        self.inner.send(event).await?;
        emit!(&EventsSent {
            count: 1,
            byte_size,
        });
        Ok(())
    }

    async fn send_all(
        &mut self,
        events: impl Stream<Item = Event> + Unpin,
    ) -> Result<(), ClosedError> {
        let mut stream = events.ready_chunks(CHUNK_SIZE);
        while let Some(events) = stream.next().await {
            self.send_batch(events).await?;
        }
        Ok(())
    }

    async fn send_batch(&mut self, events: Vec<Event>) -> Result<(), ClosedError> {
        let mut count = 0;
        let mut byte_size = 0;

        for event in events {
            let event_size = event.size_of();
            match self.inner.send(event).await {
                Ok(()) => {
                    count += 1;
                    byte_size += event_size;
                }
                Err(error) => {
                    emit!(&EventsSent { count, byte_size });
                    return Err(error.into());
                }
            }
        }

        emit!(&EventsSent { count, byte_size });

        Ok(())
    }

    async fn send_result_stream<E>(
        &mut self,
        mut stream: impl Stream<Item = Result<Event, E>> + Unpin,
    ) -> Result<(), StreamSendError<E>> {
        let mut to_forward = Vec::with_capacity(CHUNK_SIZE);
        loop {
            tokio::select! {
                next = stream.next(), if to_forward.len() <= CHUNK_SIZE => {
                    match next {
                        Some(Ok(event)) => {
                            to_forward.push(event);
                        }
                        Some(Err(error)) => {
                            if !to_forward.is_empty() {
                                self.send_batch(to_forward).await?;
                            }
                            return Err(StreamSendError::Stream(error));
                        }
                        None => {
                            if !to_forward.is_empty() {
                                self.send_batch(to_forward).await?;
                            }
                            break;
                        }
                    }
                }
                else => {
                    if !to_forward.is_empty() {
                        let out = std::mem::replace(&mut to_forward, Vec::with_capacity(CHUNK_SIZE));
                        self.send_batch(out).await?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[pin_project]
#[derive(Debug)]
pub struct ReceiverStream<T> {
    #[pin]
    inner: mpsc::Receiver<T>,
}

impl<T> ReceiverStream<T> {
    fn new(inner: mpsc::Receiver<T>) -> Self {
        Self { inner }
    }

    pub fn close(&mut self) {
        self.inner.close()
    }
}

impl<T> Stream for ReceiverStream<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        this.inner.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
