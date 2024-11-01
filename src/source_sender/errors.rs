use std::fmt;

use tokio::sync::mpsc;
use vector_lib::buffers::topology::channel::SendError;

use crate::event::{Event, EventArray};

#[derive(Clone, Debug)]
pub struct ClosedError;

impl fmt::Display for ClosedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sender is closed.")
    }
}

impl std::error::Error for ClosedError {}

impl From<mpsc::error::SendError<Event>> for ClosedError {
    fn from(_: mpsc::error::SendError<Event>) -> Self {
        Self
    }
}

impl From<mpsc::error::SendError<EventArray>> for ClosedError {
    fn from(_: mpsc::error::SendError<EventArray>) -> Self {
        Self
    }
}

impl<T> From<SendError<T>> for ClosedError {
    fn from(_: SendError<T>) -> Self {
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
