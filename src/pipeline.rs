use std::{collections::VecDeque, fmt};

use futures::{stream, Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
#[cfg(test)]
use vector_core::event::EventStatus;
use vector_core::{event::Event, internal_event::EventsSent, ByteSizeOf};

const CHUNK_SIZE: usize = 1000;

#[derive(Debug)]
pub struct ClosedError;

impl fmt::Display for ClosedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pipeline is closed.")
    }
}

impl std::error::Error for ClosedError {}

impl From<mpsc::error::SendError<Event>> for ClosedError {
    fn from(_: mpsc::error::SendError<Event>) -> Self {
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

#[derive(Derivative, Clone)]
#[derivative(Debug)]
pub struct Pipeline {
    inner: mpsc::Sender<Event>,
    enqueued: VecDeque<Event>,
}

impl Pipeline {
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
        }

        Ok(())
    }

    pub async fn send_result_stream<E>(
        &mut self,
        events: impl Stream<Item = Result<Event, E>> + Unpin,
    ) -> Result<(), StreamSendError<E>> {
        let mut stream = events.ready_chunks(CHUNK_SIZE);
        while let Some(results) = stream.next().await {
            let mut stream_error = None;
            let mut to_forward = Vec::with_capacity(results.len());
            for result in results {
                match result {
                    Ok(event) => to_forward.push(event),
                    Err(e) => {
                        stream_error = Some(e);
                        break;
                    }
                }
            }
            if let Err(closed_err) = self.send_all(&mut stream::iter(to_forward)).await {
                return Err(StreamSendError::Closed(closed_err));
            }
            if let Some(error) = stream_error {
                return Err(StreamSendError::Stream(error));
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

    pub fn from_sender(inner: mpsc::Sender<Event>) -> Self {
        Self {
            inner,
            // We ensure the buffer is sufficient that it is unlikely to require reallocations.
            // There is a possibility a component might blow this queue size.
            enqueued: VecDeque::with_capacity(10),
        }
    }
}
