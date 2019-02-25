use crate::record::{proto, Record};
use futures::{
    task::{self, AtomicTask, Task},
    Async, AsyncSink, Poll, Sink, Stream,
};
use leveldb::database::{
    batch::{Batch, Writebatch},
    compaction::Compaction,
    iterator::{Iterable, LevelDBIterator},
    kv::KV,
    options::{Options, ReadOptions, WriteOptions},
    Database,
};
use log::error;
use prost::Message;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

#[derive(Copy, Clone, Debug)]
struct Key(usize);

impl db_key::Key for Key {
    fn from_u8(key: &[u8]) -> Self {
        assert_eq!(key.len(), 8);

        // TODO: replace with try_from once it's stable
        let bytes = [
            key[0], key[1], key[2], key[3], key[4], key[5], key[6], key[7],
        ];

        Self(usize::from_be_bytes(bytes))
    }

    fn as_slice<T, F: Fn(&[u8]) -> T>(&self, f: F) -> T {
        let bytes = self.0.to_be_bytes();
        f(&bytes)
    }
}

pub struct Writer {
    db: Arc<Database<Key>>,
    offset: Arc<AtomicUsize>,
    write_notifier: Arc<AtomicTask>,
    blocked_write_tasks: Arc<Mutex<Vec<Task>>>,
    writebatch: Writebatch<Key>,
    batch_size: usize,
    max_size: usize,
    current_size: Arc<AtomicUsize>,
}

// Writebatch isn't Send, but the leveldb docs explicitly say that it's okay to share across threads
unsafe impl Send for Writer {}

impl Clone for Writer {
    fn clone(&self) -> Self {
        Self {
            db: Arc::clone(&self.db),
            offset: Arc::clone(&self.offset),
            write_notifier: Arc::clone(&self.write_notifier),
            blocked_write_tasks: Arc::clone(&self.blocked_write_tasks),
            writebatch: Writebatch::new(),
            batch_size: 0,
            max_size: self.max_size,
            current_size: Arc::clone(&self.current_size),
        }
    }
}

impl Sink for Writer {
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(
        &mut self,
        record: Self::SinkItem,
    ) -> Result<AsyncSink<Self::SinkItem>, Self::SinkError> {
        let mut value = vec![];
        proto::Record::from(record).encode(&mut value).unwrap(); // This will not error when writing to a Vec
        let record_size = value.len();

        if self.current_size.fetch_add(record_size, Ordering::Relaxed) > self.max_size {
            self.blocked_write_tasks
                .lock()
                .unwrap()
                .push(task::current());

            self.current_size.fetch_sub(record_size, Ordering::Relaxed);

            self.poll_complete()?;

            let record = proto::Record::decode(value).unwrap().into();
            return Ok(AsyncSink::NotReady(record));
        }

        let key = self.offset.fetch_add(1, Ordering::Relaxed);

        self.writebatch.put(Key(key), &value);
        self.batch_size += 1;

        if self.batch_size >= 100 {
            self.poll_complete()?;
        }

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        // This doesn't write all the way through to disk and doesn't need to be wrapped
        // with `blocking`. (It does get written to a memory mapped table that will be
        // flushed even in the case of a process crash.)
        if self.batch_size > 0 {
            self.write_batch();
        }

        Ok(Async::Ready(()))
    }
}

impl Writer {
    fn write_batch(&mut self) {
        self.db
            .write(WriteOptions::new(), &self.writebatch)
            .unwrap();
        self.writebatch = Writebatch::new();
        self.batch_size = 0;
        self.write_notifier.notify();
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        if self.batch_size > 0 {
            self.write_batch();
        }

        // We need to wake up the reader so it can return None if there are no more writers
        self.write_notifier.notify();
    }
}

pub struct Reader {
    db: Arc<Database<Key>>,
    offset: usize,
    write_notifier: Arc<AtomicTask>,
    blocked_write_tasks: Arc<Mutex<Vec<Task>>>,
    advance: bool,
    delete_batch: Writebatch<Key>,
    batch_size: usize,
    current_size: Arc<AtomicUsize>,
}

// Writebatch isn't Send, but the leveldb docs explicitly say that it's okay to share across threads
unsafe impl Send for Reader {}

impl Stream for Reader {
    type Item = Record;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // If the previous call to `poll` succeeded, `forward` won't call `poll` again
        // until the sink has accepted the write.
        if self.advance {
            self.delete_batch.delete(Key(self.offset));
            self.batch_size += 1;
            self.offset += 1;
            self.advance = false;
        }

        if self.batch_size >= 10000 {
            self.delete_batch();
        }

        // If there's no value at offset, we return NotReady and rely on Writer
        // using write_notifier to wake this task up after the next write.
        self.write_notifier.register();

        // This will usually complete instantly, but in the case of a large queue (or a fresh launch of
        // the app), this will have to go to disk.
        let next = tokio_threadpool::blocking(|| {
            self.db.get(ReadOptions::new(), Key(self.offset)).unwrap()
        })
        .unwrap();

        if let Async::Ready(Some(value)) = next {
            self.current_size.fetch_sub(value.len(), Ordering::Relaxed);

            match proto::Record::decode(value) {
                Ok(record) => {
                    let record = Record::from(record);
                    self.advance = true;

                    Ok(Async::Ready(Some(record)))
                }
                Err(err) => {
                    error!("Error deserializing proto: {:?}", err);
                    debug_assert!(false);
                    self.poll()
                }
            }
        } else if Arc::strong_count(&self.db) == 1 {
            // There are no writers left
            Ok(Async::Ready(None))
        } else {
            self.delete_batch();

            Ok(Async::NotReady)
        }
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.delete_batch();
    }
}

impl Reader {
    fn delete_batch(&mut self) {
        if self.batch_size > 0 {
            self.db
                .write(WriteOptions::new(), &self.delete_batch)
                .unwrap();

            self.db.compact(&Key(0), &Key(self.offset));

            self.delete_batch = Writebatch::new();
            self.batch_size = 0;
        }

        for task in self.blocked_write_tasks.lock().unwrap().drain(..) {
            task.notify();
        }
    }
}

pub fn open(path: &std::path::Path, max_size: usize) -> (Writer, Reader) {
    let mut options = Options::new();
    options.create_if_missing = true;

    let db: Database<Key> = Database::open(path, options).unwrap();
    let db = Arc::new(db);

    let head;
    let tail;
    {
        let mut iter = db.keys_iter(ReadOptions::new());
        head = iter.next().map(|k| k.0).unwrap_or(0);
        iter.seek_to_last();
        tail = if iter.valid() { iter.key().0 + 1 } else { 0 };
    }

    let initial_size = db.value_iter(ReadOptions::new()).map(|v| v.len()).sum();
    let current_size = Arc::new(AtomicUsize::new(initial_size));

    let write_notifier = Arc::new(AtomicTask::new());

    let blocked_write_tasks = Arc::new(Mutex::new(Vec::new()));

    let writer = Writer {
        db: Arc::clone(&db),
        write_notifier: Arc::clone(&write_notifier),
        blocked_write_tasks: Arc::clone(&blocked_write_tasks),
        offset: Arc::new(AtomicUsize::new(tail)),
        writebatch: Writebatch::new(),
        batch_size: 0,
        max_size,
        current_size: Arc::clone(&current_size),
    };
    let reader = Reader {
        db: Arc::clone(&db),
        write_notifier: Arc::clone(&write_notifier),
        blocked_write_tasks,
        offset: head,
        advance: false,
        delete_batch: Writebatch::new(),
        batch_size: 0,
        current_size,
    };

    (writer, reader)
}
