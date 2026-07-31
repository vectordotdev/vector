use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::{
    test::SizedRecord,
    variants::disk_v2::{
        io::{AsyncFile, Metadata},
        writer::{FlushResult, RecordWriter},
    },
};

const MAX_RECORD_SIZE: usize = 1024 * 1024;
const MAX_DATA_FILE_SIZE: u64 = MAX_RECORD_SIZE as u64;

/// Models an async file whose accepted writes are not observable until it is flushed.
#[derive(Debug, Default)]
struct FlushGatedFile {
    pending: Vec<u8>,
    visible: Vec<u8>,
    flush_count: usize,
}

impl AsyncRead for FlushGatedFile {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for FlushGatedFile {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.pending.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.visible.append(&mut this.pending);
        this.flush_count += 1;
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

impl AsyncFile for FlushGatedFile {
    async fn metadata(&self) -> io::Result<Metadata> {
        Ok(Metadata {
            len: (self.pending.len() + self.visible.len()) as u64,
        })
    }

    async fn truncate(&self, _size: u64) -> io::Result<()> {
        Ok(())
    }

    async fn sync_all(&self) -> io::Result<()> {
        Ok(())
    }
}

fn record_writer(
    write_buffer_size: usize,
    inner: FlushGatedFile,
) -> RecordWriter<FlushGatedFile, SizedRecord> {
    RecordWriter::new(
        inner,
        0,
        write_buffer_size,
        MAX_DATA_FILE_SIZE,
        MAX_RECORD_SIZE,
    )
}

#[tokio::test]
async fn write_larger_than_internal_buffer_flushes_before_reporting_progress() {
    let mut writer = record_writer(1, FlushGatedFile::default());

    let (record_bytes, result) = writer
        .write_record(1, SizedRecord::new(64))
        .await
        .expect("write should succeed");

    assert_eq!(
        result,
        Some(FlushResult {
            events_flushed: 1,
            bytes_flushed: record_bytes as u64,
        })
    );
    assert_eq!(writer.get_ref().visible.len(), record_bytes);
    assert!(writer.get_ref().pending.is_empty());
    assert!(writer.get_ref().flush_count > 0);
}

#[tokio::test]
async fn buffered_write_flushes_before_reporting_progress() {
    let mut writer = record_writer(MAX_RECORD_SIZE, FlushGatedFile::default());

    let (record_bytes, write_result) = writer
        .write_record(1, SizedRecord::new(64))
        .await
        .expect("write should succeed");

    assert_eq!(write_result, None);
    assert!(writer.get_ref().visible.is_empty());
    assert!(writer.get_ref().pending.is_empty());
    assert_eq!(writer.get_ref().flush_count, 0);

    let flush_result = writer.flush().await.expect("flush should succeed");

    assert_eq!(
        flush_result,
        Some(FlushResult {
            events_flushed: 1,
            bytes_flushed: record_bytes as u64,
        })
    );
    assert_eq!(writer.get_ref().visible.len(), record_bytes);
    assert!(writer.get_ref().pending.is_empty());
    assert_eq!(writer.get_ref().flush_count, 1);
}

#[tokio::test]
async fn empty_buffer_flushes_inner_writer() {
    let payload = b"pending inner write";
    let inner = FlushGatedFile {
        pending: payload.to_vec(),
        ..FlushGatedFile::default()
    };
    let mut writer = record_writer(8, inner);

    let result = writer.flush().await.expect("flush should succeed");

    assert_eq!(result, None);
    assert_eq!(writer.get_ref().visible, payload);
    assert!(writer.get_ref().pending.is_empty());
    assert_eq!(writer.get_ref().flush_count, 1);
}
