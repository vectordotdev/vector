use std::{
    collections::{HashMap, VecDeque},
    io::Cursor,
    sync::{
        atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering},
        Arc, Mutex,
    },
    task::Poll,
};

use crossbeam_queue::SegQueue;
use proptest::{prop_assert, prop_assert_eq, proptest};
use temp_dir::TempDir;
use tokio::runtime::Builder;

use crate::{
    buffer_usage_data::BufferUsageHandle,
    encoding::FixedEncodable,
    test::install_tracing_helpers,
    variants::disk_v2::{
        common::MAX_FILE_ID, record::RECORD_HEADER_LEN, writer::RecordWriter, Buffer,
        DiskBufferConfig, WriterError,
    },
    EventCount,
};

mod action;
use self::{
    action::{arb_actions, Action},
    record::EncodeError,
};

mod common;
use self::common::{arb_buffer_config, Progress};

mod filesystem;
use self::filesystem::TestFilesystem;

mod record;
use self::record::Record;

mod sequencer;
use self::sequencer::{ActionSequencer, ReadActionResult, WriteActionResult};

/// Model for the filesystem.
///
/// Provides roughly-equivalent methods as the `Filesystem` trait, and internally holds the state of
/// all files used by the reader and writer.
#[derive(Debug)]
struct FilesystemModel {
    file_size_limit: u64,
    write_buffer_size: u64,
    files: Mutex<HashMap<u16, FileModel>>,
}

impl FilesystemModel {
    pub fn new(config: &DiskBufferConfig<TestFilesystem>) -> Self {
        Self {
            file_size_limit: config.max_data_file_size,
            write_buffer_size: config.write_buffer_size as u64,
            files: Mutex::default(),
        }
    }

    fn open_file(&self, id: u16) -> Option<FileModel> {
        let files = self.files.lock().expect("poisoned");
        files.get(&id).cloned()
    }

    fn create_file(&self, id: u16) -> Option<FileModel> {
        let mut files = self.files.lock().expect("poisoned");
        if files.contains_key(&id) {
            return None;
        }

        let file = FileModel::new(self.file_size_limit, self.write_buffer_size);
        files.insert(id, file.clone());

        Some(file)
    }

    fn delete_file(&self, id: u16) -> bool {
        let mut files = self.files.lock().expect("poisoned");
        files.remove(&id).is_some()
    }
}

/// Models the all-in behavior of for a data file wrapped with a buffered writer.
///
/// We deal explicitly with records but do track the on-disk byte size of a record, and correctly
/// encapsulate the concept of a buffered writer by tracking unflushed records, as well as
/// forcefully flushing when the internal buffer is overrun, and so on.
#[derive(Clone, Debug)]
struct FileModel {
    file_size_limit: u64,
    write_buffer_size: u64,
    unflushed_records: Arc<SegQueue<(Record, u64)>>,
    unflushed_bytes: Arc<AtomicU64>,
    flushed_records: Arc<SegQueue<(Record, u64)>>,
    flushed_bytes: Arc<AtomicU64>,
    finalized: Arc<AtomicBool>,
}

impl FileModel {
    fn new(file_size_limit: u64, write_buffer_size: u64) -> Self {
        Self {
            file_size_limit,
            write_buffer_size,
            unflushed_records: Arc::default(),
            unflushed_bytes: Arc::default(),
            flushed_records: Arc::default(),
            flushed_bytes: Arc::default(),
            finalized: Arc::default(),
        }
    }

    fn finalize(&self) {
        self.finalized.store(true, Ordering::SeqCst);
    }

    fn is_finalized(&self) -> bool {
        self.finalized.load(Ordering::SeqCst)
    }

