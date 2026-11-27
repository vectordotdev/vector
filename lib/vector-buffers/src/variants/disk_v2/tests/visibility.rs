use std::{
    io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::{
    buffer_usage_data::BufferUsageHandle,
    test::{SizedRecord, with_temp_dir},
    variants::disk_v2::{
        Buffer, DiskBufferConfigBuilder,
        io::{AsyncFile, Filesystem, Metadata, ProductionFilesystem},
    },
};

#[derive(Clone, Debug, Default)]
struct VisibilityGate {
    hide_reads: Arc<AtomicBool>,
    eof_reads: Arc<AtomicUsize>,
    read_calls: Arc<AtomicUsize>,
}

#[derive(Debug)]
struct VisibilityFile {
    inner: tokio::fs::File,
    gate: VisibilityGate,
}

impl AsyncRead for VisibilityFile {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.gate.read_calls.fetch_add(1, Ordering::Relaxed);
        if self.gate.hide_reads.load(Ordering::Acquire) {
            self.gate.eof_reads.fetch_add(1, Ordering::Relaxed);
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for VisibilityFile {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl AsyncFile for VisibilityFile {
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

impl Filesystem for VisibilityGate {
    type File = VisibilityFile;
    type MemoryMap = <ProductionFilesystem as Filesystem>::MemoryMap;
    type MutableMemoryMap = <ProductionFilesystem as Filesystem>::MutableMemoryMap;

    async fn open_file_writable(&self, path: &Path) -> io::Result<VisibilityFile> {
        Ok(VisibilityFile {
            inner: ProductionFilesystem.open_file_writable(path).await?,
            gate: self.clone(),
        })
    }

    async fn open_file_writable_atomic(&self, path: &Path) -> io::Result<VisibilityFile> {
        Ok(VisibilityFile {
            inner: ProductionFilesystem.open_file_writable_atomic(path).await?,
            gate: self.clone(),
        })
    }

    async fn open_file_readable(&self, path: &Path) -> io::Result<VisibilityFile> {
        Ok(VisibilityFile {
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
async fn published_progress_recovers_when_last_write_becomes_visible_without_notification() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();
        async move {
            let gate = VisibilityGate::default();
            let config = DiskBufferConfigBuilder::from_path(data_dir)
                .filesystem(gate.clone())
                .build()
                .unwrap();
            let (mut writer, mut reader, _) =
                Buffer::from_config_inner(config, BufferUsageHandle::noop())
                    .await
                    .unwrap();
            writer.write_record(SizedRecord::new(128)).await.unwrap();
            gate.hide_reads.store(true, Ordering::Release);
            writer.flush().await.unwrap();

            // Simulate a queued filesystem write: metadata and its notification have arrived,
            // but a separate read handle still sees EOF. Releasing visibility emits no wake-up.
            let release = gate.clone();
            let release_task = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                release.hide_reads.store(false, Ordering::Release);
            });
            let record = tokio::time::timeout(Duration::from_secs(1), reader.next())
                .await
                .unwrap()
                .unwrap();
            release_task.await.unwrap();
            assert_eq!(record, Some(SizedRecord::new(128)));
            // Retries back off instead of spinning while visibility is delayed.
            assert!(gate.eof_reads.load(Ordering::Relaxed) < 20);
        }
    })
    .await;
}

#[tokio::test]
async fn caught_up_reader_does_not_poll_while_waiting_for_new_progress() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();
        async move {
            let gate = VisibilityGate::default();
            let config = DiskBufferConfigBuilder::from_path(data_dir)
                .filesystem(gate.clone())
                .build()
                .unwrap();
            let (mut writer, mut reader, _) =
                Buffer::from_config_inner(config, BufferUsageHandle::noop())
                    .await
                    .unwrap();
            // Open the writer file but leave its record buffered and unpublished.
            writer.write_record(SizedRecord::new(128)).await.unwrap();
            gate.hide_reads.store(true, Ordering::Release);
            let mut waiting = Box::pin(reader.next());
            tokio::time::timeout(
                Duration::from_secs(1),
                std::future::poll_fn(|cx| {
                    assert!(waiting.as_mut().poll(cx).is_pending());
                    if gate.eof_reads.load(Ordering::Relaxed) != 0 {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                }),
            )
            .await
            .unwrap();
            let initial = gate.eof_reads.load(Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(20)).await;
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(waiting.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
            assert_eq!(gate.eof_reads.load(Ordering::Relaxed), initial);
        }
    })
    .await;
}

#[tokio::test]
async fn dropped_tail_does_not_retry_eof_for_earlier_unacknowledged_data() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();
        async move {
            let gate = VisibilityGate::default();
            let config = DiskBufferConfigBuilder::from_path(data_dir)
                .filesystem(gate.clone())
                .build()
                .unwrap();
            let (mut writer, mut reader, ledger) =
                Buffer::from_config_inner(config, BufferUsageHandle::noop())
                    .await
                    .unwrap();
            let first_bytes = writer.write_record(SizedRecord::new(128)).await.unwrap();
            let bad_bytes = writer.write_record(SizedRecord::new(129)).await.unwrap();
            writer.flush().await.unwrap();
            let path = ledger.get_current_writer_data_file_path();
            tokio::time::timeout(Duration::from_secs(1), async {
                while tokio::fs::metadata(&path).await.unwrap().len()
                    < (first_bytes + bad_bytes) as u64
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            super::known_errors::corrupt_record_checksum(
                &path,
                first_bytes,
                first_bytes + bad_bytes,
            )
            .await;

            // Keep the first delivery unacknowledged while the corrupt final frame is consumed.
            let first = reader.next().await.unwrap().unwrap();
            assert_eq!(first, SizedRecord::new(128));
            assert!(reader.next().await.unwrap_err().is_bad_read());
            assert_eq!(ledger.get_total_buffer_size(), first_bytes as u64);
            let mut waiting = Box::pin(reader.next());
            assert!(
                tokio::time::timeout(Duration::from_millis(20), &mut waiting)
                    .await
                    .is_err()
            );
            let initial = gate.read_calls.load(Ordering::Relaxed);
            assert!(
                tokio::time::timeout(Duration::from_millis(250), &mut waiting)
                    .await
                    .is_err()
            );
            assert_eq!(
                gate.read_calls.load(Ordering::Relaxed),
                initial,
                "consumed corrupt bytes must not keep EOF retries alive"
            );
            drop(waiting);

            // New published bytes must still retry visibility after the corrupt frame.
            gate.hide_reads.store(true, Ordering::Release);
            writer.write_record(SizedRecord::new(130)).await.unwrap();
            writer.flush().await.unwrap();
            let release = gate.clone();
            let release_task = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                release.hide_reads.store(false, Ordering::Release);
            });
            let next = tokio::time::timeout(Duration::from_secs(1), reader.next())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(next, Some(SizedRecord::new(130)));
            release_task.await.unwrap();
            drop(first);
        }
    })
    .await;
}
