use std::{
    cmp,
    collections::{HashMap, VecDeque},
    error,
    f32::consts::E,
    fmt,
    io::{self, Cursor},
    mem,
    num::{NonZeroU16, NonZeroU32},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use async_trait::async_trait;
use bytes::{Buf, BufMut};
use futures::{future::BoxFuture, FutureExt};
use parking_lot::Mutex;
use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    runtime::Builder,
};
use tokio_test::task::{spawn, Spawn};
use vector_common::byte_size_of::ByteSizeOf;

use crate::{
    buffer_usage_data::BufferUsageHandle,
    disk_v2::{
        io::{AsyncFile, Metadata, ReadableMemoryMap, WritableMemoryMap},
        tests::install_tracing_helpers,
        writer::RecordWriter,
        Buffer, DiskBufferConfig, DiskBufferConfigBuilder, Filesystem, Reader, ReaderError, Writer,
        WriterError,
    },
    encoding::{DecodeBytes, EncodeBytes},
    Acker,
};

type TestReader = Reader<Record, TestFilesystem>;
type TestWriter = Writer<Record, TestFilesystem>;
type ReaderResult<T> = Result<T, ReaderError<Record>>;
type WriterResult<T> = Result<T, WriterError<Record>>;

// This is specifically set at 60KB because we allow a maximum record size of up to 64KB, and so
// we'd likely to occassionally encounter a record that, when encoded, is larger than the write
// buffer overall, which exercises the "write this record directly to the wrapped writer" logic that
// exists in `tokio::io::BufWriter` itself.
const TEST_WRITE_BUFFER_SIZE: u64 = 60 * 1024;

macro_rules! quickcheck_assert_eq {
    ($expected:expr, $actual:expr, $reason:expr) => {{
        if $expected != $actual {
            return TestResult::error($reason);
        }
    }};
}

#[derive(Debug)]
pub struct EncodeError;

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for EncodeError {}

#[derive(Debug)]
pub struct DecodeError;

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for DecodeError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    id: u64,
    size: u32,
}

impl Record {
    pub(crate) const fn new(id: u64, size: u32) -> Self {
        Record { id, size }
    }

    const fn header_len() -> usize {
        mem::size_of::<u64>() + mem::size_of::<u32>()
    }

    const fn len(&self) -> usize {
        Self::header_len() + self.size as usize
    }
}

impl ByteSizeOf for Record {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

impl Arbitrary for Record {
    fn arbitrary(g: &mut Gen) -> Self {
        Record {
            id: u64::arbitrary(g),
            size: NonZeroU32::arbitrary(g).get(),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let id = self.id;
        let size = self.size;

        Box::new(
            id.shrink()
                .zip(size.shrink())
                .map(|(id, size)| Record::new(id, size)),
        )
    }
}

impl EncodeBytes for Record {
    type Error = EncodeError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::Error>
    where
        B: BufMut,
        Self: Sized,
    {
        if buffer.remaining_mut() < self.len() {
            return Err(EncodeError);
        }

        buffer.put_u64(self.id);
        buffer.put_u32(self.size);
        buffer.put_bytes(0x42, self.size as usize);
        Ok(())
    }

    fn encoded_size(&self) -> Option<usize> {
        Some(self.len())
    }
}

impl DecodeBytes for Record {
    type Error = DecodeError;

    fn decode<B>(mut buffer: B) -> Result<Self, Self::Error>
    where
        B: Buf,
        Self: Sized,
    {
        if buffer.remaining() < Self::header_len() {
            return Err(DecodeError);
        }

        let id = buffer.get_u64();
        let size = buffer.get_u32();

        if buffer.remaining() < size as usize {
            return Err(DecodeError);
        }

        let payload = buffer.copy_to_bytes(size as usize);
        let valid = &payload.iter().all(|b| *b == 0x42);
        if !valid {
            return Err(DecodeError);
        }

        Ok(Record::new(id, size))
    }
}

#[derive(Clone, Debug)]
enum Action {
    WriteRecord(Record),
    FlushWrites,
    ReadRecord,
    // TODO: It'd be nice to allow parameterizing the amount of reads to acknowledge, but we don't
    // have the logic to bound the amount we acknowledge on the ledger side, so doing so here would
    // cause very bad breakage until we added support for that.
    AcknowledgeRead,
}

impl Action {
    fn is_write(&self) -> bool {
        match self {
            Self::WriteRecord(_) | Action::FlushWrites => true,
            _ => false,
        }
    }

