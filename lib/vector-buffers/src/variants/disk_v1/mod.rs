mod acknowledgements;
mod key;
mod reader;
mod writer;

#[cfg(test)]
mod tests;

use std::{
    collections::VecDeque,
    error::Error,
    fmt::Debug,
    io,
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, AtomicUsize},
        Arc,
    },
};

use async_trait::async_trait;
use futures::task::AtomicWaker;
use leveldb::{
    batch::Writebatch,
    database::Database,
    iterator::{Iterable, LevelDBIterator},
    options::{Options, ReadOptions},
};
use parking_lot::Mutex;
use snafu::{ResultExt, Snafu};
use tokio::time::Instant;

use crate::{
    buffer_usage_data::BufferUsageHandle,
    topology::{
        acks::OrderedAcknowledgements,
        builder::IntoBuffer,
        channel::{ReceiverAdapter, SenderAdapter},
    },
    Acker, Bufferable,
};

use self::key::Key;
pub use self::{acknowledgements::create_disk_v1_acker, reader::Reader, writer::Writer};

/// How much of disk buffer needs to be deleted before we trigger compaction.
const MAX_UNCOMPACTED_DENOMINATOR: u64 = 10;

#[derive(Debug, Snafu)]
pub enum DataDirError {
    #[snafu(display("The configured data_dir {:?} does not exist, please create it and make sure the vector process can write to it", data_dir))]
    NotFound { data_dir: PathBuf },
    #[snafu(display("The configured data_dir {:?} is not writable by the vector process, please ensure vector can write to that directory", data_dir))]
    NotWritable { data_dir: PathBuf },
    #[snafu(display("Unable to look up data_dir {:?}: {:?}", data_dir, source))]
    Metadata {
        data_dir: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Unable to open data_dir {:?}: {:?}", data_dir, source))]
    Open {
        data_dir: PathBuf,
        source: leveldb::database::error::Error,
    },
}

pub struct DiskV1Buffer {
    id: String,
    data_dir: PathBuf,
    max_size: u64,
}

impl DiskV1Buffer {
    pub fn new(id: String, data_dir: PathBuf, max_size: u64) -> Self {
        DiskV1Buffer {
            id,
            data_dir,
            max_size,
        }
    }
}

#[async_trait]
impl<T> IntoBuffer<T> for DiskV1Buffer
where
    T: Bufferable + Clone,
{
    fn provides_instrumentation(&self) -> bool {
        true
    }

    async fn into_buffer_parts(
        self: Box<Self>,
        usage_handle: BufferUsageHandle,
    ) -> Result<(SenderAdapter<T>, ReceiverAdapter<T>, Option<Acker>), Box<dyn Error + Send + Sync>>
    {
        usage_handle.set_buffer_limits(Some(self.max_size), None);

        // Create the actual buffer subcomponents.
        let (writer, reader, acker) = open(&self.data_dir, &self.id, self.max_size, usage_handle)?;

        Ok((
            SenderAdapter::opaque(writer),
            ReceiverAdapter::opaque(reader),
            Some(acker),
        ))
    }
}

/// Opens a [`leveldb_buffer::Buffer`].
///
/// # Errors
///
/// This function will fail with [`DataDirError`] if the directory does not exist at
/// `data_dir`, if permissions are not sufficient etc.
pub(self) fn open<T>(
    data_dir: &Path,
    name: &str,
    max_size: u64,
    usage_handle: BufferUsageHandle,
) -> Result<(Writer<T>, Reader<T>, Acker), DataDirError>
where
    T: Bufferable + Clone,
{
    // Make sure we have permissions to read/write to the top-level data directory.
    check_data_dir_permissions(data_dir)?;

    // In #10379, we introduced a regrression by changing the data directory used for disk v1
    // buffers, going from `<component name>_buffer` to `<component name>_id`.  I still don't really
    // know why I made this change, but it was made, and released.  So now we have to compensate for
    // it.
    //
    // The following logic is meant to try and offer a more graceful transition between the two, if
    // possible.  We'll refer to the `_buffer` approach as the "old style" and the `_id` approach as
    // the "new style".
    //
    // Logic of this transition code:
    // - if there is not an existing directory matching the old style path, then use the new style
    //   path (regardless of whether or not the new style path exists)
    // - if there is a data directory for both the old style and new style paths, use the new style
    //   path, but we also try opening the database under the old style path to see if it still
    //   contains any data:
    //   -- if there is any data in the old style path, emit a warn log that
    //      there is still data in the old style path, which may represent older records --- if there
    //      is no data in the old style path, then delete the old style path entirely and simply move
    //      forward with the new style path
    //   -- if there is a data directory for the old style path only, move it to the "new style" path,
    //      and use the new style path
    //
    // This should provide an effective transition for users switching to 0.19.1 from 0.18.0 or
    // earlier that transparently switches directories if there is no data in the old buffer, but
    // warns them appropriately if there is.  We don't try to read the old data preferentially
    // because we might otherwise introduce old data that could mess up observability pipelines.
    //
    // For new users starting out cleanly with 0.19.0 or higher, there's no change in behavior.
    let buffer_id = get_new_style_buffer_dir_name(name);
    let path = data_dir.join(buffer_id);
    let path_exists = check_data_dir_exists(&path)?;

    let old_buffer_id = get_old_style_buffer_dir_name(name);
    let old_path = data_dir.join(old_buffer_id);
    let old_path_exists = check_data_dir_exists(&old_path)?;

    if old_path_exists {
        if path_exists {
            let sidelined_buffer_id = get_sidelined_old_style_buffer_dir_name(name);
            let sidelined_path = data_dir.join(sidelined_buffer_id);

            // Both old style and new style paths exist.  We check if the old style path has any data,
            // and if it does, we emit a warning log because since the new style path exists, we don't
            // want to risk missing data on that side by trying to read the old data first and then
            // moving to the new data, etc.
            //
            // If there's no data in the old style path, though, we just delete the directory and move
            // on: no need to emit anything because nothing is being lost.
            let (old_buffer_size, old_buffer_record_count) = db_initial_size::<T>(&old_path)?;
            if old_buffer_size != 0 || old_buffer_record_count != 0 {
                // The old style path still has some data, so all we're going to do is warn the user
                // that this is the case, since we don't want to risk reading older records that
                // they've moved on from after switching to the new style path.
                warn!(
                    old_buffer_record_count, old_buffer_size,
                    "Found both old and new buffers with data for '{}' sink. This may indicate that you upgraded to 0.19.x prior to a regression being fixed which deals with disk buffer directory names. Using new buffers and ignoring old. See https://github.com/vectordotdev/vector/issues/10430 for more information.\n\nYou can suppress this message by renaming the old buffer data directory to something else.  Current path for old buffer data directory: {}, suggested path for renaming: {}",
                    name, old_path.to_string_lossy(), sidelined_path.to_string_lossy()
                );
            } else {
                // The old style path has no more data.  Theoretically, we should be able to delete
                // it, but that's a bit risky, so we just rename it instead.
                std::fs::rename(&old_path, &sidelined_path)
                    .map_err(|e| map_io_error(e, &sidelined_path))?;

                info!(
                    "Archived old buffer data directory from '{}' to '{}' for '{}' sink.",
                    old_path.to_string_lossy(),
                    sidelined_path.to_string_lossy(),
                    name
                );
            }
        } else {
            // Old style path exists, but not the new style path.  Move the old style path to the
            // new style path and then use the new style path going forward.
            std::fs::rename(&old_path, &path).map_err(|e| map_io_error(e, &path))?;

            info!(
                "Migrated old buffer data directory from '{}' to '{}' for '{}' sink.",
                old_path.to_string_lossy(),
                path.to_string_lossy(),
                name
            );
        }
    }

    build(&path, max_size, usage_handle)
}

/// Read the byte size and item size of the database
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
pub(super) fn db_initial_size<T>(path: &Path) -> Result<(u64, u64), DataDirError>
where
    T: Bufferable,
{
    let mut options = Options::new();
    options.create_if_missing = true;

    let db: Database<Key> = Database::open(path, options).with_context(|_| OpenSnafu {
        data_dir: path.parent().expect("always a parent"),
    })?;

    let mut byte_size = 0;
    let mut key_count = 0;
    let mut first_key = None;
    let mut last_key = None;
    for (k, v) in db.iter(ReadOptions::new()) {
        byte_size += v.len() as u64;
        key_count += 1;

        if first_key.is_none() {
            first_key = Some(k.0);
        }
        last_key = Some(k.0);
    }

    // Keys are assigned such that if we write an item that compromises 10 events, and that item has
    // key K, then the next key we generate will be K+10.  This lets us take the difference between
    // the last key and the first key to get the number of actual events in the buffer, minus the
    // events contained in the last key/value pair.  We decode it below to get that, too.
    let mut item_size = last_key.unwrap_or(0) as u64 - first_key.unwrap_or(0) as u64;

    if last_key.is_some() {
        let iter = db.iter(ReadOptions::new());
        iter.seek_to_last();
        if iter.valid() {
            let val = iter.value();
            if let Ok(item) = T::decode(T::get_metadata(), &val[..]) {
                item_size += item.event_count() as u64;
            }
        }
    }

    debug!(
        ?first_key,
        ?last_key,
        "Read {} key(s) from database, with {} bytes total, comprising {} events total.",
        key_count,
        byte_size,
        item_size
    );

    Ok((byte_size, item_size))
}

/// Build a new `DiskBuffer` rooted at `path`
///
/// # Errors
///
/// Function will fail if the permissions of `path` are not correct, if
/// there is no space available on disk etc.
#[allow(clippy::cast_precision_loss)]
pub fn build<T: Bufferable>(
    path: &Path,
    max_size: u64,
    usage_handle: BufferUsageHandle,
) -> Result<(Writer<T>, Reader<T>, Acker), DataDirError> {
    // New `max_size` of the buffer is used for storing the unacked events.
    // The rest is used as a buffer which when filled triggers compaction.
    let max_uncompacted_size = max_size / MAX_UNCOMPACTED_DENOMINATOR;
    let max_size = max_size - max_uncompacted_size;

    let (initial_byte_size, initial_item_size) = db_initial_size::<T>(path)?;
    usage_handle.increment_received_event_count_and_byte_size(initial_item_size, initial_byte_size);

    let mut options = Options::new();
    options.create_if_missing = true;

    let db: Database<Key> = Database::open(path, options).with_context(|_| OpenSnafu {
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

    let current_size = Arc::new(AtomicU64::new(initial_byte_size));

    let write_notifier = Arc::new(AtomicWaker::new());

    let blocked_write_tasks = Arc::new(Mutex::new(Vec::new()));

    let ack_counter = Arc::new(AtomicUsize::new(0));
    let acker = create_disk_v1_acker(&ack_counter, &write_notifier);

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
        usage_handle: usage_handle.clone(),
    };

    let reader = Reader {
        db: Arc::clone(&db),
        write_notifier: Arc::clone(&write_notifier),
        blocked_write_tasks,
        read_offset: head,
        compacted_offset: 0,
        delete_offset: head,
        current_size,
        ack_counter,
        max_uncompacted_size,
        uncompacted_size: 0,
        record_acks: OrderedAcknowledgements::from_acked(head),
        buffer: VecDeque::new(),
        last_compaction: Instant::now(),
        last_flush: Instant::now(),
        pending_read: None,
        usage_handle,
        phantom: PhantomData,
    };

    Ok((writer, reader, acker))
}

fn map_io_error<P>(e: io::Error, data_dir: P) -> DataDirError
where
    P: AsRef<Path>,
{
    match e.kind() {
        io::ErrorKind::PermissionDenied => DataDirError::NotWritable {
            data_dir: data_dir.as_ref().to_path_buf(),
        },
        io::ErrorKind::NotFound => DataDirError::NotFound {
            data_dir: data_dir.as_ref().to_path_buf(),
        },
        _ => DataDirError::Metadata {
            data_dir: data_dir.as_ref().to_path_buf(),
            source: e,
        },
    }
}

fn check_data_dir_exists<P>(data_dir: P) -> Result<bool, DataDirError>
where
    P: AsRef<Path>,
{
    std::fs::metadata(&data_dir)
        .map(|m| m.is_dir())
        .or_else(|e| match map_io_error(e, &data_dir) {
            DataDirError::NotFound { .. } => Ok(false),
            de => Err(de),
        })
}

fn check_data_dir_permissions<P>(data_dir: P) -> Result<(), DataDirError>
where
    P: AsRef<Path>,
{
    std::fs::metadata(&data_dir)
        .map_err(|e| map_io_error(e, &data_dir))
        .and_then(|m| {
            if m.permissions().readonly() {
                Err(DataDirError::NotWritable {
                    data_dir: data_dir.as_ref().to_path_buf(),
                })
            } else {
                Ok(())
            }
        })
}

pub(self) fn get_old_style_buffer_dir_name(base: &str) -> String {
    format!("{}_buffer", base)
}

pub(self) fn get_new_style_buffer_dir_name(base: &str) -> String {
    format!("{}_id", base)
}

pub(self) fn get_sidelined_old_style_buffer_dir_name(base: &str) -> String {
    format!("{}_buffer_old", base)
}
