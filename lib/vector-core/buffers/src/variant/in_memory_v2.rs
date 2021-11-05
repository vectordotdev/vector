use tokio::sync::mpsc::channel;
use tokio_stream::wrappers::ReceiverStream;

use crate::topology::{builder::IntoBuffer, poll_sender::PollSender};

pub struct MemoryBuffer {
    capacity: usize,
}

impl MemoryBuffer {
    pub fn new(capacity: usize) -> Self {
        MemoryBuffer { capacity }
    }
}

impl<T> IntoBuffer<T> for MemoryBuffer
where
    T: Send + 'static,
{
    fn into_buffer_parts(self) -> (PollSender<T>, ReceiverStream<T>) {
        let (tx, rx) = channel(self.capacity);

        (PollSender::new(tx), ReceiverStream::new(rx))
    }
}
