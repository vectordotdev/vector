use std::{
    collections::{HashMap, VecDeque},
    io::Cursor,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    task::Poll,
};

use crossbeam_queue::SegQueue;
use parking_lot::Mutex;
use proptest::proptest;
use quickcheck::{QuickCheck, TestResult};
use tokio::runtime::Builder;

use crate::{
    buffer_usage_data::BufferUsageHandle,
    quickcheck_assert_eq,
    test::common::install_tracing_helpers,
    variants::disk_v2::{writer::RecordWriter, Buffer, DiskBufferConfig, DiskBufferConfigBuilder},
    WhenFull,
};

mod action;
use self::action::*;

mod common;
use self::common::*;

mod filesystem;
use self::filesystem::*;

mod record;
use self::record::*;

mod sequencer;
use self::sequencer::*;

struct FilesystemModel {
    files: Mutex<HashMap<u16, FileModel>>,
}

/// Models the all-in behavior of for a data file wrapped with a buffered writer.
#[derive(Clone, Default)]
struct FileModel {
    unflushed_records: Arc<SegQueue<(Record, u64)>>,
    unflushed_size: Arc<AtomicU64>,
    flushed_records: Arc<SegQueue<(Record, u64)>>,
    flushed_size: Arc<AtomicU64>,
}

impl FileModel {
    fn push_record(&self, record: Record, record_len: u64) {
        self.unflushed_records.push((record, record_len));
        self.unflushed_size.fetch_add(record_len, Ordering::Relaxed);
    }

    fn flush_records(&self) {
        while let Some((record, record_len)) = self.unflushed_records.pop() {
            self.unflushed_size.fetch_sub(record_len, Ordering::Relaxed);
            self.flushed_size.fetch_add(record_len, Ordering::Relaxed);
            self.flushed_records.push((record, record_len));
        }
    }
}

struct LedgerModel {
    config: DiskBufferConfig<TestFilesystem>,
    reader_woken: AtomicBool,
    writer_woken: AtomicBool,
}

impl LedgerModel {
    fn wake_reader(&self) {
        self.reader_woken.store(true, Ordering::Relaxed);
    }

    fn reader_woken(&self) -> bool {
        self.reader_woken.load(Ordering::Relaxed)
    }

    fn consume_reader_wakeup(&self) {
        self.reader_woken.store(false, Ordering::Relaxed);
    }

    fn wake_writer(&self) {
        self.writer_woken.store(true, Ordering::Relaxed);
    }

    fn writer_woken(&self) -> bool {
        self.writer_woken.load(Ordering::Relaxed)
    }

    fn consume_writer_wakeup(&self) {
        self.writer_woken.store(false, Ordering::Relaxed);
    }
}

enum ReaderModelState {
    Idle,
    PendingRead { polled_once: bool },
}

struct ReaderModel {
    ledger: Arc<LedgerModel>,
    current_file: Option<FileModel>,
    state: ReaderModelState,
}

enum WriterModelState {
    Idle,
    PendingWrite { polled_once: bool },
    PendingFlush { polled_once: bool },
    Closed,
}

struct WriterModel {
    ledger: Arc<LedgerModel>,
    current_file: Option<FileModel>,
    state: ReaderModelState,
}

struct NewModel {
    filesystem: Arc<FilesystemModel>,
    ledger: Arc<LedgerModel>,
    reader: ReaderModel,
    writer: WriterModel,
}

struct Model {
    current_data_file_size: u64,
    current_buffer_size: u64,
    unflushed_write_bytes: u64,
    current_write_buffer_size: u64,
    unflushed_records: Vec<Record>,
    flushed_records: VecDeque<Record>,
    unacked_reads: VecDeque<u64>,
    buffer_config: DiskBufferConfig<TestFilesystem>,
    writer_closed: bool,
    reader_woken: bool,
    writer_woken: bool,
    record_writer: RecordWriter<Cursor<Vec<u8>>, Record>,
}

// TODO: we should create two new model types to represent the reader and writer, respectively, so
// that we can better encapsulate the state changes, since things are starting to get a little too
// hairy otherwise.  we also need to be able to represent the concept of actions that are blocked
// still being checkable i.e. right now a read is always blocked if there's no wake up for the
// reader, but we should always be able to run a read _once_ before having to rely on being woken up
// to do a second, third, etc "poll" of that operation.
impl Model {
    fn from_config(config: &DiskBufferConfig<TestFilesystem>) -> Self {
        Self {
            current_data_file_size: 0,
            current_buffer_size: 0,
            unflushed_write_bytes: 0,
            current_write_buffer_size: 0,
            unflushed_records: Vec::new(),
            unacked_reads: VecDeque::new(),
            flushed_records: VecDeque::new(),
            buffer_config: config.clone(),
            writer_closed: false,
            reader_woken: false,
            writer_woken: false,
            record_writer: RecordWriter::new(
                Cursor::new(Vec::new()),
                0,
                config.write_buffer_size,
                config.max_data_file_size,
                config.max_record_size,
            ),
        }
    }