    fn is_read(&self) -> bool {
        match self {
            Self::ReadRecord => true,
            _ => false,
        }
    }

    fn is_ack(&self) -> bool {
        match self {
            Self::AcknowledgeRead => true,
            _ => false,
        }
    }
}

impl Arbitrary for Action {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        match u8::arbitrary(g) % 4 {
            0 => {
                // Match our record payload size to just a little bit above our max_record_size to
                // try and reduce the number of actions that quickcheck generates with records that
                // are too large.
                let record_size = u16::arbitrary(g) as u32 + u8::arbitrary(g) as u32;
                let record = Record::new(u64::arbitrary(g), record_size);
                Action::WriteRecord(record)
            }
            1 => Action::FlushWrites,
            2 => Action::ReadRecord,
            _ => Action::AcknowledgeRead,
        }
    }
}

#[derive(Debug, PartialEq)]
enum Progress {
    WroteRecord(usize),
    WriteError,
    ReadRecord(Option<Record>),
    ReadError,
    Blocked,
}

fn io_err_already_exists() -> io::Error {
    io::Error::new(io::ErrorKind::AlreadyExists, "file already exists")
}

fn io_err_not_found() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "file not found")
}

fn io_err_permission_denied() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, "permission denied")
}

struct FileInner {
    buf: Option<Vec<u8>>,
}

impl FileInner {
    fn consume_buf(&mut self) -> Vec<u8> {
        self.buf.take().expect("tried to consume buf, but empty")
    }

    fn return_buf(&mut self, buf: Vec<u8>) {
        let previous = self.buf.replace(buf);
        assert!(previous.is_none());
    }
}

impl Default for FileInner {
    fn default() -> Self {
        Self {
            buf: Some(Vec::new()),
        }
    }
}

impl fmt::Debug for FileInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let buf_debug = match &self.buf {
            None => String::from("(none)"),
            Some(buf) => format!("({} bytes)", buf.len()),
        };

        f.debug_struct("FileInner")
            .field("buf", &buf_debug)
            .finish()
    }
}

#[derive(Clone, Debug)]
struct TestFile {
    inner: Arc<Mutex<FileInner>>,
    is_writable: bool,
    read_pos: usize,
}

impl TestFile {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FileInner::default())),
            is_writable: false,
            read_pos: 0,
        }
    }

    fn set_readable(&mut self) {
        self.is_writable = false;
    }

    fn set_writable(&mut self) {
        self.is_writable = true;
    }

    fn as_mmap(&self) -> TestMmap {
        let inner = Arc::clone(&self.inner);
        inner.into()
    }
}

struct TestMmap {
    inner: Arc<Mutex<FileInner>>,
    buf: Option<Vec<u8>>,
}

impl From<Arc<Mutex<FileInner>>> for TestMmap {
    fn from(inner: Arc<Mutex<FileInner>>) -> Self {
        let buf = {
            let mut guard = inner.lock();
            guard.consume_buf()
        };

        Self {
            inner,
            buf: Some(buf),
        }
    }
}

impl Drop for TestMmap {
    fn drop(&mut self) {
        let buf = self.buf.take().expect("buf must exist");
        let mut inner = self.inner.lock();
        inner.return_buf(buf);
    }
}

impl AsRef<[u8]> for TestMmap {
    fn as_ref(&self) -> &[u8] {
        self.buf.as_ref().expect("mmap buf consumed").as_slice()
    }
}

impl ReadableMemoryMap for TestMmap {}

