//! Buffer-usage accounting around `Bufferable::filter_unencodable`.
//!
//! When the disk-v2 sender drops sub-items because they exceed protobuf nesting
//! limits, those drops must show up as unintentional buffer drops so that
//! `buffer_size_*` (received minus left) stays consistent with what is actually
//! queued on disk. Without that, a single rejected event makes the buffer report
//! one queued event forever.

use std::{error, fmt, num::NonZeroUsize, time::Duration};

use bytes::{Buf, BufMut};
use tokio::time::timeout;
use vector_common::{
    byte_size_of::ByteSizeOf,
    finalization::{
        AddBatchNotifier, BatchNotifier, EventFinalizers, Finalizable, MergeFinalizable,
    },
};

use super::create_default_buffer_v2_with_usage;
use crate::{
    Bufferable, EventCount, MemoryBufferSize, WhenFull,
    encoding::FixedEncodable,
    test::{install_tracing_helpers, with_temp_dir},
    topology::channel::{BufferSender, SenderAdapter, limited},
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

// This fixture carries no finalizers -- it exists only to pin event counts either side of
// the filter -- so both impls are inert, matching `add_batch_notifier` above. They are
// required because `Bufferable` gained a `GroupedFinalizable` bound, which is satisfied for
// any `MergeFinalizable` via a blanket impl.
impl Finalizable for FilterableBatch {
    fn take_finalizers(&mut self) -> EventFinalizers {
        EventFinalizers::default()
    }
}

impl MergeFinalizable for FilterableBatch {
    fn merge_finalizers(&mut self, finalizers: EventFinalizers) {
        drop(finalizers);
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
    fn is_fully_encodable(&self) -> bool {
        self.post_filter == self.events
    }

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
///
/// Note: we deliberately do NOT attach `with_usage_instrumentation` to the
/// `BufferSender`. In production, `TopologyBuilder::build` skips that call for
/// disk-v2 because the stage `provides_instrumentation()` itself via the ledger.
/// This test reflects that production wiring: filter-drop accounting goes
/// through `Ledger::track_dropped`, and `usage` (returned by the helper) IS the
/// ledger's handle.
#[tokio::test]
async fn filter_drops_are_reported_as_unintentional_buffer_drops() {
    let _a = install_tracing_helpers();

    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let (writer, _reader, _ledger, usage) =
                create_default_buffer_v2_with_usage::<_, FilterableBatch>(data_dir).await;
            let mut sender = BufferSender::new(SenderAdapter::from(writer), WhenFull::Block);

            // 10 events arrive, filter keeps 3. Flush after each send so the
            // ledger's `track_write` actually reaches the usage handle (it only
            // fires when buffered writes are flushed to disk).
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
            sender.flush().await.expect("flush should succeed");

            let snapshot = usage.snapshot();
            assert_eq!(
                snapshot.received_event_count, 10,
                "received counts both the 7 filter-dropped events (via track_dropped) \
                 and the 3 events flushed to disk (via track_write)",
            );
            assert_eq!(
                snapshot.dropped_event_count, 7,
                "filter drops show up under the disk-v2 stage's unintentional dropped count \
                 so buffer_size stays consistent (received - sent - dropped = 3 queued)",
            );
            assert_eq!(
                snapshot.dropped_event_count_intentional, 0,
                "no buffer-fullness drops here",
            );

            // 5 events arrive, filter drops them all (nothing reaches disk).
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
                "fully-filtered item still bumps received via track_dropped",
            );
            assert_eq!(
                snapshot.dropped_event_count, 12,
                "all 5 events from the fully-filtered item are reported as unintentional drops",
            );
        }
    })
    .await;
}

/// Under `WhenFull::Overflow`, an item the base stage cannot encode must reach the
/// overflow stage *intact* while the base stage still has room.
///
/// This is the near-full half of the state-independence guarantee: the routing decision
/// is made from the item alone, so it does not matter how full the base stage is.
#[tokio::test]
async fn unencodable_item_overflows_intact_when_base_has_room() {
    let _a = install_tracing_helpers();

    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let (writer, _reader, _ledger, _usage) =
                create_default_buffer_v2_with_usage::<_, FilterableBatch>(data_dir).await;

            let (overflow_tx, mut overflow_rx) = limited(
                MemoryBufferSize::MaxEvents(NonZeroUsize::new(100).unwrap()),
                None,
                None,
            );
            let mut sender = BufferSender::with_overflow(
                SenderAdapter::from(writer),
                BufferSender::new(SenderAdapter::from(overflow_tx), WhenFull::Block),
            );

            // The disk stage is empty, so it has ample room. The item is wholly
            // unencodable, so it must still be handed to the overflow stage rather than
            // filtered away.
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

            let received = overflow_rx.next().await.expect("item must reach overflow");
            assert_eq!(
                received,
                FilterableBatch {
                    events: 5,
                    post_filter: 0,
                },
                "overflow must receive the item intact, with no sub-items pruned",
            );
        }
    })
    .await;
}

