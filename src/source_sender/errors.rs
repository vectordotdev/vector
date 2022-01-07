use std::fmt;

use futures::channel::mpsc;

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