    fn close_writer(&mut self) {
        self.writer_closed = true;
        self.reader_woken = true;
    }

    fn get_archived_record_len(&mut self, record: Record) -> u64 {
        // We do a dummy `archive_record` call to simply do the work of encoding/archiving without
        // writing the value anywhere.  `RecordWriter` clears its encoding/serialization buffers on
        // each call to `archive_record` so we don't have to do any pre/post-cleanup to avoid memory
        // growth, etc.
        self.record_writer
            .archive_record(1, record)
            .expect("detached record archiving should not fail") as u64
    }

    fn write_record(&mut self, record: Record) -> Progress {
        self.writer_ensure_ready();

        // Check if the record would exceed the maximum record size, which should always fail.
        if record.len() > self.buffer_config.max_record_size {
            return Progress::WriteError;
        }

        // Write the record in the same way that the buffer would, which is the only way we can
        // calculate the true size that record occupies.
        let archived_len = self.get_archived_record_len(record.clone());

        // Any time a write occurs with `tokio::io::BufWriter`, it checks if the incoming write
        // would cause the internal buffer to overflow, and if it would, it flushes whatever it has
        // first before proceeding with actually buffering the write.
        if self.current_write_buffer_size + archived_len > TEST_WRITE_BUFFER_SIZE {
            self.flush_writes(false);
        }

        // Now store the record and adjust our offsets, sizes, etc.
        //
        // TODO: Handle logic for the current data file going over its size limit and needing to
        // roll over to the next one.
        // TODO: Handle logic for blocking writes when the overall buffer size goes over the
        // configured limit and needs to wait for reads to happen that reduce it.
        self.unflushed_records.push(record);
        self.unflushed_write_bytes += archived_len;
        self.current_data_file_size += archived_len;
        self.current_write_buffer_size += archived_len;

        // Additionally, if a write is bigger than the actual internal write buffer capacity, the
        // buffered writer will just write directly to the writer it's wrapping.  We simulate that
        // here by just immediately flushing if our write was big enough.
        if self.current_write_buffer_size >= TEST_WRITE_BUFFER_SIZE {
            self.flush_writes(false);
        }

        Progress::RecordWritten(archived_len as usize)
    }

    fn writer_ensure_ready(&mut self) {
        if self.current_data_file_size >= self.buffer_config.max_data_file_size {
            debug!("flushing writes and rolling to next data file");
            self.flush_writes(true);
            self.current_data_file_size = 0;
        }
    }

    fn flush_writes(&mut self, notify_reader: bool) {
        self.flushed_records
            .extend(self.unflushed_records.drain(..));
        self.current_write_buffer_size = 0;
        self.current_buffer_size += self.unflushed_write_bytes;
        self.unflushed_write_bytes = 0;

        if notify_reader {
            self.reader_woken = true;
        }
    }

    fn read_record(&mut self) -> Progress {
        if !self.reader_woken {
            return Progress::Blocked;
        }

        self.reader_woken = false;
        match self.flushed_records.pop_front() {
            None => {
                if self.writer_closed && self.current_buffer_size == 0 {
                    Progress::RecordRead(None)
                } else {
                    Progress::Blocked
                }
            }
            Some(record) => {
                let archive_len = self.get_archived_record_len(record.clone());
                self.unacked_reads.push_back(archive_len);

                Progress::RecordRead(Some(record))
            }
        }
    }

    fn acknowledge_read(&mut self) {
        if let Some(unacked_read) = self.unacked_reads.pop_front() {
            self.current_buffer_size -= unacked_read;
            self.writer_woken = true;
        } else {
            panic!("tried to acknowledge read that did not exist");
        }
    }
}