impl WritableMemoryMap for TestMmap {
    fn flush(&self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncRead for TestFile {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let new_read_pos = {
            let mut inner = self.inner.lock();
            let src = inner.buf.as_mut().expect("file buf consumed");

            let cap = buf.remaining();
            let pos = self.read_pos;
            let available = src.len() - pos;
            let n = cmp::min(cap, available);

            let to = pos + n;
            buf.put_slice(&src[pos..to]);
            to
        };

        self.read_pos = new_read_pos;

        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for TestFile {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if !self.is_writable {
            return Err(io_err_permission_denied()).into();
        }

        let mut inner = self.inner.lock();
        let dst = inner.buf.as_mut().expect("file buf consumed");
        dst.extend_from_slice(buf);

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if !self.is_writable {
            return Err(io_err_permission_denied()).into();
        }

        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if !self.is_writable {
            return Err(io_err_permission_denied()).into();
        }

        Poll::Ready(Ok(()))
    }
}

#[async_trait]
impl AsyncFile for TestFile {
    async fn metadata(&self) -> io::Result<Metadata> {
        let len = {
            let inner = self.inner.lock();
            inner.buf.as_ref().expect("file buf consumed").len()
        };

        Ok(Metadata { len: len as u64 })
    }

    async fn sync_all(&self) -> io::Result<()> {
        Ok(())
    }
}

// Inner state of the test filesystem.
#[derive(Debug, Default)]
struct FilesystemInner {
    files: HashMap<PathBuf, TestFile>,
}

impl FilesystemInner {
    fn open_file_writable(&mut self, path: &PathBuf) -> TestFile {
        let file = self
            .files
            .entry(path.clone())
            .or_insert_with(|| TestFile::new());
        let mut new_file = file.clone();
        new_file.set_writable();

        new_file
    }

    fn open_file_writable_atomic(&mut self, path: &PathBuf) -> Option<TestFile> {
        if self.files.contains_key(path) {
            None
        } else {
            let mut new_file = TestFile::new();
            new_file.set_writable();

            self.files.insert(path.clone(), new_file.clone());

            Some(new_file)
        }
    }

    fn open_file_readable(&mut self, path: &PathBuf) -> Option<TestFile> {
        self.files.get(path).cloned().map(|mut f| {
            f.set_readable();
            f
        })
    }

    fn open_mmap_readable(&mut self, path: &PathBuf) -> Option<TestMmap> {
        self.files.get(path).map(|f| f.as_mmap())
    }

    fn open_mmap_writable(&mut self, path: &PathBuf) -> Option<TestMmap> {
        self.files.get(path).map(|f| f.as_mmap())
    }

    fn delete_file(&mut self, path: &PathBuf) -> bool {
        self.files.remove(path).is_some()
    }
}

/// A `Filesystem` that tracks files in memory and allows introspection from the outside.
#[derive(Debug)]
struct TestFilesystem {
    inner: Arc<Mutex<FilesystemInner>>,
}

impl TestFilesystem {
    fn get_view(&self) -> TestFilesystemView {
        TestFilesystemView {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Clone for TestFilesystem {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Default for TestFilesystem {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FilesystemInner::default())),
        }
    }
}

struct TestFilesystemView {
    inner: Arc<Mutex<FilesystemInner>>,
}

#[async_trait]
impl Filesystem for TestFilesystem {
    type File = TestFile;
    type MemoryMap = TestMmap;
    type MutableMemoryMap = TestMmap;

    async fn open_file_writable(&self, path: &PathBuf) -> io::Result<Self::File> {
        let mut inner = self.inner.lock();
        Ok(inner.open_file_writable(path))
    }

    async fn open_file_writable_atomic(&self, path: &PathBuf) -> io::Result<Self::File> {
        let mut inner = self.inner.lock();
        match inner.open_file_writable_atomic(path) {
            Some(file) => Ok(file),
            None => Err(io_err_already_exists()),
        }
    }

    async fn open_file_readable(&self, path: &PathBuf) -> io::Result<Self::File> {
        let mut inner = self.inner.lock();
        match inner.open_file_readable(path) {
            Some(file) => Ok(file),
            None => Err(io_err_not_found()),
        }
    }

    async fn open_mmap_readable(&self, path: &PathBuf) -> io::Result<Self::MemoryMap> {
        let mut inner = self.inner.lock();
        match inner.open_mmap_readable(path) {
            Some(mmap) => Ok(mmap),
            None => Err(io_err_not_found()),
        }
    }

    async fn open_mmap_writable(&self, path: &PathBuf) -> io::Result<Self::MutableMemoryMap> {
        let mut inner = self.inner.lock();
        match inner.open_mmap_writable(path) {
            Some(mmap) => Ok(mmap),
            None => Err(io_err_not_found()),
        }
    }

    async fn delete_file(&self, path: &PathBuf) -> io::Result<()> {
        let mut inner = self.inner.lock();
        if inner.delete_file(path) {
            Ok(())
        } else {
            Err(io_err_not_found())
        }
    }
}

impl Arbitrary for DiskBufferConfigBuilder<TestFilesystem> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        // We limit our buffer size, data file size, and record size to make sure that quickcheck
        // can actually make some reasonable progress.  We do this in a ratio of 64 to 2 to 1,
        // respectively.  This should allow exercising the file ID limit in test mode (32 data files
        // max) but not constantly hitting it.
        let max_buffer_size = NonZeroU16::arbitrary(g).get() as u64 * 64;
        let max_data_file_size = NonZeroU16::arbitrary(g).get() as u64 * 2;
        let max_record_size = NonZeroU16::arbitrary(g).get() as usize;

        let mut path = std::env::temp_dir();
        path.push("vector-disk-v2-model");

        DiskBufferConfigBuilder::from_path(path)
            .max_buffer_size(max_buffer_size)
            .max_data_file_size(max_data_file_size)
            .max_record_size(max_record_size)
            .write_buffer_size(TEST_WRITE_BUFFER_SIZE as usize)
            .flush_interval(Duration::arbitrary(g))
            .filesystem(TestFilesystem::default())
    }

