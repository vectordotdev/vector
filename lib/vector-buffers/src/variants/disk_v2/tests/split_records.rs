//! Splitting items whose encoded form exceeds the maximum record size.
//!
//! The disk-v2 writer stores an item as one record and drops the whole record when it encodes to
//! more than the maximum record size. A batch of events that is only too large *together* used to
//! be lost in full; the sender now splits it via `Bufferable::split_in_half` so that only an event
//! too large on its own is dropped.

use std::{error, fmt, time::Duration};

use bytes::{Buf, BufMut};
use tokio::time::timeout;
use vector_common::{
    byte_size_of::ByteSizeOf,
    finalization::{
        AddBatchNotifier, BatchNotifier, EventFinalizers, Finalizable, MergeFinalizable,
    },
};

use super::{
    create_buffer_v2_with_data_file_count_limit, create_buffer_v2_with_max_record_size_and_usage,
    get_corrected_max_record_size, get_minimum_data_file_size_for_record_payload, read_next_some,
};
use crate::{
    Bufferable, EventCount, WhenFull,
    encoding::FixedEncodable,
    test::{install_tracing_helpers, with_temp_dir},
    topology::channel::{BufferSender, SenderAdapter},
    variants::disk_v2::{BufferReader, Filesystem, TryWriteOutcome},
};

/// Bytes of payload carried by a regular item in these tests.
const ITEM_LEN: u32 = 100;

/// A batch of numbered items, each carrying `len` bytes of payload. Its in-memory size equals its
/// encoded size, so the sender's size-based split decisions line up with what the writer accepts.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Batch {
    items: Vec<(u32, u32)>,
}

impl Batch {
    fn of(lens: &[u32]) -> Self {
        Self {
            items: (0u32..).zip(lens.iter().copied()).collect(),
        }
    }

    fn ids(&self) -> Vec<u32> {
        self.items.iter().map(|(id, _)| *id).collect()
    }
}

impl AddBatchNotifier for Batch {
    fn add_batch_notifier(&mut self, batch: BatchNotifier) {
        drop(batch);
    }
}

// This fixture carries no finalizers; splitting moves each event's own finalizers along with it,
// which `EventArray` gets for free because they live in the events' metadata.
impl Finalizable for Batch {
    fn take_finalizers(&mut self) -> EventFinalizers {
        EventFinalizers::default()
    }
}

impl MergeFinalizable for Batch {
    fn merge_finalizers(&mut self, finalizers: EventFinalizers) {
        drop(finalizers);
    }
}

impl ByteSizeOf for Batch {
    fn size_of(&self) -> usize {
        self.encoded_size().expect("always known")
    }

    fn allocated_bytes(&self) -> usize {
        0
    }
}

impl EventCount for Batch {
    fn event_count(&self) -> usize {
        self.items.len()
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

impl FixedEncodable for Batch {
    type EncodeError = CodecError;
    type DecodeError = CodecError;

    fn encode<B: BufMut>(self, buf: &mut B) -> Result<(), Self::EncodeError> {
        if buf.remaining_mut() < self.encoded_size().expect("always known") {
            return Err(CodecError);
        }
        buf.put_u32(u32::try_from(self.items.len()).unwrap());
        for (id, len) in self.items {
            buf.put_u32(id);
            buf.put_u32(len);
            buf.put_bytes(0xAA, len as usize);
        }
        Ok(())
    }

    fn decode<B: Buf>(mut buf: B) -> Result<Self, Self::DecodeError> {
        let count = buf.get_u32();
        let items = (0..count)
            .map(|_| {
                let id = buf.get_u32();
                let len = buf.get_u32();
                buf.advance(len as usize);
                (id, len)
            })
            .collect();
        Ok(Self { items })
    }

    fn encoded_size(&self) -> Option<usize> {
        Some(
            4 + self
                .items
                .iter()
                .map(|(_, len)| 8 + *len as usize)
                .sum::<usize>(),
        )
    }
}

impl Bufferable for Batch {
    fn split_in_half(mut self) -> Result<(Self, Self), Self> {
        if self.items.len() < 2 {
            return Err(self);
        }
        let back = self.items.split_off(self.items.len() / 2);
        Ok((self, Self { items: back }))
    }

    fn rejoin(mut self, back: Self) -> Self {
        self.items.extend(back.items);
        self
    }
}

/// Reads records until `expected` items have arrived, returning their ids in order.
async fn read_ids<FS>(reader: &mut BufferReader<Batch, FS>, expected: usize) -> Vec<u32>
where
    FS: Filesystem,
    FS::File: Unpin,
{
    let mut ids = Vec::new();
    while ids.len() < expected {
        let record = timeout(Duration::from_secs(5), read_next_some(reader))
            .await
            .expect("record should be readable");
        ids.extend(record.ids());
    }
    ids
}

/// An item that is over the record limit only because of how many events it holds is written as
/// several records, and every event reaches the reader in order.
#[tokio::test]
async fn item_over_the_record_limit_is_written_as_several_records() {
    let _a = install_tracing_helpers();

    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Two regular items fit in a record; six do not.
            let max_record_size = get_corrected_max_record_size(&Batch::of(&[ITEM_LEN, ITEM_LEN]));
            let (writer, mut reader, _ledger, usage) =
                create_buffer_v2_with_max_record_size_and_usage::<_, Batch>(
                    data_dir,
                    max_record_size,
                )
                .await;
            let mut sender = BufferSender::new(SenderAdapter::from(writer), WhenFull::Block);

            sender
                .send(Batch::of(&[ITEM_LEN; 6]), None)
                .await
                .expect("send should succeed");
            sender.flush().await.expect("flush should succeed");

            assert_eq!(read_ids(&mut reader, 6).await, vec![0, 1, 2, 3, 4, 5]);
            let snapshot = usage.snapshot();
            assert_eq!(snapshot.received_event_count, 6);
            assert_eq!(
                snapshot.dropped_event_count, 0,
                "splitting means no event is lost to the record limit",
            );
        }
    })
    .await;
}