    fn write(&self, record: Record, record_len: u64) -> (Option<Record>, usize, u64) {
        assert!(
            !self.is_finalized(),
            "invariant violation: tried to write to file after finalization"
        );

        // If this record, when written, would exceed the maximum file size, we have to reject it,
        // unless the file is entirely empty, in which case we always allow at least one write:
        let file_size_overall =
            self.flushed_bytes.load(Ordering::SeqCst) + self.unflushed_bytes.load(Ordering::SeqCst);
        if file_size_overall > 0 && file_size_overall + record_len > self.file_size_limit {
            return (Some(record), 0, 0);
        }

        let mut flushed_events = 0;
        let mut flushed_bytes = 0;

        // If this write would exceed our internal write buffer capacity, flush any unflushed
        // records first:
        if self.unflushed_bytes.load(Ordering::SeqCst) + record_len > self.write_buffer_size {
            let (events, bytes) = self.flush();
            flushed_events += events;
            flushed_bytes += bytes;
        }

        if record_len >= self.write_buffer_size {
            // The record is bigger the write buffer itself, so just write it immediately:
            flushed_events += record.event_count();
            flushed_bytes += record_len;

            self.flushed_bytes.fetch_add(record_len, Ordering::SeqCst);
            self.flushed_records.push((record, record_len));
        } else {
            self.unflushed_bytes.fetch_add(record_len, Ordering::SeqCst);
            self.unflushed_records.push((record, record_len));
        }

        (None, flushed_events, flushed_bytes)
    }

    fn flush(&self) -> (usize, u64) {
        let mut flushed_events = 0;
        let mut flushed_bytes = 0;
        while let Some((record, record_len)) = self.unflushed_records.pop() {
            flushed_events += record.event_count();
            flushed_bytes += record_len;

            self.unflushed_bytes.fetch_sub(record_len, Ordering::SeqCst);
            self.flushed_bytes.fetch_add(record_len, Ordering::SeqCst);
            self.flushed_records.push((record, record_len));
        }

        (flushed_events, flushed_bytes)
    }

    fn read(&self) -> Option<(Record, u64)> {
        self.flushed_records.pop()
    }

    fn flushed_size(&self) -> u64 {
        self.flushed_bytes.load(Ordering::SeqCst)
    }

    fn unflushed_size(&self) -> u64 {
        self.unflushed_bytes.load(Ordering::SeqCst)
    }
}

/// Model for the ledger.
///
/// Very simplistic as we really just use it to hold the buffer size, in bytes and events, and the
/// reader/writer file state.
struct LedgerModel {
    config: DiskBufferConfig<TestFilesystem>,
    buffer_size: AtomicU64,
    writer_file_id: AtomicU16,
    reader_file_id: AtomicU16,
    writer_done: AtomicBool,
    unread_events: AtomicU64,
}

impl LedgerModel {
    fn new(config: &DiskBufferConfig<TestFilesystem>) -> Self {
        Self {
            config: config.clone(),
            buffer_size: AtomicU64::new(0),
            writer_file_id: AtomicU16::new(0),
            reader_file_id: AtomicU16::new(0),
            writer_done: AtomicBool::new(false),
            unread_events: AtomicU64::new(0),
        }
    }

    fn config(&self) -> &DiskBufferConfig<TestFilesystem> {
        &self.config
    }

    fn mark_writer_done(&self) {
        self.writer_done.store(true, Ordering::SeqCst);
    }

    fn is_writer_done(&self) -> bool {
        self.writer_done.load(Ordering::SeqCst)
    }

    fn get_buffer_size(&self) -> u64 {
        self.buffer_size.load(Ordering::SeqCst)
    }

    fn increment_buffer_size(&self, amount: u64) {
        self.buffer_size.fetch_add(amount, Ordering::SeqCst);
    }

    fn decrement_buffer_size(&self, amount: u64) {
        self.buffer_size.fetch_sub(amount, Ordering::SeqCst);
    }

    fn get_unread_events(&self) -> u64 {
        self.unread_events.load(Ordering::SeqCst)
    }

    fn increment_unread_events(&self, amount: u64) {
        self.unread_events.fetch_add(amount, Ordering::SeqCst);
    }

    fn decrement_unread_events(&self, amount: u64) {
        self.unread_events.fetch_sub(amount, Ordering::SeqCst);
    }

    fn get_writer_file_id(&self) -> u16 {
        self.writer_file_id.load(Ordering::SeqCst) % MAX_FILE_ID
    }

    fn increment_writer_file_id(&self) {
        self.writer_file_id.fetch_add(1, Ordering::SeqCst);
    }

    fn get_reader_file_id(&self) -> u16 {
        self.reader_file_id.load(Ordering::SeqCst) % MAX_FILE_ID
    }

    fn increment_reader_file_id(&self) {
        self.reader_file_id.fetch_add(1, Ordering::SeqCst);
    }
}

enum ReaderModelState {
    Idle,
    PendingRead,
}

