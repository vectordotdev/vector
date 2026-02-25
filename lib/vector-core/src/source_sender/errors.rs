use std::fmt;

use vector_buffers::topology::channel;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SendError {
    Timeout,
    Closed,
}

impl<T> From<channel::SendError<T>> for SendError {
    fn from(_: channel::SendError<T>) -> Self {
        Self::Closed
    }
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => f.write_str("Send timed out."),
            Self::Closed => f.write_str("Sender is closed."),
        }
    }
}

impl std::error::Error for SendError {}