proptest! {
    #[test]
    fn model_check(config in arb_buffer_config(), actions in arb_actions(0..10)) {
        let _ = install_tracing_helpers();

        info!(message = "starting new model check run",
            actions = actions.len(), max_buffer_size = config.max_buffer_size,
            max_data_file_size = config.max_data_file_size,
            max_record_size = config.max_record_size, flush_interval = ?config.flush_interval);

        let rt = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("should not fail to build runtime");

        rt.block_on(async move {
            // Create our model, which is the high-level representation of how we expect the system
            // under test (SUT) to behave.
            let mut model = Model::from_config(&config);

            // Create our actual SUT, including the `ActionSequencer` which allows us to enforce the
            // type system constraints/SUT usage contract constraints in a step-wise fashion.
            //
            // The doc comments for `ActionSequencer` explain this in more detail.
            let usage_handle = BufferUsageHandle::noop(WhenFull::Block);
            let (writer, reader, acker) =
                Buffer::<Record>::from_config(config, usage_handle)
                    .await
                    .expect("should not fail to build buffer");

            let mut sequencer = ActionSequencer::new(actions, reader, writer, acker);

            let mut loop_id = 0;
            debug!("starting iteration");

            loop {
                loop_id += 1;
                trace!("starting loop #{}", loop_id);

                // We manully check if the sequencer has any write operations left, either
                // in-flight or yet-to-be-triggered, and if none are left, we mark the writer
                // closed.  This allows us to properly inform the model that reads should start
                // returning `None` if there's no more flushed records left vs being blocked on
                // writer activity.
                if sequencer.all_write_operations_finished() {
                    model.close_writer();
                    sequencer.close_writer();
                }

                // If we were able to trigger a new action, see if it's an action we can immediately
                // run against the model.  If it's an action that may be asynchronous/blocked on
                // progress of another component, we try it later on, which lets us deduplicate some code.
                if let Some(action) = sequencer.trigger_next_runnable_action() {
                    debug!("triggered action: {:?}", action);

                    match action {
                        Action::AcknowledgeRead => model.acknowledge_read(),
                        _ => {},
                    }
                } else {
                    debug!("no triggerable action, checking for in-flight actions...");
                    let mut made_progress = false;

                    // We had no triggerable action, so check if the sequencer has an in-flight write
                    // operation, or in-flight read operation, and see if either can complete.  If
                    // so, we then run them against the model.
                    if let Some((action, sut_result)) = sequencer.get_pending_write_action() {
                        debug!("found in-flight write action: {:?}", action);
                        match action {
                            Action::WriteRecord(record) => {
                                let model_result = model.write_record(record.clone());
                                debug!("pending write result: model={:?}, SUT={:?}", model_result, sut_result);

                                match sut_result {
                                    // The SUT made no progress, so the model should not have made
                                    // progress either.
                                    Poll::Pending => assert_eq!(model_result, Progress::Blocked, "expected blocked write"),
                                    Poll::Ready(sut_result) => {
                                        made_progress = true;
                                        match sut_result {
                                            WriteActionResult::Write(result) => match result {
                                                Ok(written) => {
                                                    assert_eq!(model_result, Progress::RecordWritten(written), "expected completed write");
                                                    debug!("completed writing record: {:?}", record);
                                                },
                                                // TODO: Should we go deeper and try to directly compare the
                                                // internal error variant?
                                                Err(e) => {
                                                    assert_eq!(model_result, Progress::WriteError, "expected write error");
                                                    debug!("error while writing record: {:?}", e);
                                                },
                                            },
                                            r => panic!("got unexpected write action result for pending write: {:?}", r),
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
                                model.flush_writes(true);
                                match sut_result {
                                    Poll::Pending => panic!("flush should never be blocked"),
                                    Poll::Ready(sut_result) => match sut_result {
                                        WriteActionResult::Flush(result) => assert!(result.is_ok()),
                                        r => panic!("got unexpected write action result for pending flush: {:?}", r),
                                    },
                                }
                            },
                            a => panic!("invalid action for pending write: {:?}", a),
                        }
                        debug!("done trying to drive in-flight write action");
                    }

                    if let Some((action, sut_result)) = sequencer.get_pending_read_action() {
                        debug!("found in-flight read action: {:?}", action);
                        match action {
                            Action::ReadRecord => {
                                let model_result = model.read_record();
                                debug!("pending read result: model={:?}, SUT={:?}", model_result, sut_result);

                                match sut_result {
                                    // The SUT made no progress, so the model should not have made
                                    // progress either.
                                    Poll::Pending => assert_eq!(model_result, Progress::Blocked, "expected blocked read"),
                                    Poll::Ready(sut_result) => {
                                        made_progress = true;
                                        match sut_result {
                                            ReadActionResult::Read(result) => match result {
                                                Ok(maybe_record) => {
                                                    assert_eq!(model_result, Progress::RecordRead(maybe_record.clone()), "expected record read");
                                                    debug!("record read result: {:?}", maybe_record);
                                                },
                                                // TODO: Should we go deeper and try to directly compare the
                                                // internal error variant?
                                                Err(e) => {
                                                    assert_eq!(model_result, Progress::ReadError, "expected read error");
                                                    debug!("error while reading record: {:?}", e);
                                                },
                                            },
                                        }
                                    },
                                }
                            },
                            a => panic!("invalid action for pending read: {:?}", a),
                        }
                        debug!("done trying to drive in-flight read action");
                    }

                    // We need to detect if our model/SUT is correctly "stalled" based on the
                    // actions we've triggered so far, since the list of input actions may correctly
                    // lead to a test that cannot actually run all actions to completion.
                    //
                    // If we made any progress at all on an in-flight operation, we continue.
                    // Otherwise, we check if we're stuck, where "stuck" implies that we have an
                    // in-flight operation that is pending and no other
                    // runnable-but-yet-to-have-been-run actions that could otherwise unblock it.
                    if made_progress {
                        debug!("made progress with in-flight action");
                    } else {
                        if !sequencer.has_remaining_runnable_actions() {
                            // We have nothing else to trigger/run, so we have nothing that could
                            // possibly allow the in-flight operation(s) to complete, so we break.
                            debug!("model/SUT progress stalled, breaking");
                            break
                        }
                    }
                }
            }

            // TODO: Compare model state with reader/writer/ledger state.
        });
    }
}
