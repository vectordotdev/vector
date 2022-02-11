use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll, Waker},
};

use bytes::BytesMut;
use futures::{task::AtomicWaker, Sink};
use leveldb::database::{
    batch::{Batch, Writebatch},
    options::WriteOptions,
    Database,
};

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
    pub(crate) write_notifier: Arc<AtomicWaker>,
    /// Waiting queue for when the disk is full.
    /// Shared with Reader.
    pub(crate) blocked_write_tasks: Arc<Mutex<Vec<Waker>>>,
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
            write_notifier: Arc::clone(&self.write_notifier),
            blocked_write_tasks: Arc::clone(&self.blocked_write_tasks),
            writebatch: Writebatch::new(),
            batch_size: 0,
            max_size: self.max_size,
            current_size: Arc::clone(&self.current_size),
            slot: None,
            usage_handle: self.usage_handle.clone(),
        }
    }
}

impl<T> Sink<T> for Writer<T>
where
    T: Bufferable,
{
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.slot.is_none() {
            Poll::Ready(Ok(()))
        } else {
            // Assumes that flush will only succeed if it has also emptied the
            // slot, hence we don't need to recheck if the slot is empty.
            self.poll_flush(cx)
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        if let Some(event) = self.try_send(item) {
            debug_assert!(self.slot.is_none());
            self.slot = Some(event);
        }
        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if let Some(event) = self.slot.take() {
            if let Some(event) = self.try_send(event) {
                self.slot = Some(event);

                self.blocked_write_tasks
                    .lock()
                    .unwrap()
                    .push(cx.waker().clone());

                if self.current_size.load(Ordering::Acquire) == 0 {
                    // This is a rare case where the reader managed to consume
                    // and delete all events in the buffer. In this case there
                    // is a scenario where the reader won't be polled again
                    // hence this sink will never be notified again so this will
                    // stall.
                    //
                    // To avoid this we notify the reader to notify this writer.
                    self.write_notifier.wake();
                }

                return Poll::Pending;
            }
        }

        self.flush();

        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(cx)
    }
}

impl<T> Writer<T>
where
    T: Bufferable,
{
    fn try_send(&mut self, event: T) -> Option<T> {
        let mut buffer: BytesMut = BytesMut::with_capacity(64);
        T::encode(event, &mut buffer).unwrap();
        let event_size = buffer.len() as u64;

        if self.current_size.fetch_add(event_size, Ordering::Relaxed) + (event_size / 2)
            > self.max_size
        {
            self.current_size.fetch_sub(event_size, Ordering::Relaxed);

            self.flush();

            return Some(T::decode(T::get_metadata(), buffer).unwrap());
        }

        let key = self.offset.fetch_add(1, Ordering::Relaxed);

        self.writebatch.put(Key(key), &buffer);
        self.batch_size += 1;

        if self.batch_size >= 100 {
            self.flush();
        }

        self.usage_handle
            .increment_received_event_count_and_byte_size(1, event_size);

        None
    }

    fn flush(&mut self) {
        // This doesn't write all the way through to disk and doesn't need to be
        // wrapped with `blocking`. (It does get written to a memory mapped
        // table that will be flushed even in the case of a process crash.)
        if self.batch_size > 0 {
            self.write_batch();
        }
    }

    fn write_batch(&mut self) {
        self.db
            .as_mut()
            .unwrap()
            .write(WriteOptions::new(), &self.writebatch)
            .unwrap();
        self.writebatch = Writebatch::new();
        self.batch_size = 0;

        self.write_notifier.wake();
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
        self.write_notifier.wake();
    }
}
