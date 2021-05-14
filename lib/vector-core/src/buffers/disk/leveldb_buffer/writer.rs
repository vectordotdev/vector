use super::Key;
use bytes::Bytes;
use futures::{task::AtomicWaker, Sink};
use leveldb::database::{
    batch::{Batch, Writebatch},
    options::WriteOptions,
    Database,
};
use std::convert::{TryFrom, TryInto};
use std::fmt::Debug;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::task::{Context, Poll, Waker};

pub struct Writer<T>
where
    T: Send + Sync + Unpin + TryInto<Bytes> + TryFrom<Bytes>,
    <T as TryInto<bytes::Bytes>>::Error: Debug,
    <T as std::convert::TryFrom<bytes::Bytes>>::Error: Debug,
{
    pub(crate) db: Option<Arc<Database<Key>>>,
    pub(crate) offset: Arc<AtomicUsize>,
    pub(crate) write_notifier: Arc<AtomicWaker>,
    pub(crate) blocked_write_tasks: Arc<Mutex<Vec<Waker>>>,
    pub(crate) writebatch: Writebatch<Key>,
    pub(crate) batch_size: usize,
    pub(crate) max_size: usize,
    pub(crate) current_size: Arc<AtomicUsize>,
    pub(crate) slot: Option<T>,
}

// Writebatch isn't Send + Send, but the leveldb docs explicitly say that it's
// okay to share across threads
unsafe impl<T> Send for Writer<T>
where
    T: Send + Sync + Unpin + TryInto<Bytes> + TryFrom<Bytes>,
    <T as TryInto<bytes::Bytes>>::Error: Debug,
    <T as TryFrom<bytes::Bytes>>::Error: Debug,
{
}
unsafe impl<T> Sync for Writer<T>
where
    T: Send + Sync + Unpin + TryInto<Bytes> + TryFrom<Bytes>,
    <T as TryInto<bytes::Bytes>>::Error: Debug,
    <T as TryFrom<bytes::Bytes>>::Error: Debug,
{
}

impl<T> Clone for Writer<T>
where
    T: Send + Sync + Unpin + TryInto<Bytes> + TryFrom<Bytes>,
    <T as TryInto<bytes::Bytes>>::Error: Debug,
    <T as TryFrom<bytes::Bytes>>::Error: Debug,
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
        }
    }
}

impl<T> Sink<T> for Writer<T>
where
    T: Send + Sync + Unpin + TryInto<Bytes> + TryFrom<Bytes>,
    <T as TryInto<bytes::Bytes>>::Error: Debug,
    <T as TryFrom<bytes::Bytes>>::Error: Debug,
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
    T: Send + Sync + Unpin + TryInto<Bytes> + TryFrom<Bytes>,
    <T as TryInto<bytes::Bytes>>::Error: Debug,
    <T as TryFrom<bytes::Bytes>>::Error: Debug,
{
    fn try_send(&mut self, event: T) -> Option<T> {
        let value: Bytes = event.try_into().unwrap();
        let event_size = value.len();

        if self.current_size.fetch_add(event_size, Ordering::Relaxed) + (event_size / 2)
            > self.max_size
        {
            self.current_size.fetch_sub(event_size, Ordering::Relaxed);

            self.flush();

            let event: T = T::try_from(value).unwrap();
            unimplemented!()
            // return Some(event);
        }

        let key = self.offset.fetch_add(1, Ordering::Relaxed);

        self.writebatch.put(Key(key), &value);
        self.batch_size += 1;

        if self.batch_size >= 100 {
            self.flush();
        }

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
    T: Send + Sync + Unpin + TryInto<Bytes> + TryFrom<Bytes>,
    <T as TryInto<bytes::Bytes>>::Error: Debug,
    <T as std::convert::TryFrom<bytes::Bytes>>::Error: Debug,
{
    fn drop(&mut self) {
        if let Some(event) = self.slot.take() {
            // This can happen if poll_close wasn't called which is a bug
            // or we are unwinding the stack.
            //
            // We can't be picky at the moment so we will allow
            // for the buffer to exceed configured limit.
            self.max_size = usize::MAX;
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
