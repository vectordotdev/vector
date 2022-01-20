use std::{
    collections::VecDeque,
    error, fmt,
    io::{self, Cursor},
    mem,
    num::{NonZeroU32, NonZeroU64, NonZeroUsize},
    path::PathBuf,
    time::Duration,
};

use async_trait::async_trait;
use bytes::{Buf, BufMut};
use core_common::byte_size_of::ByteSizeOf;
use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
use tokio::runtime::Builder;

use crate::{
    disk_v2::{writer::RecordWriter, DiskBufferConfig, DiskBufferConfigBuilder, Filesystem},
    encoding::{DecodeBytes, EncodeBytes},
};

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

impl EncodeBytes<Record> for Record {
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

impl DecodeBytes<Record> for Record {
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
}

impl Arbitrary for Action {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        match u8::arbitrary(g) % 3 {
            0 => {
                let record = Record::new(u64::arbitrary(g), u32::arbitrary(g));
                Action::WriteRecord(record)
            }
            1 => Action::FlushWrites,
            _ => Action::ReadRecord,
        }
    }
}

enum Progress {
    Advanced,
    Blocked,
}

struct TestFile;

#[derive(Clone, Debug)]
struct TestFilesystem;

#[async_trait]
impl Filesystem for TestFilesystem {
    type File = Cursor<Vec<u8>>;
    type MemoryMap = Vec<u8>;
    type MutableMemoryMap = Vec<u8>;

    async fn open_file_writable(&self, path: &PathBuf) -> io::Result<Self::File> {
        Ok(Cursor::new(Vec::new()))
    }

    async fn open_file_writable_atomic(&self, path: &PathBuf) -> io::Result<Self::File> {
        Ok(Cursor::new(Vec::new()))
    }

    async fn open_file_readable(&self, path: &PathBuf) -> io::Result<Self::File> {
        Ok(Cursor::new(Vec::new()))
    }

    async fn open_mmap_readable(&self, path: &PathBuf) -> io::Result<Self::MemoryMap> {
        Ok(Vec::new())
    }

    async fn open_mmap_writable(&self, path: &PathBuf) -> io::Result<Self::MutableMemoryMap> {
        Ok(Vec::new())
    }

    async fn delete_file(&self, path: &PathBuf) -> io::Result<()> {
        Ok(())
    }
}

impl Arbitrary for DiskBufferConfig<TestFilesystem> {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        DiskBufferConfigBuilder::from_path(PathBuf::arbitrary(g))
            .max_data_file_size(NonZeroU64::arbitrary(g).get())
            .max_buffer_size(NonZeroU64::arbitrary(g).get())
            .max_record_size(NonZeroUsize::arbitrary(g).get())
            .flush_interval(Duration::arbitrary(g))
            .filesystem(TestFilesystem)
            .build()
    }
}

struct Model {
    max_data_file_size: u64,
    max_buffer_size: u64,
    max_record_size: usize,
    current_data_file_size: u64,
    current_buffer_size: u64,
    unflushed_records: Vec<Record>,
    flushed_records: VecDeque<Record>,
    record_writer: RecordWriter<Cursor<Vec<u8>>, Record>,
}

impl Model {
    fn from_config<FS>(config: &DiskBufferConfig<FS>) -> Self {
        Self {
            max_data_file_size: config.max_data_file_size,
            max_buffer_size: config.max_buffer_size,
            max_record_size: config.max_record_size,
            current_data_file_size: 0,
            current_buffer_size: 0,
            unflushed_records: Vec::new(),
            flushed_records: VecDeque::new(),
            record_writer: RecordWriter::new(Cursor::new(Vec::new()), config.max_record_size),
        }
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
        // Write the record in the same way that the buffer would, which is the only way we can
        // calculate the true size that record occupies.
        let archived_len = self.get_archived_record_len(record.clone());

        // Now store the record and adjust our buffer/data file size.
        //
        // TODO: Double check if we update size stats when we write or when we flush.
        // TODO: We need to also account for the automatic flushing that the buffered writer will do
        // when we exceed the internal write buffer, or if a write would exceed it.
        self.unflushed_records.push(record);
        self.current_buffer_size += archived_len;
        self.current_data_file_size += archived_len;

        Progress::Advanced
    }

    fn flush_writes(&mut self) -> Progress {
        self.flushed_records
            .extend(self.unflushed_records.drain(..));
        Progress::Advanced
    }

    fn read_record(&mut self) -> Option<Record> {
        match self.flushed_records.pop_front() {
            None => None,
            Some(record) => {
                let archive_len = self.get_archived_record_len(record.clone());
                self.current_buffer_size -= archive_len;

                Some(record)
            }
        }
    }
}

#[test]
fn model_check() {
    fn inner(config: DiskBufferConfig<TestFilesystem>, actions: Vec<Action>) -> TestResult {
        let rt = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("should not fail to build runtime");

        let mut model = Model::from_config(&config);

        rt.block_on(async move {
            for action in actions {
                match action {
                    Action::WriteRecord(record) => {
                        let model_result = model.write_record(record);
                    }
                    Action::FlushWrites => {
                        let model_result = model.flush_writes();
                    }
                    Action::ReadRecord => {
                        let record = model.read_record();
                    }
                }
            }

            TestResult::passed()
        })
    }

    let inner_fn: fn(DiskBufferConfig<TestFilesystem>, Vec<Action>) -> TestResult = inner;
    QuickCheck::new()
        .tests(10_000)
        .max_tests(100_000)
        .quickcheck(inner_fn);
}
