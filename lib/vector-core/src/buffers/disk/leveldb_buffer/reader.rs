use super::Key;
use crate::bytes::DecodeBytes;
use bytes::Bytes;
use futures::{task::AtomicWaker, Stream};
use leveldb::database::{
    batch::{Batch, Writebatch},
    compaction::Compaction,
    iterator::{Iterable, LevelDBIterator},
    options::{ReadOptions, WriteOptions},
    Database,
};
use std::collections::VecDeque;
use std::fmt::Display;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::task::{Context, Poll, Waker};

pub struct Reader<T>
where
    T: Send + Sync + Unpin,
{
    pub(crate) db: Arc<Database<Key>>,
    pub(crate) read_offset: usize,
    pub(crate) delete_offset: usize,
    pub(crate) write_notifier: Arc<AtomicWaker>,
    pub(crate) blocked_write_tasks: Arc<Mutex<Vec<Waker>>>,
    pub(crate) current_size: Arc<AtomicUsize>,
    pub(crate) ack_counter: Arc<AtomicUsize>,
    pub(crate) uncompacted_size: usize,
    pub(crate) unacked_sizes: VecDeque<usize>,
    pub(crate) buffer: Vec<Vec<u8>>,
    pub(crate) max_uncompacted_size: usize,
    pub(crate) phantom: PhantomData<T>,
}

// Writebatch isn't Send, but the leveldb docs explicitly say that it's okay to
// share across threads
unsafe impl<T> Send for Reader<T> where T: Send + Sync + Unpin {}

impl<T> Stream for Reader<T>
where
    T: Send + Sync + Unpin + DecodeBytes<T>,
    <T as DecodeBytes<T>>::Error: Display,
{
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // If there's no value at read_offset, we return NotReady and rely on
        // Writer using write_notifier to wake this task up after the next
        // write.
        self.write_notifier.register(cx.waker());

        self.delete_acked();

        if self.buffer.is_empty() {
            // This will usually complete instantly, but in the case of a large
            // queue (or a fresh launch of the app), this will have to go to
            // disk.
            let new_data = tokio::task::block_in_place(|| {
                self.db
                    .value_iter(ReadOptions::new())
                    .from(&Key(self.read_offset))
                    .to(&Key(self.read_offset + 100))
                    .collect()
            });
            self.buffer = new_data;
            self.buffer.reverse(); // so we can pop
        }

        if let Some(value) = self.buffer.pop() {
            self.unacked_sizes.push_back(value.len());
            self.read_offset += 1;

            let buffer: Bytes = Bytes::from(value);
            match T::decode(buffer) {
                Ok(event) => Poll::Ready(Some(event)),
                Err(error) => {
                    error!(message = "Error deserializing event.", %error);
                    debug_assert!(false);
                    self.poll_next(cx)
                }
            }
        } else if Arc::strong_count(&self.db) == 1 {
            // There are no writers left
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

impl<T> Drop for Reader<T>
where
    T: Send + Sync + Unpin,
{
    fn drop(&mut self) {
        self.delete_acked();
        // Compact on every shutdown
        self.compact();
    }
}

impl<T> Reader<T>
where
    T: Send + Sync + Unpin,
{
    fn delete_acked(&mut self) {
        let num_to_delete = self.ack_counter.swap(0, Ordering::Relaxed);

        if num_to_delete > 0 {
            let new_offset = self.delete_offset + num_to_delete;
            assert!(
                new_offset <= self.read_offset,
                "Tried to ack beyond read offset"
            );

            let mut delete_batch = Writebatch::new();

            for i in self.delete_offset..new_offset {
                delete_batch.delete(Key(i));
            }

            self.db.write(WriteOptions::new(), &delete_batch).unwrap();

            self.delete_offset = new_offset;

            let size_deleted = self.unacked_sizes.drain(..num_to_delete).sum();
            self.current_size.fetch_sub(size_deleted, Ordering::Release);

            self.uncompacted_size += size_deleted;
            if self.uncompacted_size > self.max_uncompacted_size {
                self.compact();
            }
        }

        for task in self.blocked_write_tasks.lock().unwrap().drain(..) {
            task.wake();
        }
    }

    pub(crate) fn compact(&mut self) {
        if self.uncompacted_size > 0 {
            self.uncompacted_size = 0;

            debug!("Compacting disk buffer.");
            self.db.compact(&Key(0), &Key(self.delete_offset));
        }
    }
}
