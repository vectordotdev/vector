use std::{fmt, pin::Pin};

use futures::{
    channel::mpsc,
    task::{Context, Poll},
    SinkExt, Stream, StreamExt,
};
use pin_project::pin_project;
#[cfg(test)]
use vector_core::event::EventStatus;
use vector_core::{event::Event, internal_event::EventsSent, ByteSizeOf};

const CHUNK_SIZE: usize = 1000;

#[derive(Debug)]
pub struct ClosedError;

impl fmt::Display for ClosedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sender is closed.")
    }
}

impl std::error::Error for ClosedError {}

impl From<mpsc::SendError> for ClosedError {
    fn from(_: mpsc::SendError) -> Self {
        Self
    }
}

#[derive(Debug)]
pub enum StreamSendError<E> {
    Closed(ClosedError),
    Stream(E),
}

impl<E> fmt::Display for StreamSendError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamSendError::Closed(e) => e.fmt(f),
            StreamSendError::Stream(e) => e.fmt(f),
        }
    }
}

impl<E> std::error::Error for StreamSendError<E> where E: std::error::Error {}

impl<E> From<ClosedError> for StreamSendError<E> {
    fn from(e: ClosedError) -> Self {
        StreamSendError::Closed(e)
    }
}

#[derive(Derivative, Clone)]
#[derivative(Debug)]
pub struct SourceSender {
    inner: mpsc::Sender<Event>,
}

impl SourceSender {
    pub async fn send(&mut self, event: Event) -> Result<(), ClosedError> {
        let byte_size = event.size_of();
        self.inner.send(event).await?;
        emit!(&EventsSent {
            count: 1,
            byte_size,
        });
        Ok(())
    }

    pub async fn send_all(
        &mut self,
        events: impl Stream<Item = Event> + Unpin,
    ) -> Result<(), ClosedError> {
        let mut stream = events.ready_chunks(CHUNK_SIZE);
        while let Some(events) = stream.next().await {
            self.send_batch(events).await?;
        }
        Ok(())
    }

    pub async fn send_batch(&mut self, events: Vec<Event>) -> Result<(), ClosedError> {
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

    pub async fn send_result_stream<E>(
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

    pub fn new_with_buffer(n: usize) -> (Self, ReceiverStream<Event>) {
        let (tx, rx) = mpsc::channel(n);
        (Self::from_sender(tx), ReceiverStream::new(rx))
    }

    pub const fn from_sender(inner: mpsc::Sender<Event>) -> Self {
        Self { inner }
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
