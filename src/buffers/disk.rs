use crate::record::Record;
use futures::{task::AtomicTask, Async, AsyncSink, Poll, Sink, Stream};
use leveldb::database::{
    batch::{Batch, Writebatch},
    iterator::{Iterable, LevelDBIterator},
    kv::KV,
    options::{Options, ReadOptions, WriteOptions},
    Database,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
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
    notifier: Arc<AtomicTask>,
    writebatch: Writebatch<Key>,
}

// Writebatch isn't Send, but the leveldb docs explicitly say that it's okay to share across threads
unsafe impl Send for Writer {}

impl Clone for Writer {
    fn clone(&self) -> Self {
        Self {
            db: Arc::clone(&self.db),
            offset: Arc::clone(&self.offset),
            notifier: Arc::clone(&self.notifier),
            writebatch: Writebatch::new(),
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
        let line = record.line;
        let value = line.as_bytes();

        let key = self.offset.fetch_add(1, Ordering::Relaxed);

        self.writebatch.put(Key(key), value);

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, Self::SinkError> {
        // TODO: should we periodically flush after N records are in the write batch?
        self.db
            .write(WriteOptions::new(), &self.writebatch)
            .unwrap();
        self.writebatch = Writebatch::new();
        self.notifier.notify();

        Ok(Async::Ready(()))
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        // We need to wake up the reader so it can return None if there are no more writers
        self.notifier.notify();
    }
}

pub struct Reader {
    db: Arc<Database<Key>>,
    offset: usize,
    notifier: Arc<AtomicTask>,
    advance: bool,
}

impl Stream for Reader {
    type Item = Record;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        // If the previous call to `poll` succeeded, `forward` won't call `poll` again
        // until the sink has accepted the write.
        if self.advance {
            self.db
                .delete(WriteOptions::new(), Key(self.offset))
                .unwrap();
            self.offset += 1;
            self.advance = false;
        }

        self.notifier.register();

        // This will usually complete instantly, but in the case of a large queue (or a fresh launch of
        // the app), this will have to go to disk.
        let next = tokio_threadpool::blocking(|| {
            self.db.get(ReadOptions::new(), Key(self.offset)).unwrap()
        })
        .unwrap();

        if let Async::Ready(Some(value)) = next {
            // TODO: round trip the original record with protobuf
            let line = String::from_utf8(value).unwrap();
            let record = Record::new_from_line(line);
            self.advance = true;

            Ok(Async::Ready(Some(record)))
        } else if Arc::strong_count(&self.db) == 1 {
            // There are no writers left
            Ok(Async::Ready(None))
        } else {
            Ok(Async::NotReady)
        }
    }
}

pub fn open(path: &std::path::Path) -> (Writer, Reader) {
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

    let notifier = Arc::new(AtomicTask::new());

    let writer = Writer {
        db: Arc::clone(&db),
        notifier: Arc::clone(&notifier),
        offset: Arc::new(AtomicUsize::new(tail)),
        writebatch: Writebatch::new(),
    };
    let reader = Reader {
        db: Arc::clone(&db),
        notifier: Arc::clone(&notifier),
        offset: head,
        advance: false,
    };

    (writer, reader)
}
