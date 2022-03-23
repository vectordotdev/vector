use std::{
    collections::VecDeque,
    fmt,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
    time::Duration,
};

use bytes::Bytes;
use futures::{task::AtomicWaker, Stream};
use leveldb::database::{
    batch::{Batch, Writebatch},
    compaction::Compaction,
    iterator::{Iterable, LevelDBIterator},
    options::{ReadOptions, WriteOptions},
    Database,
};
use parking_lot::Mutex;
use tokio::{task::JoinHandle, time::Instant};

use super::Key;
use crate::{
    buffer_usage_data::BufferUsageHandle,
    topology::acks::{EligibleMarker, EligibleMarkerLength, MarkerError, OrderedAcknowledgements},
    Bufferable,
};

/// How much time needs to pass between compaction to trigger new one.
const MIN_TIME_UNCOMPACTED: Duration = Duration::from_secs(60);

/// Minimal size of uncompacted for which a compaction can be triggered.
const MIN_UNCOMPACTED_SIZE: u64 = 4 * 1024 * 1024;

/// How often we flush deletes to the database.
pub const FLUSH_INTERVAL: Duration = Duration::from_millis(250);

/// The reader side of N to 1 channel through leveldb.
///
/// Reader maintains/manages events thorugh several stages.
/// Unread -> Read -> Deleted -> Compacted
///
/// So the disk buffer (indices/keys) is separated into following regions.
/// |--Compacted--|--Deleted--|--Read--|--Unread
///  ^             ^   ^       ^        ^
///  |             |   |-acked-|        |
///  0   `compacted_offset`    |        |
///                     `delete_offset` |
///                                `read_offset`
pub struct Reader<T> {
    /// Leveldb database.
    /// Shared with Writers.
    pub(crate) db: Arc<Database<Key>>,
    /// First unread key
    pub(crate) read_offset: usize,
    /// First uncompacted key
    pub(crate) compacted_offset: usize,
    /// First not deleted key
    pub(crate) delete_offset: usize,
    /// Reader is notified by Writers through this Waker.
    /// Shared with Writers.
    pub(crate) write_notifier: Arc<AtomicWaker>,
    /// Writers blocked by disk being full.
    /// Shared with Writers.
    pub(crate) blocked_write_tasks: Arc<Mutex<Vec<Waker>>>,
    /// Size of unread events in bytes.
    /// Shared with Writers.
    pub(crate) current_size: Arc<AtomicU64>,
    /// Number of oldest read, not deleted, events that have been acked by the consumer.
    /// Shared with consumer.
    pub(crate) ack_counter: Arc<AtomicUsize>,
    /// Size of deleted, not compacted, events in bytes.
    pub(crate) uncompacted_size: u64,
    /// Keys of unacked events.
    pub(crate) record_acks: OrderedAcknowledgements<usize, usize>,
    /// Buffer for internal use.
    pub(crate) buffer: VecDeque<(Key, Vec<u8>)>,
    /// Limit on uncompacted_size after which we trigger compaction.
    pub(crate) max_uncompacted_size: u64,
    /// Last time that compaction was triggered.
    pub(crate) last_compaction: Instant,
    /// Last time that delete flush was triggered.
    pub(crate) last_flush: Instant,
    // Pending read from the LevelDB datasbase
    pub(crate) pending_read: Option<JoinHandle<Vec<(Key, Vec<u8>)>>>,
    // Buffer usage data.
    pub(crate) usage_handle: BufferUsageHandle,
    pub(crate) phantom: PhantomData<T>,
}