    /*fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let data_dir = self.data_dir.clone();
        let max_buffer_size = self
            .max_buffer_size
            .expect("max_buffer_size must be specified")
            .shrink();
        let max_data_file_size = self
            .max_data_file_size
            .expect("max_data_file_size must be specified")
            .shrink();
        let max_record_size = self
            .max_record_size
            .expect("max_record_size must be specified")
            .shrink();
        let flush_interval = self
            .flush_interval
            .expect("flush_interval must be specified")
            .shrink();

        let params = max_buffer_size
            .zip(max_data_file_size)
            .zip(max_record_size)
            .zip(flush_interval);
        Box::new(params.map(
            move |(((max_buffer_size, max_data_file_size), max_record_size), flush_interval)| {
                DiskBufferConfigBuilder::from_path(data_dir.clone())
                    .max_buffer_size(max_buffer_size)
                    .max_data_file_size(max_data_file_size)
                    .max_record_size(max_record_size)
                    .flush_interval(flush_interval)
                    .filesystem(TestFilesystem::default())
            },
        ))
    }*/
}

struct Model {
    max_data_file_size: u64,
    max_buffer_size: u64,
    max_record_size: usize,
    current_data_file_size: u64,
    current_buffer_size: u64,
    current_write_buffer_size: u64,
    unflushed_records: Vec<Record>,
    flushed_records: VecDeque<Record>,
    unacked_reads: u64,
    buffer_config: DiskBufferConfig<TestFilesystem>,
    writer_closed: bool,
    record_writer: RecordWriter<Cursor<Vec<u8>>, Record>,
}

impl Model {
    fn from_config(config: &DiskBufferConfig<TestFilesystem>) -> Self {
        Self {
            max_data_file_size: config.max_data_file_size,
            max_buffer_size: config.max_buffer_size,
            max_record_size: config.max_record_size,
            current_data_file_size: 0,
            current_buffer_size: 0,
            current_write_buffer_size: 0,
            unflushed_records: Vec::new(),
            unacked_reads: 0,
            flushed_records: VecDeque::new(),
            buffer_config: config.clone(),
            writer_closed: false,
            record_writer: RecordWriter::new(
                Cursor::new(Vec::new()),
                config.write_buffer_size,
                config.max_record_size,
            ),
        }
    }

    fn close_writer(&mut self) {
        self.writer_closed = true;
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
            self.flush_writes();
        }

        // Now store the record and adjust our offsets, sizes, etc.
        //
        // TODO: Handle logic for the current data file going over its size limit and needing to
        // roll over to the next one.
        // TODO: Handle logic for blocking writes when the overall buffer size goes over the
        // configured limit and needs to wait for reads to happen that reduce it.
        self.unflushed_records.push(record);
        self.current_buffer_size += archived_len;
        self.current_data_file_size += archived_len;
        self.current_write_buffer_size += archived_len;

