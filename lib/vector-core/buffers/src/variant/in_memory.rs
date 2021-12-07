use std::error::Error;

use async_trait::async_trait;
use tokio::sync::mpsc::channel;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    buffer_usage_data::BufferUsageHandle,
    topology::{builder::IntoBuffer, poll_sender::PollSender},
    Acker, Bufferable,
};

pub struct MemoryBuffer {
    capacity: usize,
}

impl MemoryBuffer {
    pub fn new(capacity: usize) -> Self {
        MemoryBuffer { capacity }
    }

    pub fn testing<T>(capacity: usize) -> (PollSender<T>, ReceiverStream<T>)
    where
        T: Bufferable,
    {
        let (tx, rx) = channel(capacity);
        (PollSender::new(tx), ReceiverStream::new(rx))
    }
}

#[async_trait]
impl<T> IntoBuffer<T> for MemoryBuffer
where
    T: Bufferable,
{
    fn provides_instrumentation(&self) -> bool {
        false
    }

    async fn into_buffer_parts(
        self: Box<Self>,
        usage_handle: &BufferUsageHandle,
    ) -> Result<(PollSender<T>, ReceiverStream<T>, Option<Acker>), Box<dyn Error + Send + Sync>>
    {
        usage_handle.set_buffer_limits(None, Some(self.capacity));

        let (tx, rx) = channel(self.capacity);
        Ok((PollSender::new(tx), ReceiverStream::new(rx), None))
    }
}
