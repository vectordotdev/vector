use std::{
    io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::Mutex,
};

use crate::{
    buffer_usage_data::BufferUsageHandle,
    test::{SizedRecord, with_temp_dir},
    variants::disk_v2::{
        Buffer, DiskBufferConfigBuilder,
        io::{AsyncFile, Filesystem, Metadata, ProductionFilesystem},
    },
};

#[derive(Clone, Debug, Default)]
struct FlushGate {
    blocked: Arc<AtomicBool>,
    entered: Arc<AtomicBool>,
}

#[derive(Debug)]
struct GatedFile {
    inner: tokio::fs::File,
    gate: FlushGate,
}

impl AsyncRead for GatedFile {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for GatedFile {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.gate.blocked.load(Ordering::Acquire) {
            self.gate.entered.store(true, Ordering::Release);
            return Poll::Pending;
        }
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl AsyncFile for GatedFile {
    async fn metadata(&self) -> io::Result<Metadata> {
        AsyncFile::metadata(&self.inner).await
    }

    async fn truncate(&self, size: u64) -> io::Result<()> {
        AsyncFile::truncate(&self.inner, size).await
    }

    async fn sync_all(&self) -> io::Result<()> {
        AsyncFile::sync_all(&self.inner).await
    }
}

impl Filesystem for FlushGate {
    type File = GatedFile;
    type MemoryMap = <ProductionFilesystem as Filesystem>::MemoryMap;
    type MutableMemoryMap = <ProductionFilesystem as Filesystem>::MutableMemoryMap;

    async fn open_file_writable(&self, path: &Path) -> io::Result<GatedFile> {
        Ok(GatedFile {
            inner: ProductionFilesystem.open_file_writable(path).await?,
            gate: self.clone(),
        })
    }

    async fn open_file_writable_atomic(&self, path: &Path) -> io::Result<GatedFile> {
        Ok(GatedFile {
            inner: ProductionFilesystem.open_file_writable_atomic(path).await?,
            gate: self.clone(),
        })
    }

    async fn open_file_readable(&self, path: &Path) -> io::Result<GatedFile> {
        Ok(GatedFile {
            inner: ProductionFilesystem.open_file_readable(path).await?,
            gate: self.clone(),
        })
    }

    async fn open_mmap_readable(&self, path: &Path) -> io::Result<Self::MemoryMap> {
        ProductionFilesystem.open_mmap_readable(path).await
    }

    async fn open_mmap_writable(&self, path: &Path) -> io::Result<Self::MutableMemoryMap> {
        ProductionFilesystem.open_mmap_writable(path).await
    }

    async fn truncate_file(&self, path: &Path, size: u64) -> io::Result<()> {
        ProductionFilesystem.truncate_file(path, size).await
    }

    async fn delete_file(&self, path: &Path) -> io::Result<()> {
        ProductionFilesystem.delete_file(path).await
    }

    async fn list_files(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        ProductionFilesystem.list_files(path).await
    }
}

#[tokio::test]
async fn shared_writer_survives_cancelled_visibility_flush_without_reusing_record_ids() {
    for capacity in [64, 4096] {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();
            async move {
                let gate = FlushGate::default();
                let config = DiskBufferConfigBuilder::from_path(data_dir)
                    .write_buffer_size(capacity)
                    .filesystem(gate.clone())
                    .build()
                    .unwrap();
                let (writer, mut reader, ledger) =
                    Buffer::from_config_inner(config, BufferUsageHandle::noop())
                        .await
                        .unwrap();
                // Match the production adapter: cancelling a send drops its lock guard, but another
                // sender still owns the same writer and must be able to continue safely.
                let shared = Arc::new(Mutex::new(writer));
                let other_sender = Arc::clone(&shared);
                gate.blocked.store(true, Ordering::Release);
                let mut cancelled = Box::pin(async {
                    let mut writer = shared.lock().await;
                    writer.write_record(SizedRecord::new(128)).await.unwrap();
                    writer.flush().await.unwrap();
                });
                tokio::time::timeout(
                    Duration::from_secs(10),
                    std::future::poll_fn(|cx| {
                        assert!(cancelled.as_mut().poll(cx).is_pending());
                        if gate.entered.load(Ordering::Acquire) {
                            Poll::Ready(())
                        } else {
                            Poll::Pending
                        }
                    }),
                )
                .await
                .unwrap();
                drop(cancelled);
                assert_eq!(other_sender.lock().await.usage_snapshot().event_count, 1);

                gate.blocked.store(false, Ordering::Release);
                let mut writer = other_sender.lock().await;
                writer.write_record(SizedRecord::new(256)).await.unwrap();
                writer.flush().await.unwrap();
                assert_eq!(ledger.state().get_next_writer_record_id(), 3);
                assert_eq!(reader.next().await.unwrap(), Some(SizedRecord::new(128)));
                assert_eq!(reader.next().await.unwrap(), Some(SizedRecord::new(256)));
            }
        })
        .await;
    }
}