impl<T> Stream for Reader<T>
where
    T: Bufferable,
{
    type Item = T;

    #[cfg_attr(test, instrument(skip(self, cx), level = "debug"))]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // If there's no value at or beyond `read_offset`, we return `Poll::Pending` and rely on
        // `Writer` using `self.write_notifier` to wake this task up after the next write.
        this.write_notifier.register(cx.waker());

        // Check for any pending acknowledgements which may make a read eligible to finally be
        // deleted from the buffer entirely.
        this.try_flush();

        if this.buffer.is_empty() {
            // This will usually complete instantly, but in the case of a large
            // queue (or a fresh launch of the app), this will have to go to
            // disk.
            loop {
                match this.pending_read.take() {
                    None => {
                        // We have no pending read in-flight, so queue one up.
                        let db = Arc::clone(&this.db);
                        let read_offset = this.read_offset;
                        let handle = tokio::task::spawn_blocking(move || {
                            db.iter(ReadOptions::new())
                                .from(&Key(read_offset))
                                .take(1000)
                                .collect::<Vec<_>>()
                        });

                        // Store the handle, and let the loop come back around.
                        this.pending_read = Some(handle);
                    }
                    Some(mut handle) => match Pin::new(&mut handle).poll(cx) {
                        Poll::Ready(r) => {
                            match r {
                                Ok(items) => {
                                    trace!(batch_size = items.len(), "LevelDB read completed.");
                                    this.buffer.extend(items);
                                }
                                Err(error) => error!(%error, "Error during read."),
                            }

                            this.pending_read = None;

                            break;
                        }
                        Poll::Pending => {
                            this.pending_read = Some(handle);
                            return Poll::Pending;
                        }
                    },
                }
            }

            // If we've broke out of the loop, our read has completed.
            this.pending_read = None;
        }

        if let Some((key, item_bytes, decode_result)) = this.decode_next_record() {
            trace!(?key, item_bytes, "Got record decode attempt.");
            match decode_result {
                Ok(item) => {
                    this.track_unacked_read(key.0, Some(item.event_count()), item_bytes);
                    Poll::Ready(Some(item))
                }
                Err(error) => {
                    error!(%error, "Error deserializing event.");

                    this.track_unacked_read(key.0, None, item_bytes);
                    Pin::new(this).poll_next(cx)
                }
            }
        } else if Arc::strong_count(&this.db) == 1 {
            // There are no writers left
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

impl<T> Drop for Reader<T> {
    fn drop(&mut self) {
        self.flush();
    }
}

impl<T: Bufferable> Reader<T> {
    /// Decodes the next buffered record, if one is available.
    #[cfg_attr(test, instrument(skip(self), level = "trace"))]
    fn decode_next_record(&mut self) -> Option<(Key, usize, Result<T, T::DecodeError>)> {
        self.buffer.pop_front().map(|(key, value)| {
            let item_bytes = value.len();
            let decode_buf = Bytes::from(value);
            (key, item_bytes, T::decode(T::get_metadata(), decode_buf))
        })
    }
}

impl<T> Reader<T> {
    /// Gets the current size of the buffer.
    fn get_buffer_size(&self) -> u64 {
        self.current_size.load(Ordering::Acquire)
    }

    /// Decreases the buffer size by the given amount.
    ///
    /// Returns the new size of the buffer.
    fn decrease_buffer_size(&mut self, amount: u64) -> u64 {
        self.uncompacted_size += amount;
        self.current_size.fetch_sub(amount, Ordering::Release) - amount
    }

    /// Tracks a read for pending deletion.
    ///
    /// Once all items of a read have been acknowledged, it will become eligible to be deleted.
    /// Additionally, reads which failed to decode will become eligible for deletion as soon as the
    /// next read occurs.
    fn track_unacked_read(&mut self, key: usize, event_count: Option<usize>, item_bytes: usize) {
        self.read_offset = match event_count {
            // We adjust our read offset to be 1 ahead of this key, because we grab records in a
            // "get the next N records that come after key K", so we only need the offset to be
            // right ahead of this key... regardless of whether or not there _is_ something valid at
            // K+1 or the next key is actually K+7, etc.
            None => key + 1,
            // Adjust our read offset based on the items within the read, as we rely on
            // the keys to track the number of _effective_ items (sum of "event count" from each item)
            // in the buffer using simple arithmetic between the first and last keys in the buffer.
            Some(len) => key + len,
        };

        // Now store a pending delete marker that will eventually be drained in our `try_flush` routine.
        if let Err(me) = self
            .record_acks
            .add_marker(key, event_count, Some(item_bytes))
        {
            match me {
                MarkerError::MonotonicityViolation => {
                    panic!("record ID monotonicity violation detected; this is a serious bug")
                }
            }
        }
    }

    /// Attempt to flush any pending deletes to the database.
    ///
    /// Flushes are driven based on elapsed time to coalsece operations that require modifying the database.
    #[cfg_attr(test, instrument(skip(self), level = "trace"))]
    fn try_flush(&mut self) {
        // Don't flush unless we've overrun our flush interval.
        if self.last_flush.elapsed() < FLUSH_INTERVAL {
            trace!("Last flush was too recent to run again.");
            return;
        }
        self.last_flush = Instant::now();

        self.flush();
    }

    /// Flushes all eligible deletes to the database.
    #[cfg_attr(test, instrument(skip(self), level = "trace"))]
    fn flush(&mut self) {
        debug!("Running flush.");

        // Consume any pending acknowledgements.
        let pending_acks = self.ack_counter.swap(0, Ordering::Relaxed);
        if pending_acks > 0 {
            self.record_acks.add_acknowledgements(pending_acks);
        }

        // See if any pending deletes actually qualify for deletion, and if so, capture them and
        // actually execute a batch delete operation.
        let mut delete_batch = Writebatch::new();
        let mut total_records = 0;
        let mut total_events = 0;
        let mut total_bytes = 0;
        while let Some(marker) = self.record_acks.get_next_eligible_marker() {
            let EligibleMarker { id: key, len, data } = marker;

            let event_count = match len {
                EligibleMarkerLength::Known(len) | EligibleMarkerLength::Assumed(len) => len,
            };
            let item_bytes = data.unwrap_or(0);

            // Add this key to our delete batch.
            delete_batch.delete(Key(key));

            // Adjust our delete offset, and if need be, the amount of remaining acknowledgements.
            //
            // We adjust the delete offset/remaining acks here so that the next call to
            // `get_next_eligible_delete` has updated offsets so we can optimally drain as many
            // eligible deletes as possible in one go.
            self.delete_offset = key.wrapping_add(event_count);

            total_records += 1;
            total_events += event_count;
            total_bytes += item_bytes;
        }

        // If we actually found anything that was ready to be deleted, execute our delete batch
        // and update our buffer usage metrics.
        if total_records > 0 {
            debug!(
                delete_offset = self.delete_offset,
                "Deleting {} records from buffer: {} items, {} bytes.",
                total_records,
                total_events,
                total_bytes
            );
            self.db.write(WriteOptions::new(), &delete_batch).unwrap();

            assert!(
                self.delete_offset <= self.read_offset,
                "tried to ack beyond read offset"
            );

            // Update our buffer size and buffer usage metrics.
            self.decrease_buffer_size(total_bytes as u64);
            self.usage_handle
                .increment_sent_event_count_and_byte_size(total_events as u64, total_bytes as u64);

            // Now that we've actually deleted some items, notify any blocked writers that progress
            // has been made so they can continue writing.
            for task in self.blocked_write_tasks.lock().drain(..) {
                task.wake();
            }
        }

        // Attempt to run a compaction if we've met the criteria to trigger one.
        self.try_compact();
    }

    /// Attempt to trigger a compaction.
    ///
    /// Compaction will only be triggered if certain criteria are met, which are specifically
    /// documented below.
    pub(super) fn try_compact(&mut self) {
        // Compaction can be triggered in two ways:
        //  1. When size of uncompacted is a percentage of total allowed size.
        //     Managed by MAX_UNCOMPACTED. This is to limit the size of disk buffer
        //     under configured max size.
        let max_trigger = self.uncompacted_size > self.max_uncompacted_size;
        //  2. When the size of uncompacted buffer is larger than unread buffer.
        //     If the sink is able to keep up with the sources, this will trigger
        //     with MIN_TIME_UNCOMPACTED interval. And if it's not keeping up,
        //     this won't trigger hence it won't slow it down, which will allow it
        //     to grow until it either hits max_uncompacted_size or manages to catch up.
        //     This is to keep the size of the disk buffer low in idle and up to date
        //     cases.
        let timed_trigger = self.last_compaction.elapsed() >= MIN_TIME_UNCOMPACTED
            && self.uncompacted_size > self.get_buffer_size();

        // Basic requirement to avoid leaving ldb files behind.
        // See:
        // Vector  https://github.com/timberio/vector/issues/7425#issuecomment-849522738
        // leveldb https://github.com/google/leveldb/issues/783
        //         https://github.com/syndtr/goleveldb/issues/166
        let min_size = self.uncompacted_size >= MIN_UNCOMPACTED_SIZE;

        if min_size && (max_trigger || timed_trigger) {
            self.uncompacted_size = 0;

            debug!("Compacting disk buffer.");
            self.db
                .compact(&Key(self.compacted_offset), &Key(self.delete_offset));

            self.compacted_offset = self.delete_offset;
            self.last_compaction = Instant::now();
        }
    }
}

impl<T: Bufferable> fmt::Debug for Reader<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reader")
            .field("read_offset", &self.read_offset)
            .field("compacted_offset", &self.compacted_offset)
            .field("delete_offset", &self.delete_offset)
            .field("write_notifier", &self.write_notifier)
            .field("blocked_write_tasks", &self.blocked_write_tasks)
            .field("current_size", &self.current_size)
            .field("ack_counter", &self.ack_counter)
            .field("uncompacted_size", &self.uncompacted_size)
            .field("record_acks", &self.record_acks)
            .field("buffer", &self.buffer)
            .field("max_uncompacted_size", &self.max_uncompacted_size)
            .field("last_compaction", &self.last_compaction)
            .field("last_flush", &self.last_flush)
            .field("pending_read", &self.pending_read)
            .field("usage_handle", &self.usage_handle)
            .field("phantom", &self.phantom)
            .finish()
    }
}
