use std::error::Error;

use async_trait::async_trait;

use crate::{
    Bufferable,
    buffer_usage_data::BufferUsageHandle,
    config::MemoryBufferSize,
    topology::{
        builder::IntoBuffer,
        channel::{ReceiverAdapter, SenderAdapter, limited},
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
    ) -> Result<(SenderAdapter<T>, ReceiverAdapter<T>), Box<dyn Error + Send + Sync>> {
        let (max_bytes, max_events) = match self.capacity {
            MemoryBufferSize::MaxEvents(max_events) => (None, Some(max_events.get())),
            MemoryBufferSize::MaxSize(max_size) => (Some(max_size.get() as u64), None),
        };

        usage_handle.set_buffer_limits(max_bytes, max_events);

        let (tx, rx) = limited(self.capacity, None, None);
        Ok((tx.into(), rx.into()))
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use tracing::Span;

    use super::*;
    use crate::{buffer_usage_data::BufferUsage, test::SizedRecord};

    async fn reported_limits(buffer: MemoryBuffer) -> (u64, usize) {
        let mut buffer_usage = BufferUsage::from_span(Span::none());
        let usage_handle = buffer_usage.add_stage(0);

        <MemoryBuffer as IntoBuffer<SizedRecord>>::into_buffer_parts(
            Box::new(buffer),
            usage_handle.clone(),
        )
        .await
        .unwrap();

        let snapshot = usage_handle.snapshot();
        (snapshot.max_size_bytes, snapshot.max_size_events)
    }

    #[tokio::test]
    async fn event_based_limit_is_reported_in_the_events_slot() {
        let (max_bytes, max_events) = reported_limits(MemoryBuffer::with_max_events(
            NonZeroUsize::new(500).unwrap(),
        ))
        .await;

        assert_eq!(max_events, 500);
        assert_eq!(max_bytes, 0);
    }

    #[tokio::test]
    async fn byte_based_limit_is_reported_in_the_bytes_slot() {
        let (max_bytes, max_events) = reported_limits(MemoryBuffer::new(
            MemoryBufferSize::MaxSize(NonZeroUsize::new(268_435_488).unwrap()),
        ))
        .await;

        assert_eq!(max_bytes, 268_435_488);
        assert_eq!(max_events, 0);
    }
}