/// When one event is over the record limit on its own, only that event is dropped; its siblings in
/// the same item are written.
#[tokio::test]
async fn only_the_event_over_the_record_limit_is_dropped() {
    let _a = install_tracing_helpers();

    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let max_record_size = get_corrected_max_record_size(&Batch::of(&[ITEM_LEN, ITEM_LEN]));
            let (writer, mut reader, _ledger, usage) =
                create_buffer_v2_with_max_record_size_and_usage::<_, Batch>(
                    data_dir,
                    max_record_size,
                )
                .await;
            let mut sender = BufferSender::new(SenderAdapter::from(writer), WhenFull::Block);

            // Item 1 alone is ten times the record limit.
            let oversized = u32::try_from(max_record_size * 10).unwrap();
            sender
                .send(Batch::of(&[ITEM_LEN, oversized, ITEM_LEN, ITEM_LEN]), None)
                .await
                .expect("send should succeed");
            sender.flush().await.expect("flush should succeed");

            assert_eq!(read_ids(&mut reader, 3).await, vec![0, 2, 3]);
            let snapshot = usage.snapshot();
            assert_eq!(
                snapshot.dropped_event_count, 1,
                "only the event that cannot fit a record on its own is dropped",
            );
        }
    })
    .await;
}

/// A non-blocking send that runs out of room partway through a split item writes the pieces that
/// fit and hands back the rest as one item, in order, so drop-newest or overflow only ever sees
/// what is not on disk.
#[tokio::test]
async fn try_send_returns_the_unwritten_rest_of_a_split_item() {
    let _a = install_tracing_helpers();

    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Each data file holds exactly one two-item record, and the buffer only a few files.
            let max_data_file_size =
                get_minimum_data_file_size_for_record_payload(&Batch::of(&[ITEM_LEN, ITEM_LEN]));
            let (writer, mut reader, _ledger) = create_buffer_v2_with_data_file_count_limit::<
                _,
                Batch,
            >(data_dir, max_data_file_size, 3)
            .await;
            let mut adapter = SenderAdapter::from(writer);

            let outcome = adapter
                .try_send(Batch::of(&[ITEM_LEN; 16]))
                .await
                .expect("try_send should not fail");
            let TryWriteOutcome::Full(rest) = outcome else {
                panic!("sixteen items cannot all fit in a three-file buffer");
            };
            adapter.flush().await.expect("flush should succeed");

            let rest_ids = rest.ids();
            let first_unwritten = *rest_ids.first().expect("the rest is never empty");
            assert!(first_unwritten > 0, "some pieces should have been written");
            assert_eq!(
                rest_ids,
                (first_unwritten..16).collect::<Vec<_>>(),
                "the rest is every unwritten item, rejoined in order",
            );
            assert_eq!(
                read_ids(&mut reader, first_unwritten as usize).await,
                (0..first_unwritten).collect::<Vec<_>>(),
                "the written pieces are the items before the rest, in order",
            );
        }
    })
    .await;
}
