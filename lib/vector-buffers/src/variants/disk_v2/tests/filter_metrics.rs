//! Buffer-usage accounting around `Bufferable::filter_unencodable`.
//!
//! When the disk-v2 sender drops sub-items because they exceed protobuf nesting
//! limits, those drops must show up as unintentional buffer drops so that
//! `buffer_size_*` (received minus left) stays consistent with what is actually
//! queued on disk. Without that, a single rejected event makes the buffer report
//! one queued event forever.

use std::{error, fmt};

use bytes::{Buf, BufMut};
use vector_common::{
    byte_size_of::ByteSizeOf,
    finalization::{AddBatchNotifier, BatchNotifier},
};

use super::create_default_buffer_v2_with_usage;
use crate::{
    Bufferable, EventCount, WhenFull,
    encoding::FixedEncodable,
    test::{install_tracing_helpers, with_temp_dir},
    topology::channel::{BufferSender, SenderAdapter},
};

/// A bufferable carrying a self-declared `event_count` of `events`, whose
/// `filter_unencodable` shrinks it to `post_filter` events (or drops it entirely
/// when `post_filter == 0`). Lets the test pin "before vs after filter" sizing
/// without needing the full `EventArray` machinery.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FilterableBatch {
    events: u32,
    post_filter: u32,
}

impl AddBatchNotifier for FilterableBatch {
    fn add_batch_notifier(&mut self, batch: BatchNotifier) {
        drop(batch);
    }
}
impl ByteSizeOf for FilterableBatch {
    fn allocated_bytes(&self) -> usize {
        0
    }
}
impl EventCount for FilterableBatch {
    fn event_count(&self) -> usize {
        self.events as usize
    }
}

#[derive(Debug)]
struct CodecError;
impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl error::Error for CodecError {}

impl FixedEncodable for FilterableBatch {
    type EncodeError = CodecError;
    type DecodeError = CodecError;
    fn encode<B: BufMut>(self, buf: &mut B) -> Result<(), Self::EncodeError> {
        if buf.remaining_mut() < 8 {
            return Err(CodecError);
        }
        buf.put_u32(self.events);
        buf.put_u32(self.post_filter);
        Ok(())
    }
    fn decode<B: Buf>(mut buf: B) -> Result<Self, Self::DecodeError> {
        Ok(FilterableBatch {
            events: buf.get_u32(),
            post_filter: buf.get_u32(),
        })
    }
    fn encoded_size(&self) -> Option<usize> {
        Some(8)
    }
}

impl Bufferable for FilterableBatch {
    fn filter_unencodable(self) -> Option<Self> {
        if self.post_filter == 0 {
            None
        } else {
            Some(FilterableBatch {
                events: self.post_filter,
                post_filter: self.post_filter,
            })
        }
    }
}

/// A partial-filter drop on a disk-v2 send must show up as an unintentional buffer
/// drop, so `buffer_size_*` stays consistent with what actually landed on disk.
#[tokio::test]
async fn filter_drops_are_reported_as_unintentional_buffer_drops() {
    let _a = install_tracing_helpers();

    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let (writer, _reader, _ledger, usage) =
                create_default_buffer_v2_with_usage::<_, FilterableBatch>(data_dir).await;
            let mut sender = BufferSender::new(SenderAdapter::from(writer), WhenFull::Block);
            sender.with_usage_instrumentation(usage.clone());

            // 10 events arrive, filter keeps 3.
            sender
                .send(
                    FilterableBatch {
                        events: 10,
                        post_filter: 3,
                    },
                    None,
                )
                .await
                .expect("send should succeed");

            let snapshot = usage.snapshot();
            assert_eq!(
                snapshot.received_event_count, 10,
                "received reflects pre-filter sizing (the item arrived at the buffer boundary)",
            );
            assert_eq!(
                snapshot.dropped_event_count, 7,
                "filter drops are reported as an unintentional buffer drop \
                 so buffer_size stays consistent (received - dropped = 3 queued)",
            );
            assert_eq!(
                snapshot.dropped_event_count_intentional, 0,
                "no buffer-fullness drops here",
            );

            // 5 events arrive, filter drops them all.
            sender
                .send(
                    FilterableBatch {
                        events: 5,
                        post_filter: 0,
                    },
                    None,
                )
                .await
                .expect("send should succeed");

            let snapshot = usage.snapshot();
            assert_eq!(
                snapshot.received_event_count, 15,
                "fully-filtered item still bumps received (it arrived at the boundary)",
            );
            assert_eq!(
                snapshot.dropped_event_count, 12,
                "all 5 events from the fully-filtered item are reported as unintentional drops",
            );
        }
    })
    .await;
}

// Note: A regression test that exercises the "full disk hands item to overflow
// unfiltered" path is not included here because reliably driving the disk-v2
// writer's `is_buffer_full()` to `true` under the minimum-size config takes
// careful tuning of record/buffer sizes (the writer's `can_write_record` check
// generally short-circuits writes *before* `total_buffer_size` reaches
// `max_buffer_size`). The fix in `SenderAdapter::try_send` is a single
// `is_buffer_full()` short-circuit before the filter runs; the existing
// disk-v2 tests cover the full-buffer behaviour at the writer level.