        // Additionally, if a write is bigger than the actual internal write buffer capacity, the
        // buffered writer will just write directly to the writer it's wrapping.  We simulate that
        // here by just immediately flushing if our write was big enough.
        if self.current_write_buffer_size >= TEST_WRITE_BUFFER_SIZE {
            self.flush_writes();
        }

        Progress::WroteRecord(archived_len as usize)
    }

    fn flush_writes(&mut self) {
        self.flushed_records
            .extend(self.unflushed_records.drain(..));
        self.current_write_buffer_size = 0;
    }

    fn read_record(&mut self) -> Progress {
        match self.flushed_records.pop_front() {
            None => {
                if self.writer_closed {
                    Progress::ReadRecord(None)
                } else {
                    Progress::Blocked
                }
            }
            Some(record) => {
                let archive_len = self.get_archived_record_len(record.clone());
                self.current_buffer_size -= archive_len;
                self.unacked_reads += 1;

                Progress::ReadRecord(Some(record))
            }
        }
    }

    fn acknowledge_read(&mut self) {
        if let Some(unacked_reads) = self.unacked_reads.checked_sub(1) {
            self.unacked_reads = unacked_reads;
        } else {
            panic!("tried to acknowledge read that did not exist");
        }
    }
}

enum ReadState {
    Inconsistent,
    Idle(TestReader),
    PendingRead(Spawn<BoxFuture<'static, (TestReader, ReaderResult<Option<Record>>)>>),
}

impl ReadState {
    fn is_idle(&self) -> bool {
        matches!(self, ReadState::Idle(_))
    }

    fn state_name(&self) -> &'static str {
        match self {
            ReadState::Inconsistent => "inconsistent",
            ReadState::Idle(_) => "idle",
            ReadState::PendingRead(_) => "pending_read",
        }
    }

    fn transition_to_read(&mut self) {
        let new_state = match mem::replace(self, ReadState::Inconsistent) {
            ReadState::Idle(mut reader) => {
                let fut = async move {
                    let result = reader.next().await;
                    (reader, result)
                };
                let spawned = spawn(fut.boxed());
                ReadState::PendingRead(spawned)
            }
            s => panic!(
                "tried to transition to pending read from state other than idle: {}",
                s.state_name()
            ),
        };
        *self = new_state;
    }
}

enum WriteState {
    Inconsistent,
    Idle(TestWriter),
    PendingWrite(
        Record,
        Spawn<BoxFuture<'static, (TestWriter, WriterResult<usize>)>>,
    ),
    PendingFlush(Spawn<BoxFuture<'static, (TestWriter, io::Result<()>)>>),
    Closed,
}

impl WriteState {
    fn is_idle(&self) -> bool {
        matches!(self, WriteState::Idle(_))
    }

    fn is_closed(&self) -> bool {
        matches!(self, WriteState::Closed)
    }

    fn state_name(&self) -> &'static str {
        match self {
            WriteState::Inconsistent => "inconsistent",
            WriteState::Idle(_) => "idle",
            WriteState::PendingWrite(_, _) => "pending_write",
            WriteState::PendingFlush(_) => "pending_flush",
            WriteState::Closed => "closed",
        }
    }

    fn transition_to_write(&mut self, record: Record) {
        let new_state = match mem::replace(self, WriteState::Inconsistent) {
            WriteState::Idle(mut writer) => {
                let cloned_record = record.clone();
                let fut = async move {
                    let result = writer.write_record(record).await;
                    (writer, result)
                };
                let spawned = spawn(fut.boxed());
                WriteState::PendingWrite(cloned_record, spawned)
            }
            s => panic!(
                "tried to transition to pending write from state other than idle: {}",
                s.state_name()
            ),
        };
        *self = new_state;
    }

    fn transition_to_flush(&mut self) {
        let new_state = match mem::replace(self, WriteState::Inconsistent) {
            WriteState::Idle(mut writer) => {
                let fut = async move {
                    let result = writer.flush().await;
                    (writer, result)
                };
                let spawned = spawn(fut.boxed());
                WriteState::PendingFlush(spawned)
            }
            s => panic!(
                "tried to transition to pending flush from state other than idle: {}",
                s.state_name()
            ),
        };
        *self = new_state;
    }

