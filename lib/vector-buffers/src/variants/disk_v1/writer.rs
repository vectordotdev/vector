use std::{
    fmt,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
};

use bytes::BytesMut;
use leveldb::database::{
    batch::{Batch, Writebatch},
    options::WriteOptions,
    Database,
};
use tokio::sync::Notify;

use super::Key;
use crate::{buffer_usage_data::BufferUsageHandle, Bufferable};

/// The writer side of N to 1 channel through leveldb.
pub struct Writer<T>
where
    T: Bufferable,
{
    /// Leveldb database.
    /// Shared with Reader.
    pub(crate) db: Option<Arc<Database<Key>>>,
    /// First unused key/index.
    /// Shared with other Writers.
    pub(crate) offset: Arc<AtomicUsize>,
    /// Writers notify Reader through this Waker.
    /// Shared with Reader.
    pub(crate) read_waker: Arc<Notify>,
    /// Reader notifies Writers through this Waker.
    /// Shared with Reader.
    pub(crate) write_waker: Arc<Notify>,
    /// Batched writes.
    pub(crate) writebatch: Writebatch<Key>,
    /// Events in batch.
    pub(crate) batch_size: usize,
    /// Max size of unread events in bytes.
    pub(crate) max_size: u64,
    /// Size of unread events in bytes.
    /// Shared with Reader.
    pub(crate) current_size: Arc<AtomicU64>,
    /// Buffer for internal use.
    pub(crate) slot: Option<T>,
    /// Buffer usage data.
    pub(crate) usage_handle: BufferUsageHandle,
}

// Writebatch isn't Send, but the leveldb docs explicitly say that it's okay to
// share across threads
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<T> Send for Writer<T> where T: Bufferable {}

impl<T> Clone for Writer<T>
where
    T: Bufferable,
{
    fn clone(&self) -> Self {
        Self {
            db: self.db.as_ref().map(Arc::clone),
            offset: Arc::clone(&self.offset),
            read_waker: Arc::clone(&self.read_waker),
            write_waker: Arc::clone(&self.write_waker),
            writebatch: Writebatch::new(),
            batch_size: 0,
            max_size: self.max_size,
            current_size: Arc::clone(&self.current_size),
            slot: None,
            usage_handle: self.usage_handle.clone(),
        }
    }
}

impl<T> Writer<T>
where
    T: Bufferable,
{
    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    pub async fn send(&mut self, mut item: T) {
        loop {
            match self.try_send(item) {
                None => break,
                Some(old_item) => {
                    item = old_item;
                    self.write_waker.notified().await;
                }
            }
        }
    }

    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    pub fn try_send(&mut self, item: T) -> Option<T> {
        let event_len = item.event_count();

        // Encode the item.
        let mut buffer: BytesMut = BytesMut::with_capacity(64);
        T::encode(item, &mut buffer).unwrap();
        let event_size = buffer.len() as u64;

        // Now that we have the encoded size, see if we can fit this item in the buffer given the
        // current size.  If it won't fit, then give back the item so we can hold on to it and wait
        // for the reader to make progress.
        if self.current_size.fetch_add(event_size, Ordering::Relaxed) + (event_size / 2)
            > self.max_size
        {
            self.current_size.fetch_sub(event_size, Ordering::Relaxed);

            return Some(T::decode(T::get_metadata(), buffer).unwrap());
        }

        // Generate the key for the item, and increment the offset by the number of events in the
        // item, which lets us look at the keys present in tehe buffer during initialization and
        // quickly calculate the total number of events in the buffer.
        let key = self.offset.fetch_add(event_len, Ordering::Relaxed);

        self.writebatch.put(Key(key), &buffer);
        self.batch_size += 1;

        if self.batch_size >= 100 {
            self.flush();
        }

        self.usage_handle
            .increment_received_event_count_and_byte_size(event_len as u64, event_size);

        None
    }

    #[cfg_attr(test, instrument(skip(self), level = "debug"))]
    pub fn flush(&mut self) {
        // This doesn't write all the way through to disk and doesn't need to be
        // wrapped with `blocking`. (It does get written to a memory mapped
        // table that will be flushed even in the case of a process crash.)
        self.write_batch();
    }

    #[cfg_attr(test, instrument(skip(self), level = "trace"))]
    fn write_batch(&mut self) {
        self.db
            .as_mut()
            .unwrap()
            .write(WriteOptions::new(), &self.writebatch)
            .unwrap();
        self.writebatch = Writebatch::new();
        self.batch_size = 0;

        self.read_waker.notify_one();
    }
}

impl<T> Drop for Writer<T>
where
    T: Bufferable,
{
    fn drop(&mut self) {
        if let Some(event) = self.slot.take() {
            // This can happen if poll_close wasn't called which is a bug
            // or we are unwinding the stack.
            //
            // We can't be picky at the moment so we will allow
            // for the buffer to exceed configured limit.
            self.max_size = u64::MAX;
            assert!(self.try_send(event).is_none());
        }

        self.flush();

        // We drop the database Arc before notifying reader to avoid the case
        // where we notify the reader, the reader reacts and checks
        // Arc::strong_count to be > 1 and then we drop the Arc which would
        // cause a stall.
        self.db.take();

        // We need to wake up the reader so it can return None if there are no
        // more writers
        self.read_waker.notify_waiters();
    }
}

impl<T: Bufferable> fmt::Debug for Writer<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Writer")
            .field("offset", &self.offset)
            .field("read_waker", &self.read_waker)
            .field("write_waker", &self.write_waker)
            .field("batch_size", &self.batch_size)
            .field("max_size", &self.max_size)
            .field("current_size", &self.current_size)
            .field("slot", &self.slot)
            .field("usage_handle", &self.usage_handle)
            .finish()
    }
}