impl ReaderModelState {
    fn transition_to_idle(&mut self) {
        match self {
            Self::Idle => panic!("should not transition to idle when already idle"),
            Self::PendingRead => *self = Self::Idle,
        }
    }

    fn transition_to_reading(&mut self) {
        *self = Self::PendingRead;
    }
}

/// Model for the reader.
struct ReaderModel {
    filesystem: Arc<FilesystemModel>,
    ledger: Arc<LedgerModel>,
    state: ReaderModelState,
    current_file: Option<FileModel>,
    current_file_bytes_read: u64,
    current_file_records_read: usize,
    outstanding_event_acks: usize,
    unconsumed_event_acks: usize,
    pending_record_acks: VecDeque<(usize, u64)>,
    unconsumed_record_acks: usize,
    pending_data_file_acks: VecDeque<(u16, usize)>,
}

impl ReaderModel {
    fn new(filesystem: Arc<FilesystemModel>, ledger: Arc<LedgerModel>) -> Self {
        let mut reader = Self {
            filesystem,
            ledger,
            state: ReaderModelState::Idle,
            current_file: None,
            current_file_bytes_read: 0,
            current_file_records_read: 0,
            outstanding_event_acks: 0,
            unconsumed_event_acks: 0,
            pending_record_acks: VecDeque::new(),
            unconsumed_record_acks: 0,
            pending_data_file_acks: VecDeque::new(),
        };

        // We do a dummy call to `check_ready` to simulate what happens when a real buffer is created,
        // as the real initialization process always ensures the current reader data file exists/is opened.
        reader.check_ready();

        reader
    }

    fn reset(&mut self) {
        self.current_file = None;
        self.current_file_records_read = 0;
        self.current_file_bytes_read = 0;
    }

    fn move_to_next_file(&mut self) {
        let required_record_acks = self.current_file_records_read;
        let current_file_id = self.ledger.get_reader_file_id();
        self.pending_data_file_acks
            .push_back((current_file_id, required_record_acks));

        self.reset();
        self.ledger.increment_reader_file_id();
    }

    fn handle_acks(&mut self) {
        // Process record acknowledgements first.
        while self.unconsumed_event_acks > 0 && !self.pending_record_acks.is_empty() {
            let (required_event_acks, record_bytes) =
                self.pending_record_acks.front().copied().unwrap();
            if self.unconsumed_event_acks > 0 {
                // We have enough unconsumed event acknowledgements to fully acknowledge this
                // record. Remove it, consume the event acknowledgements, add a record
                // acknowledgement, and update the buffer size.
                _ = self.pending_record_acks.pop_front().unwrap();
                self.unconsumed_event_acks -= 1;
                self.unconsumed_record_acks += 1;

                self.ledger.decrement_buffer_size(record_bytes);
                self.ledger
                    .decrement_unread_events(required_event_acks as u64);
            } else {
                // Not enough event acknowledgements to proceed, so we can't do anything more.
                break;
            }
        }

        // Now process data file acknowledgements.
        while self.unconsumed_record_acks > 0 && !self.pending_data_file_acks.is_empty() {
            let (file_id, required_record_acks) =
                self.pending_data_file_acks.front().copied().unwrap();
            if self.unconsumed_record_acks >= required_record_acks {
                // We have enough unconsumed record acknowledgements to fully acknowledge this data
                // file. Remove it, consume the record acknowledgements, and delete the data file.
                _ = self.pending_data_file_acks.pop_front().unwrap();
                self.unconsumed_record_acks -= required_record_acks;

                assert!(
                    self.filesystem.delete_file(file_id),
                    "invariant violation: tried to delete file id {file_id}, but file does not exist"
                );
            } else {
                // Not enough delete acks to proceed, so we can't do anything more.
                break;
            }
        }
    }

    fn done_with_current_file(&mut self) -> bool {
        let current_file = self.current_file.as_ref().unwrap();
        let file_flushed_bytes = current_file.flushed_size();

        // If we've read as many bytes as there are flushed bytes in the file, and the file has been
        // finalized, then yes, we've gone as far as we can reading this file.
        self.current_file_bytes_read == file_flushed_bytes && current_file.is_finalized()
    }