    fn transition_to_closed(&mut self) {
        let new_state = match mem::replace(self, WriteState::Inconsistent) {
            WriteState::Idle(mut writer) => {
                // Technically, dropping the writer alone would also close the writer, logically,
                // but I'm doing it explicitly here for my own sanity when reading the code.
                writer.close();
                WriteState::Closed
            }
            // Already closed, nothing else to do.
            WriteState::Closed => WriteState::Closed,
            s => panic!(
                "tried to transition to closed from state other than idle: {}",
                s.state_name()
            ),
        };
        *self = new_state;
    }
}

#[derive(Debug)]
enum ReadActionResult {
    Read(ReaderResult<Option<Record>>),
}

#[derive(Debug)]
enum WriteActionResult {
    Write(WriterResult<usize>),
    Flush(io::Result<()>),
}

struct ActionSequencer {
    actions: Vec<Action>,
    read_state: ReadState,
    write_state: WriteState,
    acker: Acker,
    unacked_reads: u64,
}

impl ActionSequencer {
    fn new(actions: Vec<Action>, reader: TestReader, writer: TestWriter, acker: Acker) -> Self {
        Self {
            actions,
            read_state: ReadState::Idle(reader),
            write_state: WriteState::Idle(writer),
            acker,
            unacked_reads: 0,
        }
    }

    /// Whether or not this sequencer has any actions left to trigger.
    fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    fn all_write_operations_finished(&self) -> bool {
        (self.write_state.is_idle() || self.write_state.is_closed())
            && self.actions.iter().all(|a| !a.is_write())
    }

    fn close_writer(&mut self) {
        self.write_state.transition_to_closed();
    }

    fn get_next_runnable_action(&self) -> Option<usize> {
        let allow_write = self.write_state.is_idle();
        let allow_read = self.read_state.is_idle();

        self.actions.iter().position(|a| match a {
            Action::WriteRecord(_) => allow_write,
            Action::FlushWrites => allow_write,
            Action::ReadRecord => allow_read,
            Action::AcknowledgeRead => self.unacked_reads > 0,
        })
    }

    fn has_remaining_runnable_actions(&self) -> bool {
        self.get_next_runnable_action().is_some()
    }

    /// Triggers the next runnable action.
    ///
    /// If an action is eligible to run, then it will be automatically run and the action itself
    /// will be returned to the caller so it may be applied against the model.  If none of ther
    /// remaining actions are eligible to run, then `None` is returned.
    ///
    /// For example, if there's an in-flight write, we can't execute another write, or a flush.
    /// Likewise, we can't execute another read if there's an in-flight read.  Acknowledgements
    /// always happen out-of-band, though, and so are always eligible.
    fn trigger_next_runnable_action(&mut self) -> Option<Action> {
        let pos = self.get_next_runnable_action();

        if let Some(action) = pos.map(|i| self.actions.remove(i)) {
            match action {
                Action::WriteRecord(record) => {
                    if !self.write_state.is_idle() {
                        panic!("got write action when write state is not idle");
                    }

                    self.write_state.transition_to_write(record.clone());
                    Some(Action::WriteRecord(record))
                }
                a @ Action::FlushWrites => {
                    if !self.write_state.is_idle() {
                        panic!("got flush action when write state is not idle");
                    }

                    self.write_state.transition_to_flush();
                    Some(a)
                }
                a @ Action::ReadRecord => {
                    if !self.read_state.is_idle() {
                        panic!("got read action when read state is not idle");
                    }

                    self.read_state.transition_to_read();
                    Some(a)
                }
                a @ Action::AcknowledgeRead => {
                    if self.unacked_reads > 0 {
                        self.unacked_reads -= 1;
                        self.acker.ack(1);
                        Some(a)
                    } else {
                        None
                    }
                }
            }
        } else {
            None
        }
    }

