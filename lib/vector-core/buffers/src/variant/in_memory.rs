use std::error::Error;

use async_trait::async_trait;
use tokio::sync::mpsc::channel;

use crate::{
    buffer_usage_data::BufferUsageHandle,
    topology::{
        builder::IntoBuffer,
        channel::{ReceiverAdapter, SenderAdapter},
    },
    Acker, Bufferable,
};

pub struct MemoryBuffer {
    capacity: usize,
}

impl MemoryBuffer {
    pub fn new(capacity: usize) -> Self {
        MemoryBuffer { capacity }
    }

    pub fn testing<T>(capacity: usize) -> (SenderAdapter<T>, ReceiverAdapter<T>)
    where
        T: Bufferable,
    {
        let (tx, rx) = channel(capacity);
        (SenderAdapter::channel(tx), ReceiverAdapter::channel(rx))
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
    ) -> Result<(SenderAdapter<T>, ReceiverAdapter<T>, Option<Acker>), Box<dyn Error + Send + Sync>>
    {
        usage_handle.set_buffer_limits(None, Some(self.capacity));

        let (tx, rx) = channel(self.capacity);
        Ok((
            SenderAdapter::channel(tx),
            ReceiverAdapter::channel(rx),
            None,
        ))
    }
}
