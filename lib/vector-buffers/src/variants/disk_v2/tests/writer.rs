use std::{
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_test::{assert_pending, assert_ready, task::spawn};

use crate::{
    test::SizedRecord,
    variants::disk_v2::{
        io::{AsyncFile, Metadata},
        writer::{FlushResult, RecordWriter},
    },
};

const MAX_RECORD_SIZE: usize = 1024 * 1024;
const MAX_DATA_FILE_SIZE: u64 = MAX_RECORD_SIZE as u64;

#[derive(Debug, Default)]
struct FlushState {
    pending: Vec<u8>,
    visible: Vec<u8>,
    flush_allowed: bool,
    flush_waker: Option<Waker>,
}

#[derive(Clone, Debug, Default)]
struct FlushControl {
    state: Arc<Mutex<FlushState>>,
}

impl FlushControl {
    fn allow_flush(&self) {
        let mut state = self
            .state
            .lock()
            .expect("flush state should not be poisoned");
        state.flush_allowed = true;
        if let Some(waker) = state.flush_waker.take() {
            waker.wake();
        }
    }

    fn pending_len(&self) -> usize {
        self.state
            .lock()
            .expect("flush state should not be poisoned")
            .pending
            .len()
    }

    fn visible_len(&self) -> usize {
        self.state
            .lock()
            .expect("flush state should not be poisoned")
            .visible
            .len()
    }
}

#[derive(Clone, Debug, Default)]
struct FlushGatedFile {
    control: FlushControl,
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
        let mut state = self
            .control
            .state
            .lock()
            .expect("flush state should not be poisoned");
        state.pending.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut state = self
            .control
            .state
            .lock()
            .expect("flush state should not be poisoned");
        if !state.flush_allowed {
            state.flush_waker = Some(cx.waker().clone());
            return Poll::Pending;
        }

        let pending = std::mem::take(&mut state.pending);
        state.visible.extend_from_slice(&pending);
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

impl AsyncFile for FlushGatedFile {
    async fn metadata(&self) -> io::Result<Metadata> {
        let state = self
            .control
            .state
            .lock()
            .expect("flush state should not be poisoned");
        Ok(Metadata {
            len: (state.pending.len() + state.visible.len()) as u64,
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
) -> (RecordWriter<FlushGatedFile, SizedRecord>, FlushControl) {
    let inner = FlushGatedFile::default();
    let control = inner.control.clone();
    (
        RecordWriter::new(
            inner,
            0,
            write_buffer_size,
            MAX_DATA_FILE_SIZE,
            MAX_RECORD_SIZE,
        ),
        control,
    )
}

#[tokio::test]
async fn direct_write_waits_for_visibility_before_reporting_progress() {
    let (mut writer, control) = record_writer(1);

    let mut write = spawn(writer.write_record(1, SizedRecord::new(64)));
    assert_pending!(write.poll());
    assert!(control.pending_len() > 0);
    assert_eq!(control.visible_len(), 0);

    control.allow_flush();

    assert!(write.is_woken());
    let (record_bytes, result) = assert_ready!(write.poll()).expect("write should succeed");
    assert_eq!(
        result,
        Some(FlushResult {
            events_flushed: 1,
            bytes_flushed: record_bytes as u64,
        })
    );
    assert_eq!(control.visible_len(), record_bytes);
}

#[tokio::test]
async fn buffered_write_waits_for_visibility_before_reporting_flush_progress() {
    let (mut writer, control) = record_writer(MAX_RECORD_SIZE);
    let (record_bytes, result) = writer
        .write_record(1, SizedRecord::new(64))
        .await
        .expect("write should succeed");
    assert_eq!(result, None);

    let mut flush = spawn(writer.flush());
    assert_pending!(flush.poll());
    assert!(control.pending_len() > 0);
    assert_eq!(control.visible_len(), 0);

    control.allow_flush();

    assert!(flush.is_woken());
    assert_eq!(
        assert_ready!(flush.poll()).expect("flush should succeed"),
        Some(FlushResult {
            events_flushed: 1,
            bytes_flushed: record_bytes as u64,
        })
    );
    assert_eq!(control.visible_len(), record_bytes);
}
