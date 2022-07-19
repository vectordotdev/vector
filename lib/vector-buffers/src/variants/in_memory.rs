use std::{error::Error, num::NonZeroUsize};

use async_trait::async_trait;

use crate::{
    buffer_usage_data::BufferUsageHandle,
    topology::{
        builder::IntoBuffer,
        channel::{limited, ReceiverAdapter, SenderAdapter},
    },
    Bufferable,
};

pub struct MemoryBuffer {
    capacity: NonZeroUsize,
}

impl MemoryBuffer {
    pub fn new(capacity: NonZeroUsize) -> Self {
        MemoryBuffer { capacity }
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
    ) -> Result<(SenderAdapter<T>, ReceiverAdapter<T>), Box<dyn Error + Send + Sync>> {
        usage_handle.set_buffer_limits(None, Some(self.capacity.get()));

        let (tx, rx) = limited(self.capacity.get());
        Ok((tx.into(), rx.into()))
    }
}
