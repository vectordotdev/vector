use crate::event::{proto, Event};
use bytes::Bytes;
use futures::{task::AtomicWaker, Sink, Stream};
use leveldb::database::{
    batch::{Batch, Writebatch},
    compaction::Compaction,
    iterator::{Iterable, LevelDBIterator},
    options::{Options, ReadOptions, WriteOptions},
    Database,
};
use prost::Message;
use snafu::ResultExt;
use std::{
    collections::VecDeque,
    convert::TryInto,
    mem::size_of,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll, Waker},
};

use super::{DataDirError, Open};
use crate::buffers::Acker;

/// How much of disk buffer needs to be deleted before we trigger compaction.
const MAX_UNCOMPACTED_DENOMINATOR: usize = 10;

#[derive(Copy, Clone, Debug)]
struct Key(pub usize);

impl db_key::Key for Key {
    fn from_u8(key: &[u8]) -> Self {
        let bytes: [u8; size_of::<usize>()] = key.try_into().expect("Key should be the right size");

        Self(usize::from_be_bytes(bytes))
    }

    fn as_slice<T, F: Fn(&[u8]) -> T>(&self, f: F) -> T {
        let bytes = self.0.to_be_bytes();
        f(&bytes)
    }
}

pub struct Writer {
    db: Option<Arc<Database<Key>>>,
    offset: Arc<AtomicUsize>,
    write_notifier: Arc<AtomicWaker>,
    blocked_write_tasks: Arc<Mutex<Vec<Waker>>>,
    writebatch: Writebatch<Key>,
    batch_size: usize,
    max_size: usize,
    current_size: Arc<AtomicUsize>,
    slot: Option<Event>,
}

// Writebatch isn't Send, but the leveldb docs explicitly say that it's okay to share across threads
unsafe impl Send for Writer {}

impl Clone for Writer {
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

impl Sink<Event> for Writer {
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.slot.is_none() {
            Poll::Ready(Ok(()))
        } else {
            // Assumes that flush will only succeed if it has also emptied the slot,
            // hence we don't need to recheck if the slot is empty.
            self.poll_flush(cx)
        }
    }

