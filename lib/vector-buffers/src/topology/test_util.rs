use std::{error, fmt, num::NonZeroUsize};

use bytes::{Buf, BufMut};
use vector_common::byte_size_of::ByteSizeOf;
use vector_common::finalization::{AddBatchNotifier, BatchNotifier};

use super::builder::TopologyBuilder;
use crate::{
    buffer_usage_data::BufferUsageHandle,
    encoding::FixedEncodable,
    topology::channel::{BufferReceiver, BufferSender},
    Bufferable, EventCount, WhenFull,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Sample(pub u64);

impl From<u64> for Sample {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

impl From<Sample> for u64 {
    fn from(s: Sample) -> Self {
        s.0
    }
}

impl AddBatchNotifier for Sample {
    fn add_batch_notifier(&mut self, batch: BatchNotifier) {
        drop(batch); // We never check acknowledgements for this type
    }
}

impl ByteSizeOf for Sample {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

// Silly implementation of `Encodable` to fulfill `Bufferable` for our test buffer code.
impl FixedEncodable for Sample {
    type EncodeError = BasicError;
    type DecodeError = BasicError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::EncodeError>
    where
        B: BufMut,
        Self: Sized,
    {
        buffer.put_u64(self.0);
        Ok(())
    }

    fn decode<B>(mut buffer: B) -> Result<Self, Self::DecodeError>
    where
        B: Buf,
    {
        if buffer.remaining() >= 8 {
            Ok(Self(buffer.get_u64()))
        } else {
            Err(BasicError("need 8 bytes minimum".to_string()))
        }
    }
}

impl EventCount for Sample {
    fn event_count(&self) -> usize {
        1
    }
}

#[derive(Debug)]
#[allow(dead_code)] // The inner _is_ read by the `Debug` impl, but that's ignored
pub struct BasicError(pub(crate) String);

impl fmt::Display for BasicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl error::Error for BasicError {}

/// Builds a buffer using in-memory channels.
///
/// If `mode` is set to `WhenFull::Overflow`, then the buffer will be set to overflow mode, with
/// another in-memory channel buffer being used as the overflow buffer.  The overflow buffer will
/// also use the same capacity as the outer buffer.
pub(crate) async fn build_buffer(
    capacity: usize,
    mode: WhenFull,
    overflow_mode: Option<WhenFull>,
) -> (
    BufferSender<Sample>,
    BufferReceiver<Sample>,
    BufferUsageHandle,
) {
    let handle = BufferUsageHandle::noop();
    let (tx, rx) = match mode {
        WhenFull::Overflow => {
            let overflow_mode = overflow_mode.expect("overflow mode cannot be empty");
            let (overflow_sender, overflow_receiver) = TopologyBuilder::standalone_memory_test(
                NonZeroUsize::new(capacity).expect("capacity must be nonzero"),
                overflow_mode,
                handle.clone(),
            )
            .await;
            let (mut base_sender, mut base_receiver) = TopologyBuilder::standalone_memory_test(
                NonZeroUsize::new(capacity).expect("capacity must be nonzero"),
                WhenFull::Overflow,
                handle.clone(),
            )
            .await;
            base_sender.switch_to_overflow(overflow_sender);
            base_receiver.switch_to_overflow(overflow_receiver);

            (base_sender, base_receiver)
        }
        m => {
            TopologyBuilder::standalone_memory_test(
                NonZeroUsize::new(capacity).expect("capacity must be nonzero"),
                m,
                handle.clone(),
            )
            .await
        }
    };

    (tx, rx, handle)
}

/// Gets the current capacity of the underlying base channel of the given sender.
fn get_base_sender_capacity<T: Bufferable>(sender: &BufferSender<T>) -> Option<usize> {
    sender.get_base_ref().capacity()
}

/// Gets the current capacity of the underlying overflow channel of the given sender..
///
/// As overflow is optional, the return value will be `None` is overflow is not configured.
fn get_overflow_sender_capacity<T: Bufferable>(sender: &BufferSender<T>) -> Option<usize> {
    sender
        .get_overflow_ref()
        .and_then(|s| s.get_base_ref().capacity())
}

/// Asserts the given sender's capacity, both for base and overflow, match the given values.
///
/// The overflow value is wrapped in `Option<T>` as not all senders will have overflow configured.
#[allow(clippy::missing_panics_doc)]
pub fn assert_current_send_capacity<T>(
    sender: &mut BufferSender<T>,
    base_expected: Option<usize>,
    overflow_expected: Option<usize>,
) where
    T: Bufferable,
{
    assert_eq!(get_base_sender_capacity(sender), base_expected);
    assert_eq!(get_overflow_sender_capacity(sender), overflow_expected);
}