    fn done_overall(&mut self) -> bool {
        let reader_file_id = self.ledger.get_reader_file_id();
        let writer_file_id = self.ledger.get_writer_file_id();
        let writer_done = self.ledger.is_writer_done();
        let buffer_empty = self.ledger.get_buffer_size() == 0;

        self.done_with_current_file()
            && buffer_empty
            && writer_done
            && reader_file_id == writer_file_id
    }

    fn check_ready(&mut self) -> bool {
        // If we have a data file open already, then we're good:
        if self.current_file.is_some() {
            return true;
        }

        // Try to open the file, and if it does not exist yet, we have to wait for the writer:
        let id = self.ledger.get_reader_file_id();
        match self.filesystem.open_file(id) {
            Some(file) => {
                self.current_file = Some(file);
                true
            }
            None => false,
        }
    }

    fn read_record(&mut self) -> Progress {
        self.state.transition_to_reading();

        loop {
            // Try and process acknowledgements before anything else.
            self.handle_acks();

            // If we can't open our desired current data file, we wait.
            if !self.check_ready() {
                return Progress::Blocked;
            }

            // If the writer is all done, and we've caught up to it, then we have no more records to read.
            if self.done_overall() {
                return Progress::RecordRead(None);
            }

            let current_file = self.current_file.as_ref().unwrap();
            if let Some((record, record_bytes)) = current_file.read() {
                self.track_read(record.event_count(), record_bytes);

                self.state.transition_to_idle();

                return Progress::RecordRead(Some(record));
            }

            // If we've read the entirety of the current data file, and it's finalized, we have to
            // move to the next one.
            if self.done_with_current_file() {
                self.move_to_next_file();
                continue;
            }

            return Progress::Blocked;
        }
    }

    fn track_read(&mut self, event_count: usize, bytes_read: u64) {
        // We need to track how many acknowledgements we expect to come in based on the number of
        // records read vs the number of their events that have been acknowledged, as we only adjust
        // the buffer size when a record has been fully acknowledged, since one record may contain
        // multiple events.
        self.outstanding_event_acks += 1;
        self.pending_record_acks
            .push_back((event_count, bytes_read));

        // Keep track of how much progress we've made in terms of this specific data file.
        self.current_file_records_read += 1;
        self.current_file_bytes_read += bytes_read;
    }

    fn acknowledge_read(&mut self) {
        // We check to make sure that we're not about to acknowledge more events than we actually
        // have read but have not yet been acknowledged, because that just should be possible.
        assert!(
            self.outstanding_event_acks > 0,
            "invariant violation: {} events unacked, tried to ack 1",
            self.outstanding_event_acks,
        );

        // Update the outstanding count of events which have not yet been acknowledged, and also
        // update the total number of acknowledgements we've gotten but have not yet been consumed,
        // as a record may have multiple events and they all need to be acknowledged before we can
        // actually count the record as fully acknowledged and removed from the buffer, etc.
        self.outstanding_event_acks -= 1;
        self.unconsumed_event_acks += 1;
    }
}

enum WriterModelState {
    Idle,
    PendingWrite,
    Closed,
}

impl WriterModelState {
    fn transition_to_idle(&mut self) {
        match self {
            Self::Idle => panic!("should not transition to idle when already idle"),
            Self::Closed => panic!("should not transition to idle when already closed"),
            Self::PendingWrite => *self = Self::Idle,
        }
    }

    fn transition_to_writing(&mut self) {
        match self {
            Self::Idle => *self = Self::PendingWrite,
            Self::PendingWrite { .. } => {}
            Self::Closed => panic!("should not transition to writing when already closed"),
        }
    }

    fn transition_to_closed(&mut self) -> bool {
        if let Self::Closed = self {
            false
        } else {
            *self = Self::Closed;
            true
        }
    }
}

/// Model for the writer.
struct WriterModel {
    filesystem: Arc<FilesystemModel>,
    ledger: Arc<LedgerModel>,
    current_file: Option<FileModel>,
    current_file_size: u64,
    current_file_full: bool,
    state: WriterModelState,
    record_writer: RecordWriter<Cursor<Vec<u8>>, Record>,
}

