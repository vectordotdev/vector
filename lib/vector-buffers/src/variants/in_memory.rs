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
            MemoryBufferSize::MaxSize(max_size) => (u64::try_from(max_size.get()).ok(), None),
        };

        usage_handle.set_buffer_limits(max_bytes, max_events);

        let (tx, rx) = limited(self.capacity, None, None);
        Ok((tx.into(), rx.into()))
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::*;
    use crate::test::SizedRecord;

    #[tokio::test]
    async fn reports_buffer_limit_in_its_configured_unit() {
        let cases = [
            (
                MemoryBufferSize::MaxEvents(NonZeroUsize::new(100).unwrap()),
                100,
                0,
            ),
            (
                MemoryBufferSize::MaxSize(NonZeroUsize::new(4096).unwrap()),
                0,
                4096,
            ),
        ];

        for (capacity, expected_max_events, expected_max_bytes) in cases {
            let usage_handle = BufferUsageHandle::noop();
            let buffer = Box::new(MemoryBuffer::new(capacity));
            let _parts = IntoBuffer::<SizedRecord>::into_buffer_parts(buffer, usage_handle.clone())
                .await
                .expect("memory buffer should build");

            let snapshot = usage_handle.snapshot();
            assert_eq!(
                snapshot.max_size_events, expected_max_events,
                "{capacity:?}"
            );
            assert_eq!(snapshot.max_size_bytes, expected_max_bytes, "{capacity:?}");
        }
    }
}