    /// Gets the result of pending write action, if one is in-flight.
    ///
    /// If a write action (either a record write or a flush) is in-flight, we attempt to poll it to
    /// see if it is still pending or can successfully complete.  If it completes, information about
    /// the action, and operation, is passed back so that it can also be run through the model to
    /// check for consistency.
    fn get_pending_write_action(&mut self) -> Option<(Action, Poll<WriteActionResult>)> {
        let write_state = mem::replace(&mut self.write_state, WriteState::Inconsistent);
        let (new_write_state, result) = match write_state {
            // No in-flight write operation.
            s @ WriteState::Idle(_) | s @ WriteState::Closed => (s, None),
            // We have an in-flight `write_record` call.
            WriteState::PendingWrite(record, mut fut) => match fut.poll() {
                // No change yet.
                Poll::Pending => (
                    WriteState::PendingWrite(record.clone(), fut),
                    Some((Action::WriteRecord(record), Poll::Pending)),
                ),
                // The `write_record` call completed.
                Poll::Ready((writer, result)) => (
                    WriteState::Idle(writer),
                    Some((
                        Action::WriteRecord(record),
                        Poll::Ready(WriteActionResult::Write(result)),
                    )),
                ),
            },
            // We have an in-flight `flush` call.
            WriteState::PendingFlush(mut fut) => match fut.poll() {
                // No change yet.
                Poll::Pending => (
                    WriteState::PendingFlush(fut),
                    Some((Action::FlushWrites, Poll::Pending)),
                ),
                // The `flush` call completed.
                Poll::Ready((writer, result)) => (
                    WriteState::Idle(writer),
                    Some((
                        Action::FlushWrites,
                        Poll::Ready(WriteActionResult::Flush(result)),
                    )),
                ),
            },
            WriteState::Inconsistent => panic!("should never start from inconsistent write state"),
        };

        self.write_state = new_write_state;
        result
    }

    /// Gets the result of pending read action, if one is in-flight.
    ///
    /// If a read action is in-flight, we attempt to poll it to see if it is still pending or can
    /// successfully complete.  If it completes, information about the action, and operation, is
    /// passed back so that it can also be run through the model to check for consistency.
    fn get_pending_read_action(&mut self) -> Option<(Action, Poll<ReadActionResult>)> {
        let read_state = mem::replace(&mut self.read_state, ReadState::Inconsistent);
        let (new_read_state, result) = match read_state {
            // No in-flight read operation.
            s @ ReadState::Idle(_) => (s, None),
            // We have an in-flight `read` call.
            ReadState::PendingRead(mut fut) => match fut.poll() {
                // No change yet.
                Poll::Pending => (
                    ReadState::PendingRead(fut),
                    Some((Action::ReadRecord, Poll::Pending)),
                ),
                // The `read` call completed.
                Poll::Ready((reader, result)) => {
                    // If a record was actually read back, track it as an unacknowledged read.
                    if let Ok(record) = &result {
                        if record.is_some() {
                            self.unacked_reads += 1;
                        }
                    }

                    (
                        ReadState::Idle(reader),
                        Some((
                            Action::ReadRecord,
                            Poll::Ready(ReadActionResult::Read(result)),
                        )),
                    )
                }
            },
            ReadState::Inconsistent => panic!("should never start from inconsistent read state"),
        };

        self.read_state = new_read_state;
        result
    }
}