impl WriterModel {
    fn new(filesystem: Arc<FilesystemModel>, ledger: Arc<LedgerModel>) -> Self {
        let record_writer = RecordWriter::new(
            Cursor::new(Vec::new()),
            0,
            ledger.config().write_buffer_size,
            ledger.config().max_data_file_size,
            ledger.config().max_record_size,
        );

        let mut writer = Self {
            filesystem,
            ledger,
            current_file: None,
            current_file_size: 0,
            current_file_full: false,
            state: WriterModelState::Idle,
            record_writer,
        };

        // We do a dummy call to `check_ready` to simulate what happens when a real buffer is created,
        // as the real initialization process always ensures the current writer data file is created/exists.
        writer.check_ready();

        writer
    }

    fn get_archived_record_len(&mut self, record: Record) -> u64 {
        // We do a dummy `archive_record` call to simply do the work of encoding/archiving without
        // writing the value anywhere.  `RecordWriter` clears its encoding/serialization buffers on
        // each call to `archive_record` so we don't have to do any pre/post-cleanup to avoid memory
        // growth, etc.
        let record_len = record.archived_len();

        match self.record_writer.archive_record(1, record) {
            Ok(token) => token.serialized_len() as u64,
            Err(e) => panic!(
                "unexpected encode error: archived_len={} max_record_size={} error={:?}",
                record_len,
                self.ledger.config().max_record_size,
                e,
            ),
        }
    }

    fn get_current_buffer_size(&self) -> u64 {
        let unflushed_bytes = self
            .current_file
            .as_ref()
            .map_or(0, FileModel::unflushed_size);
        self.ledger.get_buffer_size() + unflushed_bytes
    }

    fn reset(&mut self) {
        self.current_file = None;
        self.current_file_size = 0;
        self.current_file_full = false;
    }

    fn try_finalize(&mut self) {
        if let Some(data_file) = self.current_file.as_ref() {
            data_file.finalize();
        }
    }

    fn flush(&mut self) {
        // Flush the current data file, if we have one open:
        let (flushed_events, flushed_bytes) = if let Some(data_file) = self.current_file.as_ref() {
            data_file.flush()
        } else {
            (0, 0)
        };

        self.track_flushed_events(flushed_events as u64, flushed_bytes);
    }

    fn track_flushed_events(&mut self, flushed_events: u64, flushed_bytes: u64) {
        self.ledger.increment_unread_events(flushed_events);
        self.ledger.increment_buffer_size(flushed_bytes);
    }

    fn check_ready(&mut self) -> bool {
        // If our buffer size is over the maximum buffer size, we have to wait for reader progress:
        if self.get_current_buffer_size() >= self.ledger.config().max_buffer_size {
            return false;
        }

        // If our current data file is at or above the limit, then flush it out, close it, and set
        // ourselves to open the next one:
        if self.current_file_full
            || self.current_file_size >= self.ledger.config().max_data_file_size
        {
            self.flush();

            let current_file = self.current_file.as_ref().unwrap();
            current_file.finalize();

            self.reset();
            self.ledger.increment_writer_file_id();
        }

        // At this point, we're not over any size limits, so if we have a file open already, we're
        // good to go.
        if self.current_file.is_some() {
            return true;
        }

        // Try to open the file, and if it already exists, we have to wait for the reader.
        // Otherwise, we _notify_ the reader, since they may be caught up and waiting on us to open
        // the next file and start reading from it:
        let id = self.ledger.get_writer_file_id();
        match self.filesystem.create_file(id) {
            Some(file) => {
                self.current_file = Some(file);
                true
            }
            None => false,
        }
    }