/// The already-full half of the same guarantee: an unencodable item reaches the overflow
/// stage intact when the base stage is at capacity, and by the same route.
///
/// The base here is an in-memory stage rather than disk, because it can be driven to a
/// known-full state deterministically. Reliably forcing disk-v2's `is_buffer_full()` to
/// `true` under the minimum-size config requires careful record/buffer size tuning, since
/// `can_write_record` generally short-circuits writes before `total_buffer_size` reaches
/// `max_buffer_size`. That substitution is sound for this property: the unencodable-item
/// decision is taken in `BufferSender` from `Bufferable::is_fully_encodable` before any
/// backend is consulted, so the base stage's type and occupancy are both immaterial. That
/// is precisely the invariant being asserted.
#[tokio::test]
async fn unencodable_item_overflows_intact_when_base_is_full() {
    let _a = install_tracing_helpers();

    let (base_tx, _base_rx) = limited::<FilterableBatch>(
        MemoryBufferSize::MaxEvents(NonZeroUsize::new(1).unwrap()),
        None,
        None,
    );
    let (overflow_tx, mut overflow_rx) = limited(
        MemoryBufferSize::MaxEvents(NonZeroUsize::new(100).unwrap()),
        None,
        None,
    );

    let mut sender = BufferSender::with_overflow(
        SenderAdapter::from(base_tx),
        BufferSender::new(SenderAdapter::from(overflow_tx), WhenFull::Block),
    );

    // Fill the base stage so any further send would be rejected for fullness.
    sender
        .send(
            FilterableBatch {
                events: 1,
                post_filter: 1,
            },
            None,
        )
        .await
        .expect("first send should occupy the base stage");

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

    let received = overflow_rx.next().await.expect("item must reach overflow");
    assert_eq!(
        received,
        FilterableBatch {
            events: 5,
            post_filter: 0,
        },
        "a full base stage must not change how an unencodable item is routed",
    );
}

/// A base stage without a wire-format constraint must keep an unencodable item rather than
/// pass it to the overflow stage.
///
/// The encodability check is a property of the *base* stage, not of the item alone. In a
/// `memory -> disk` overflow topology the memory stage can hold an arbitrarily nested item
/// safely, so diverting it past memory would hand an item the base could have kept to a
/// stage that has no choice but to drop it. This is the mirror image of the
/// `disk -> memory` cases above and guards against reintroducing that assumption.
#[tokio::test]
async fn unencodable_item_stays_in_base_when_base_has_no_encoding_constraint() {
    let _a = install_tracing_helpers();

    let (base_tx, mut base_rx) = limited::<FilterableBatch>(
        MemoryBufferSize::MaxEvents(NonZeroUsize::new(100).unwrap()),
        None,
        None,
    );
    let (overflow_tx, mut overflow_rx) = limited(
        MemoryBufferSize::MaxEvents(NonZeroUsize::new(100).unwrap()),
        None,
        None,
    );

    let mut sender = BufferSender::with_overflow(
        SenderAdapter::from(base_tx),
        BufferSender::new(SenderAdapter::from(overflow_tx), WhenFull::Block),
    );

    // The base is in-memory and empty, so it can hold this item despite the item being
    // unencodable for a protobuf-backed stage.
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

    let received = timeout(Duration::from_secs(5), base_rx.next())
        .await
        .expect("item must stay in the base stage rather than be diverted to overflow")
        .expect("base stage should yield the item");
    assert_eq!(
        received,
        FilterableBatch {
            events: 5,
            post_filter: 0,
        },
        "an unconstrained base stage must keep the item intact",
    );
    assert!(
        timeout(Duration::from_millis(50), overflow_rx.next())
            .await
            .is_err(),
        "the overflow stage must not be involved when the base can hold the item",
    );
}
