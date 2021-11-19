use std::{cmp, io};

use crc32fast::Hasher;
use rkyv::{archived_root, AlignedVec};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use super::record::{try_as_record_archive, ArchivedRecord, Record, RecordStatus};

pub struct ReadToken(u64);

impl ReadToken {
    pub fn record_id(&self) -> u64 {
        self.0
    }
}

pub enum RecordEntry {
    Valid(ReadToken),
    Corrupted,
    FailedDeserialization(String),
}

pub struct RecordReader<R> {
    reader: BufReader<R>,
    aligned_buf: AlignedVec,
    checksummer: Hasher,
    current_record_id: u64,
}

impl<R> RecordReader<R>
where
    R: AsyncRead + Unpin,
{
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
            aligned_buf: AlignedVec::new(),
            checksummer: Hasher::new(),
            current_record_id: 0,
        }
    }

    async fn read_length_delimiter(&mut self) -> io::Result<Option<usize>> {
        loop {
            if self.reader.buffer().len() >= 4 {
                let length_buf = &self.reader.buffer()[..4];
                let length = length_buf
                    .try_into()
                    .expect("the slice is the length of a u32");
                self.reader.consume(4);

                return Ok(Some(u32::from_be_bytes(length) as usize));
            }

            let buf = self.reader.fill_buf().await?;
            if buf.is_empty() {
                return Ok(None);
            }
        }
    }

    pub async fn try_next_record(&mut self) -> io::Result<Option<RecordEntry>> {
        let record_len = match self.read_length_delimiter().await? {
            Some(len) => len,
            None => return Ok(None),
        };

        // Read in all of the bytes we need first.
        self.aligned_buf.clear();
        while self.aligned_buf.len() < record_len {
            let needed = record_len - self.aligned_buf.len();
            let buf = self.reader.fill_buf().await?;

            let available = cmp::min(buf.len(), needed);
            self.aligned_buf.extend_from_slice(&buf[..available]);
            self.reader.consume(available);
        }

        // Now see if we can deserialize our archived record from this.
        let buf = self.aligned_buf.as_slice();
        match try_as_record_archive(buf, &self.checksummer) {
            // TODO: do something in the error / corrupted cases; emit an error, increment an error
            // counter, yadda yadda. something.
            RecordStatus::FailedDeserialization(de) => {
                Ok(Some(RecordEntry::FailedDeserialization(de.into_inner())))
            }
            RecordStatus::Corrupted => Ok(Some(RecordEntry::Corrupted)),
            RecordStatus::Valid(id) => {
                self.current_record_id = id;
                Ok(Some(RecordEntry::Valid(ReadToken(id))))
            }
        }
    }

    pub async fn read_record(&mut self, token: ReadToken) -> io::Result<&ArchivedRecord<'_>> {
        if token.0 != self.current_record_id {
            panic!("using expired read token");
        }

        // SAFETY:
        // - `try_next_record` is the only method that can hand back a `ReadToken`
        // - we only get a `ReadToken` if there's a valid record in `self.aligned_buf`
        // - `try_next_record` does all the archive checks, checksum validation, etc
        unsafe { Ok(archived_root::<Record<'_>>(&self.aligned_buf)) }
    }
}