    fn write_record(&mut self, mut record: Record) -> Progress {
        // We don't accept writing records with an event count of zero:
        if record.event_count() == 0 {
            return Progress::WriteError(WriterError::EmptyRecord);
        }

        self.state.transition_to_writing();

        loop {
            // If we can't open our desired current data file, or the buffer is full, we wait.
            if !self.check_ready() {
                return Progress::Blocked;
            }

            // Check if the record would exceed the maximum record size, which should always fail.
            //
            // NOTE: Why are we using the "failed to encode" error here and not the "record too
            // large" error? As the comments in the actual writer code elucidate, the encoding may
            // fail when it actually tries to make sure there's enough space to encode itself, gets
            // told there isn't, and returns a generic error.  We don't know that from the generic
            // error alone, so we have some stand-in code that returns encoder errors as a
            // passthrough, and then it checks afterwards if the encoding buffer exceeds the
            // configured limit.
            //
            // This is a bit redundant as we explicitly limit the buffer we pass to the encoding
            // method to have a maximum capacity of whatever the maximum record limit is.. but it's
            // there to be thorough in the case of a bug on the `bytes` side.
            //
            // TODO: We should probably provide a generic error enum for encoding/decoding where the
            // type can tell us specifically if it ran out of space to encode itself, or if it hit
            // another general error... that's starting to nest the errors a bit deep, though, so
            // I'm not entirely sold.  For our purposes here, we know the expected error when the
            // record can't encode itself due to space limitations, so the differentiation on the
            // front end is more about providing an informative error, but the writer can't really
            // do anything different if they get "failed to encode" vs "record too large".
            let encoded_len = record
                .encoded_size()
                .expect("record used in model must provide this");
            let encoded_len_limit = self.ledger.config().max_record_size - RECORD_HEADER_LEN;
            if encoded_len > encoded_len_limit {
                return Progress::WriteError(WriterError::FailedToEncode {
                    source: EncodeError,
                });
            }

            // Write the record in the same way that the buffer would, which is the only way we can
            // calculate the true size that record occupies.
            let archived_len = self.get_archived_record_len(record.clone());

            // If this record would cause us to exceed the maximum data file size of the current data file, mark the
            // current data file full so that we can loop around and open the next one.
            if self.current_file_size + archived_len > self.ledger.config().max_data_file_size {
                self.current_file_full = true;
                continue;
            }

            // If this record would cause us to exceed our maximum buffer size, then the writer would have to wait for
            // the reader to make some sort of progress to try actually writing it.
            if self.get_current_buffer_size() + archived_len > self.ledger.config().max_buffer_size
            {
                return Progress::Blocked;
            }

            // Now try to "actually" write it, which may or may not fail depending on if the file is
            // full or not/could hold this record.  We archive the record manually, too, to get its true
            // on-disk size:
            let data_file = self
                .current_file
                .as_ref()
                .expect("current file must be present");
            match data_file.write(record, archived_len) {
                (Some(old_record), 0, 0) => {
                    // We would have overfilled the data file, so we need to open a new data file now
                    // and try again. We do this by setting the current file size to the maximum to
                    // trigger the logic to flush the old file, close it, and open the next one:
                    record = old_record;
                    self.current_file_size = self.ledger.config().max_data_file_size;

                    continue;
                }
                (None, flushed_events, flushed_bytes) => {
                    self.current_file_size += archived_len;

                    // We buffered the write but had to do some flushing to make it possible, so
                    // track the events that have now hit the data file.
                    self.track_flushed_events(flushed_events as u64, flushed_bytes);
                }
                _ => {
                    panic!("invariant violation: write can't flush if it has denied write overall")
                }
            }

            self.state.transition_to_idle();

            let written = archived_len.try_into().unwrap();
            return Progress::RecordWritten(written);
        }
    }

    fn close(&mut self) {
        if self.state.transition_to_closed() {
            self.ledger.mark_writer_done();
            self.try_finalize();
        }
    }
}

/// Model for the buffer.
struct BufferModel {
    ledger: Arc<LedgerModel>,
    reader: ReaderModel,
    writer: WriterModel,
}

impl BufferModel {
    fn from_config(config: &DiskBufferConfig<TestFilesystem>) -> Self {
        let filesystem = Arc::new(FilesystemModel::new(config));
        let ledger = Arc::new(LedgerModel::new(config));

        Self {
            ledger: Arc::clone(&ledger),
            reader: ReaderModel::new(Arc::clone(&filesystem), Arc::clone(&ledger)),
            writer: WriterModel::new(filesystem, ledger),
        }
    }

    fn ledger(&self) -> &LedgerModel {
        self.ledger.as_ref()
    }

    fn write_record(&mut self, record: Record) -> Progress {
        self.writer.write_record(record)
    }

    fn flush(&mut self) {
        self.writer.flush();
    }

    fn read_record(&mut self) -> Progress {
        self.reader.read_record()
    }

    fn acknowledge_read(&mut self) {
        self.reader.acknowledge_read();
    }

    fn close_writer(&mut self) {
        self.writer.close();
    }
}

