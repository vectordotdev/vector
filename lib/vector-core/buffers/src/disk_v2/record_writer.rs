use std::io;

use crc32fast::Hasher;
use rkyv::{
    ser::{
        serializers::{
            AlignedSerializer, AllocScratch, BufferScratch, CompositeSerializer, FallbackScratch,
        },
        Serializer,
    },
    AlignedVec, Infallible,
};
use tokio::{
    fs::File,
    io::{AsyncWrite, AsyncWriteExt, BufWriter},
};

use super::record::Record;

pub struct RecordWriter<W> {
    writer: BufWriter<W>,
    buf: AlignedVec,
    scratch: AlignedVec,
    checksummer: Hasher,
}

impl<W> RecordWriter<W>
where
    W: AsyncWrite + Unpin,
{
    /// Creates a new `RecordWriter` around the provided writer.
    ///
    /// Internally, the writer is wrapped in a `BufWriter<W>`, so callers should not pass in an
    /// already buffered writer.
    pub fn new(writer: W) -> Self {
        Self {
            writer: BufWriter::new(writer),
            buf: AlignedVec::with_capacity(2048),
            scratch: AlignedVec::with_capacity(2048),
            checksummer: Hasher::new(),
        }
    }

    /// Writes a record.
    ///
    /// Returns the number of bytes written to serialize the record, including the framing. Writes
    /// are not automatically flushed, so `flush` must be called after any record write if there is
    /// a requirement for the record to immediately be written all the way to the underlying writer.
    ///
    /// # Errors
    ///
    /// If there is an I/O error while writing the record, an error variant will be returned
    /// describing the error.  Additionally, if there is an error while serializing the record, an
    /// error variant will be returned describing the serialization error.
    pub async fn write_record(&mut self, id: u64, payload: &[u8]) -> io::Result<usize> {
        let record = Record::with_checksum(id, payload, &self.checksummer);

        // Serialize the record., and grab the length
        self.buf.clear();
        let archive_len = {
            let mut serializer = CompositeSerializer::new(
                AlignedSerializer::new(&mut self.buf),
                FallbackScratch::new(BufferScratch::new(&mut self.scratch), AllocScratch::new()),
                Infallible,
            );

            if let Err(e) = serializer.serialize_value(&record) {
                // TODO: what do we do here? :thinkies:
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("failed to serialize record: {}", e),
                ));
            }
            serializer.pos()
        };
        assert_eq!(self.buf.len(), archive_len as usize);

        let wire_archive_len: u32 = archive_len
            .try_into()
            .expect("archive len should always fit into a u32");
        let _ = self
            .writer
            .write_all(&wire_archive_len.to_be_bytes()[..])
            .await?;
        let _ = self.writer.write_all(&self.buf).await?;

        Ok(4 + archive_len)
    }

    /// Flushes the writer.
    ///
    /// This flushes both the internal buffered writer and the underlying writer object.
    ///
    /// # Errors
    ///
    /// If there is an I/O error while flushing either the buffered writer or the underlying writer,
    /// an error variant will be returned describing the error.
    pub async fn flush(&mut self) -> io::Result<()> {
        let _ = self.writer.flush().await?;
        self.writer.get_mut().flush().await
    }
}

impl RecordWriter<File> {
    /// Synchronizes the underlying file to disk.
    ///
    /// This tries to synchronize both data and metadata.
    ///
    /// # Errors
    ///
    /// If there is an I/O error while syncing the file, an error variant will be returned
    /// describing the error.
    pub async fn sync_all(&mut self) -> io::Result<()> {
        self.writer.get_mut().sync_all().await
    }
}