    fn start_send(mut self: Pin<&mut Self>, item: Event) -> Result<(), Self::Error> {
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
                    // is a scenario where the reader won't be polled again hence
                    // this sink will never be notified again so this will stall.
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

impl Writer {
    fn try_send(&mut self, event: Event) -> Option<Event> {
        let mut value = vec![];
        proto::EventWrapper::from(event).encode(&mut value).unwrap(); // This will not error when writing to a Vec
        let event_size = value.len();

        if self.current_size.fetch_add(event_size, Ordering::Relaxed) + (event_size / 2)
            > self.max_size
        {
            self.current_size.fetch_sub(event_size, Ordering::Relaxed);

            self.flush();

            let buf = Bytes::from(value);
            let event = proto::EventWrapper::decode(buf).unwrap().into();
            return Some(event);
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
        // This doesn't write all the way through to disk and doesn't need to be wrapped
        // with `blocking`. (It does get written to a memory mapped table that will be
        // flushed even in the case of a process crash.)
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

impl Drop for Writer {
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

        // We drop the database Arc before notifying reader to avoid the case where we
        // notify the reader, the reader reacts and checks Arc::strong_count to be > 1
        // and then we drop the Arc which would cause a stall.
        self.db.take();
        // We need to wake up the reader so it can return None if there are no more writers
        self.write_notifier.wake();
    }
}

pub struct Reader {
    db: Arc<Database<Key>>,
    read_offset: usize,
    delete_offset: usize,
    write_notifier: Arc<AtomicWaker>,
    blocked_write_tasks: Arc<Mutex<Vec<Waker>>>,
    current_size: Arc<AtomicUsize>,
    ack_counter: Arc<AtomicUsize>,
    uncompacted_size: usize,
    unacked_sizes: VecDeque<usize>,
    buffer: Vec<Vec<u8>>,
    max_uncompacted_size: usize,
}

// Writebatch isn't Send, but the leveldb docs explicitly say that it's okay to share across threads
unsafe impl Send for Reader {}

impl Stream for Reader {
    type Item = Event;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // If there's no value at read_offset, we return NotReady and rely on Writer
        // using write_notifier to wake this task up after the next write.
        self.write_notifier.register(cx.waker());

        self.delete_acked();

        if self.buffer.is_empty() {
            // This will usually complete instantly, but in the case of a large queue (or a fresh launch of
            // the app), this will have to go to disk.
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

            let buf = Bytes::from(value);
            match proto::EventWrapper::decode(buf) {
                Ok(event) => {
                    let event = Event::from(event);
                    Poll::Ready(Some(event))
                }
                Err(error) => {
                    error!(message = "Error deserializing proto.", %error);
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

impl Drop for Reader {
    fn drop(&mut self) {
        self.delete_acked();
        // Compact on every shutdown
        self.compact();
    }
}

impl Reader {
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

    fn compact(&mut self) {
        if self.uncompacted_size > 0 {
            self.uncompacted_size = 0;

            debug!("Compacting disk buffer.");
            self.db.compact(&Key(0), &Key(self.delete_offset));
        }
    }
}

pub struct Buffer;

/// Read the byte size of the database
///
/// There is a mismatch between leveldb's mechanism and vector's. While vector
/// would prefer to keep as little in-memory as possible leveldb, being a
/// database, has the opposite consideration. As such it may mmap 1000 of its
/// LDB files into vector's address space at a time with no ability for us to
/// change this number. See [leveldb issue
/// 866](https://github.com/google/leveldb/issues/866). Because we do need to
/// know the byte size of our store we are forced to iterate through all the LDB
/// files on disk, meaning we impose a huge memory burden on our end users right
/// at the jump in conditions where the disk buffer has filled up. This'll OOM
/// vector, meaning we're trapped in a catch 22.
///
/// This function does not solve the problem -- leveldb will still map 1000
/// files if it wants -- but we at least avoid forcing this to happen at the
/// start of vector.
fn db_initial_size(path: &Path) -> Result<usize, DataDirError> {
    let mut options = Options::new();
    options.create_if_missing = true;
    let db: Database<Key> = Database::open(&path, options).with_context(|| Open {
        data_dir: path.parent().expect("always a parent"),
    })?;
    Ok(db.value_iter(ReadOptions::new()).map(|v| v.len()).sum())
}

impl super::DiskBuffer for Buffer {
    type Writer = Writer;
    type Reader = Reader;

    // We convert `max_size` into an f64 at
    #[allow(clippy::cast_precision_loss)]
    fn build(
        path: PathBuf,
        max_size: usize,
    ) -> Result<(Self::Writer, Self::Reader, Acker), DataDirError> {
        // New `max_size` of the buffer is used for storing the unacked events.
        // The rest is used as a buffer which when filled triggers compaction.
        let max_uncompacted_size = max_size / MAX_UNCOMPACTED_DENOMINATOR;
        let max_size = max_size - max_uncompacted_size;

        let initial_size = db_initial_size(&path)?;

        let mut options = Options::new();
        options.create_if_missing = true;

        let db: Database<Key> = Database::open(&path, options).with_context(|| Open {
            data_dir: path.parent().expect("always a parent"),
        })?;
        let db = Arc::new(db);

        let head;
        let tail;
        {
            let mut iter = db.keys_iter(ReadOptions::new());
            head = iter.next().map_or(0, |k| k.0);
            iter.seek_to_last();
            tail = if iter.valid() { iter.key().0 + 1 } else { 0 };
        }

        let current_size = Arc::new(AtomicUsize::new(initial_size));

        let write_notifier = Arc::new(AtomicWaker::new());

        let blocked_write_tasks = Arc::new(Mutex::new(Vec::new()));

        let ack_counter = Arc::new(AtomicUsize::new(0));
        let acker = Acker::Disk(Arc::clone(&ack_counter), Arc::clone(&write_notifier));

        let writer = Writer {
            db: Some(Arc::clone(&db)),
            write_notifier: Arc::clone(&write_notifier),
            blocked_write_tasks: Arc::clone(&blocked_write_tasks),
            offset: Arc::new(AtomicUsize::new(tail)),
            writebatch: Writebatch::new(),
            batch_size: 0,
            max_size,
            current_size: Arc::clone(&current_size),
            slot: None,
        };

        let mut reader = Reader {
            db: Arc::clone(&db),
            write_notifier: Arc::clone(&write_notifier),
            blocked_write_tasks,
            read_offset: head,
            delete_offset: head,
            current_size,
            ack_counter,
            max_uncompacted_size,
            uncompacted_size: 1,
            unacked_sizes: VecDeque::new(),
            buffer: Vec::new(),
        };
        // Compact on every start
        reader.compact();

        Ok((writer, reader, acker))
    }
}