proptest! {
    #[test]
    fn model_check(mut config in arb_buffer_config(), actions in arb_actions(0..64)) {
        let rt = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("should not fail to build runtime");

        let _a = install_tracing_helpers();
        info!(
            actions = actions.len(),
            max_buffer_size = config.max_buffer_size,
            max_data_file_size = config.max_data_file_size,
            max_record_size = config.max_record_size,
            "Starting model.",
        );

        // We generate a new temporary directory and overwrite the data directory in the buffer
        // configuration. This allows us to use a utility that will generate a random directory each
        // time -- parallel runs of this test can't clobber each other anymore -- but also ensure
        // that the directory is cleaned up when the test run is over.
        let buf_dir = TempDir::with_prefix("vector-buffers-disk-v2-model").expect("creating temp dir should never fail");
        config.data_dir = buf_dir.path().to_path_buf();

        rt.block_on(async move {
            // This model tries to encapsulate all of the behavior of the disk buffer v2
            // implementation, and has a few major parts that we'll briefly talk about: the model
            // itself, input actions, and the sequencer.
            //
            // At the very top, we have our input actions, which are mapped one-to-one with the
            // possible actions that can influence the disk buffer: reading records, writing
            // records, flushing writes, and acknowledging reads.
            //
            // After that, we have the model itself, which essentially a barebones re-implementation
            // of the disk buffer itself without any asynchrony, rich error handling, etc.  We scope
            // its behavior in certain ways -- no asynchrony, deterministic I/O, etc -- so that we
            // can focus on the core logic how what should the state of the disk buffer be after
            // executing a certain sequence of actions.
            //
            // Finally, we have the action sequencer.  As part of any property test, you inevitably
            // want to, and need to, test the actual system: the system under test, or SUT.  In our
            // case, however, our SUT is asynchronous, which represents a problem when we want to be
            // able to apply an action to it and observe the change in state without letting the
            // asynchronous runtime drive background computations or operations that might change
            // the state of the SUT before the next time we apply an action to it.  To deal with
            // this, we have the action sequencer.
            //
            // The action sequencer translates the input action into a real action against the SUT,
            // but holds the action future directly, instead of spawning it on a runtime to run to
            // its logical conclusion.  This lets us poll it in a one-shot mode, as well as
            // intercept its waker state.  Now, as we run one action that may need to logically wait
            // for an asynchronous wakeup from another operation occurring, we can determine if
            // running a subsequent action actually woke up the action that needed to wait.
            //
            // Beyond that, the action sequencer is also aware of the constraints around the API of
            // the buffer, such as the fact that all operations require mutable access, so there can
            // only be a write or flush operation in-flight for the writer, but not both, as well as
            // one read operation in-flight for the reader.
            //
            // Thus, the action sequencer can start operations, know when they've made progress and
            // should be tried again before pulling a new action from the remaining actions in the
            // input sequence, and so on.  Effectively, we can deterministically drive asynchronous
            // actions that are coupled to one another, in a lockstep fashion, with the model.
            let mut model = BufferModel::from_config(&config);

            let usage_handle = BufferUsageHandle::noop();
            let (writer, reader, ledger) =
                Buffer::<Record>::from_config_inner(config, usage_handle)
                    .await
                    .expect("should not fail to build buffer");

            let mut sequencer = ActionSequencer::new(actions, reader, writer);

            let mut closed_writers = false;

            loop {
                // The model runs in a current-thread tokio runtime,
                // but the acknowledgement handling runs in a
                // background task. This yields to any such background
                // task before returning in order to ensure
                // acknowledgements are fully accounted for before
                // doing the next operation.
                tokio::task::yield_now().await;

                // We manually check if the sequencer has any write operations left, either
                // in-flight or yet-to-be-triggered, and if none are left, we mark the writer
                // closed.  This allows us to properly inform the model that reads should start
                // returning `None` if there's no more flushed records left vs being blocked on
                // writer activity.
                if sequencer.all_write_operations_finished() && !closed_writers {
                    model.close_writer();
                    sequencer.close_writer();

                    closed_writers = true;
                }

                // If we were able to trigger a new action, see if it's an action we can immediately
                // run against the model.  If it's an action that may be asynchronous/blocked on
                // progress of another component, we try it later on, which lets us deduplicate some code.
                if let Some(action) = sequencer.trigger_next_runnable_action() {
                    if let Action::AcknowledgeRead = action {
                        // Acknowledgements are based on atomics, so they never wait asynchronously.
                        model.acknowledge_read();
                    }
                } else {
                    let mut made_progress = false;

                    // We had no triggerable action, so check if the sequencer has an in-flight write
                    // operation, or in-flight read operation, and see if either can complete.  If
                    // so, we then run them against the model.
                    if let Some((action, sut_result)) = sequencer.get_pending_write_action() {
                        match action {
                            Action::WriteRecord(record) => {
                                let model_result = model.write_record(record.clone());
                                match sut_result {
                                    // The SUT made no progress, so the model should not have made
                                    // progress either.
                                    Poll::Pending => prop_assert_eq!(model_result, Progress::Blocked, "expected blocked write"),
                                    Poll::Ready(sut_result) => {
                                        made_progress = true;
                                        match sut_result {
                                            WriteActionResult::Write(result) => match result {
                                                Ok(written) => prop_assert_eq!(model_result, Progress::RecordWritten(written), "expected completed write"),
                                                Err(e) => prop_assert_eq!(model_result, Progress::WriteError(e), "expected write error"),
                                            },
                                            WriteActionResult::Flush(r) => panic!("got unexpected flush action result for pending write: {r:?}"),
                                        }
                                    },
                                }
                            },
                            Action::FlushWrites => {
                                // Technically speaking, a flush should never actually block under test
                                // because we've made flushing deterministic with our filesystem
                                // implementation.  This is why despite the fact that the sequencer has
                                // to run it like an async operation, we don't check the result like an
                                // async operation.
                                model.flush();
                                match sut_result {
                                    Poll::Pending => panic!("flush should never be blocked"),
                                    Poll::Ready(sut_result) => match sut_result {
                                        WriteActionResult::Flush(result) => prop_assert!(result.is_ok()),
                                        WriteActionResult::Write(r) => panic!("got unexpected write action result for pending flush: {r:?}"),
                                    },
                                }
                            },
                            a => panic!("invalid action for pending write: {a:?}"),
                        }
                    }

                    if let Some((action, sut_result)) = sequencer.get_pending_read_action() {
                        match action {
                            Action::ReadRecord => {
                                let model_result = model.read_record();
                                match sut_result {
                                    // The SUT made no progress, so the model should not have made
                                    // progress either.
                                    Poll::Pending => prop_assert_eq!(model_result, Progress::Blocked, "expected blocked read"),
                                    Poll::Ready(sut_result) => {
                                        made_progress = true;
                                        match sut_result {
                                            ReadActionResult::Read(result) => match result {
                                                Ok(maybe_record) => prop_assert_eq!(model_result, Progress::RecordRead(maybe_record.clone()), "expected record read"),
                                                Err(e) => prop_assert_eq!(model_result, Progress::ReadError(e), "expected read error"),
                                            },
                                        }
                                    },
                                }
                            },
                            a => panic!("invalid action for pending read: {a:?}"),
                        }
                    }

                    // We need to detect if our model/SUT is correctly "stalled" based on the
                    // actions we've triggered so far, since the list of input actions may correctly
                    // lead to a test that cannot actually run all actions to completion.
                    //
                    // If we made any progress at all on an in-flight operation, we continue.
                    // Otherwise, we check if we're stuck, where "stuck" implies that we have an
                    // in-flight operation that is pending and no other
                    // runnable-but-yet-to-have-been-run actions that could otherwise unblock it.
                    if !made_progress && !sequencer.has_remaining_runnable_actions() {
                        // We have nothing else to trigger/run, so we have nothing that could
                        // possibly allow the in-flight operation(s) to complete, so we break.
                        break
                    }
                }
            }

            // The model/sequencer got as far as they could, so just check to make sure the
            // model/SUT are in agreement in terms of buffer size, unread records (events), etc:
            prop_assert_eq!(model.ledger().get_buffer_size(), ledger.get_total_buffer_size(),
                "model and SUT buffer size should be equal");

            // NOTE: We call them "unread events" here, but the buffer calls them "records".  A
            // record is generally a single write, which might represent one event or N events.
            // However, the record ID counter is advanced by the number of events in a record, so at
            // the end of the day, the "total record" count is really the total event count.
            prop_assert_eq!(model.ledger().get_unread_events(), ledger.get_total_records(),
                "model and SUT unread events should be equal");

            Ok(())
        })?;
    }
}
