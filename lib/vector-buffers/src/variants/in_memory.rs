use std::error::Error;

use async_trait::async_trait;

use crate::{
    Bufferable,
    buffer_usage_data::BufferUsageHandle,
    config::MemoryBufferSize,
    topology::{
        builder::IntoBuffer,
        channel::{ReceiverAdapter, SenderAdapter, limited_with_usage_handle},
    },
};

pub struct MemoryBuffer {
    capacity: MemoryBufferSize,
}

impl MemoryBuffer {
    pub fn new(capacity: MemoryBufferSize) -> Self {
        MemoryBuffer { capacity }
    }

    #[cfg(test)]
    pub fn with_max_events(n: std::num::NonZeroUsize) -> Self {
        Self {
            capacity: MemoryBufferSize::MaxEvents(n),
        }
    }
}

#[async_trait]
impl<T> IntoBuffer<T> for MemoryBuffer
where
    T: Bufferable,
{
    async fn into_buffer_parts(
        self: Box<Self>,
        usage_handle: BufferUsageHandle,
        observe: bool,
    ) -> Result<(SenderAdapter<T>, ReceiverAdapter<T>), Box<dyn Error + Send + Sync>> {
        let (max_bytes, max_events) = match self.capacity {
            MemoryBufferSize::MaxEvents(max_events) => (None, Some(max_events.get())),
            MemoryBufferSize::MaxSize(max_bytes) => (Some(max_bytes.get() as u64), None),
        };

        usage_handle.set_buffer_limits(max_bytes, max_events);

        let occupancy_handle = observe.then_some(usage_handle.clone());
        let (tx, rx) = limited_with_usage_handle(self.capacity, None, None, occupancy_handle);
        Ok((tx.into(), rx.into()))
    }
}