#[test]
fn model_check() {
    let _ = install_tracing_helpers();

    fn inner(config: DiskBufferConfigBuilder<TestFilesystem>, actions: Vec<Action>) -> TestResult {
        let config = match config.build() {
            Ok(config) => {
                // Limit our buffer config to the following:
                // - max buffer size of 64MB
                // - max data file size of 2MB
                // - max record size of 1MB
                //
                // Otherwise, the model just runs uselessly slow.
                if config.max_buffer_size > 64_000_000
                    || config.max_data_file_size > 2_000_000
                    || config.max_record_size > 1_000_000
                {
                    return TestResult::discard();
                }

                config
            }
            // While we got an error from building the configuration, there's no real failure from
            // a test perspective: we simply gave it invalid parameters so we need to try again.
            Err(_) => return TestResult::discard(),
        };

        // Can't run without any actions.
        if actions.is_empty() {
            return TestResult::discard();
        }

        // Ensure our list of actions doesn't have too many acknowledgements.  Essentially, in
        // practice, acknowledging a read should only be triggered once a record is actually read,
        // so we should never have more acknowledgement actions than reads, essentially.
        let actions = prune_unmatched_ack_actions(actions);

        info!(message = "starting new model check run",
            actions = actions.len(), max_buffer_size = config.max_buffer_size,
            max_data_file_size = config.max_data_file_size,
            max_record_size = config.max_record_size, flush_interval = ?config.flush_interval);

        let rt = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("should not fail to build runtime");

        rt.block_on(async move {
            // Create our model, which represents the expected behavior of the buffer, and our
            // "system under test", which is the actual buffer implementation.
            let mut model = Model::from_config(&config);

            let usage_handle = BufferUsageHandle::noop();
            let (writer, reader, acker) =
                Buffer::<Record>::from_config(config, usage_handle)
                    .await
                    .expect("should not fail to build buffer");

            // Additionally, since some operations may have to wait for progress for the reader,
            // writer, or acker, we need to split up those operations and track the in-flight action
            // for each group.
            //
            // What this means is that if we get a write action, and we run it against the model and
            // the SUT, and it has to wait for the rmeader to make progress... well, we need to
            // capture that in-flight future for the write operation against the SUT and attempt to
            // drive it every time we make progress on the reader side, and vise versa.
            //
            // This is a potentially a bit of a perversion of the nature of getting our actions in a
            // specific order and then running them slightly _out_ of order, but it's the best way I
            // could think of to control the scheduling and correctly model the nature of the reader
            // and writer constantly waking each other up as progress is made.
            //
            // Critically, however, we _drain_ the actions such that, if we were in fact blocked on
            // a write operation, we would grab the next non-write/non-flush operation, and continue
            // to do that until we ran out of non-write/non-flush operations.
            //
            // Since the generator may actually generate an unbalanced read/write workload, there
            // may actually be actions that can never complete once we're out of other actions to
            // run, and that's OK!  So long as the model agrees the operation should still
            // theoretically be waiting for progress from the other side, etc, then the behavior is
            // valid, and that test would be a valid run.
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
                                debug!("pending read result: model={:?}, SUT={:?}", model_result, sut_result);

                                match sut_result {
                                    // The SUT made no progress, so the model should not have made
                                    // progress either.
                                    Poll::Pending => quickcheck_assert_eq!(model_result, Progress::Blocked, "expected blocked write"),
                                    Poll::Ready(sut_result) => {
                                        made_progress = true;
                                        match sut_result {
                                            WriteActionResult::Write(result) => match result {
                                                Ok(written) => {
                                                    quickcheck_assert_eq!(model_result, Progress::WroteRecord(written), "expected completed write");
                                                    debug!("completed writing record: {:?}", record);
                                                },
                                                // TODO: Should we go deeper and try to directly compare the
                                                // internal error variant?
                                                Err(e) => {
                                                    quickcheck_assert_eq!(model_result, Progress::WriteError, "expected write error");
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
                                model.flush_writes();
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
                                    Poll::Pending => quickcheck_assert_eq!(model_result, Progress::Blocked, "expected blocked read"),
                                    Poll::Ready(sut_result) => {
                                        made_progress = true;
                                        match sut_result {
                                            ReadActionResult::Read(result) => match result {
                                                Ok(maybe_record) => {
                                                    quickcheck_assert_eq!(model_result, Progress::ReadRecord(maybe_record.clone()), "expected record read");
                                                    debug!("record read result: {:?}", maybe_record);
                                                },
                                                // TODO: Should we go deeper and try to directly compare the
                                                // internal error variant?
                                                Err(e) => {
                                                    quickcheck_assert_eq!(model_result, Progress::ReadError, "expected read error");
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

            TestResult::passed()
        })
    }

    let inner_fn: fn(DiskBufferConfigBuilder<TestFilesystem>, Vec<Action>) -> TestResult = inner;
    QuickCheck::new()
        .tests(10_000)
        .max_tests(100_000)
        .quickcheck(inner_fn);
}

fn prune_unmatched_ack_actions(actions: Vec<Action>) -> Vec<Action> {
    let mut outstanding_reads = 0;
    actions
        .into_iter()
        .filter(|a| {
            // If there's a read, we add it to our outstanding read total.
            if a.is_read() {
                outstanding_reads += 1;
                true
            } else if a.is_ack() {
                // If we see an acknowledgement action, there has to be an outstanding read that it
                // can pair with, otherwise things would be unbalanced.
                if outstanding_reads == 0 {
                    false
                } else {
                    outstanding_reads -= 1;
                    true
                }
            } else {
                // Everything else can pass through untouched.
                true
            }
        })
        .collect::<Vec<_>>()
}
