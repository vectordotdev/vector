use std::{error, fmt};

use bytes::{Buf, BufMut};

use super::builder::TopologyBuilder;
use crate::{
    encoding::FixedEncodable,
    topology::channel::{BufferReceiver, BufferSender},
    Bufferable, WhenFull,
};

// Silly implementation of `Encodable` to fulfill `Bufferable` for our test buffer code.
impl FixedEncodable for u64 {
    type EncodeError = BasicError;
    type DecodeError = BasicError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::EncodeError>
    where
        B: BufMut,
        Self: Sized,
    {
        buffer.put_u64(self);
        Ok(())
    }

    fn decode<B>(mut buffer: B) -> Result<u64, Self::DecodeError>
    where
        B: Buf,
    {
        if buffer.remaining() >= 8 {
            Ok(buffer.get_u64())
        } else {
            Err(BasicError("need 8 bytes minimum".to_string()))
        }
    }
}

#[derive(Debug)]
pub struct BasicError(pub(crate) String);

impl fmt::Display for BasicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for BasicError {}

/// Builds a buffer using in-memory channels.
///
/// If `mode` is set to `WhenFull::Overflow`, then the buffer will be set to overflow mode, with
/// another in-memory channel buffer being used as the overflow buffer.  The overflow buffer will
/// also use the same capacity as the outer buffer.
pub async fn build_buffer(
    capacity: usize,
    mode: WhenFull,
    overflow_mode: Option<WhenFull>,
) -> (BufferSender<u64>, BufferReceiver<u64>) {
    match mode {
        WhenFull::Overflow => {
            let overflow_mode = overflow_mode.expect("overflow mode cannot be empty");
            let (overflow_sender, overflow_receiver) =
                TopologyBuilder::memory_v2(capacity, overflow_mode).await;
            let (mut base_sender, mut base_receiver) =
                TopologyBuilder::memory_v2(capacity, WhenFull::Overflow).await;
            base_sender.switch_to_overflow(overflow_sender);
            base_receiver.switch_to_overflow(overflow_receiver);

            (base_sender, base_receiver)
        }
        m => TopologyBuilder::memory_v2(capacity, m).await,
    }
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
