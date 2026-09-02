use std::{
    fmt,
    future::Future,
    io::{self, Write as _},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Weak},
    task::{Context, Poll},
};

use tokio::{
    fs::OpenOptions,
    io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf},
};

#[cfg(test)]
use std::sync::Mutex;

const RESUMABLE_WRITE_CHUNK_SIZE: usize = 256 * 1024;

/// Returns whether an I/O error indicates exhausted filesystem capacity or quota.
pub(crate) fn is_filesystem_full(error: &io::Error) -> bool {
    if matches!(
        error.kind(),
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded
    ) {
        return true;
    }

    #[cfg(unix)]
    if matches!(error.raw_os_error(), Some(libc::ENOSPC | libc::EDQUOT)) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::is_filesystem_full;
    use std::io;

    #[test]
    fn classifies_storage_full() {
        assert!(is_filesystem_full(&io::Error::from(
            io::ErrorKind::StorageFull
        )));
        assert!(!is_filesystem_full(&io::Error::from(
            io::ErrorKind::PermissionDenied
        )));
        assert!(is_filesystem_full(&io::Error::from(
            io::ErrorKind::QuotaExceeded
        )));
    }

    #[cfg(unix)]
    #[test]
    fn classifies_unix_capacity_errors() {
        assert!(is_filesystem_full(&io::Error::from_raw_os_error(
            libc::ENOSPC
        )));
        assert!(is_filesystem_full(&io::Error::from_raw_os_error(
            libc::EDQUOT
        )));
    }
}

#[cfg(unix)]
const FILE_MODE_OWNER_RW_GROUP_RO: u32 = 0o640;

/// File metadata.
pub struct Metadata {
    pub(crate) len: u64,
}

impl Metadata {
    /// Gets the length of the file, in bytes.
    pub fn len(&self) -> u64 {
        self.len
    }
}

/// Generalized interface for opening and deleting files from a filesystem.
pub trait Filesystem: Send + Sync {
    type File: AsyncFile;
    type MemoryMap: ReadableMemoryMap;
    type MutableMemoryMap: WritableMemoryMap;

    /// Opens a file for writing, creating it if it does not exist.
    ///
    /// This opens the file in "append" mode, such that the starting position in the file will be
    /// set to the end of the file: the file will not be truncated.  Additionally, the file is
    /// readable.
    ///
    /// # Errors
    ///
    /// If an I/O error occurred when attempting to open the file for writing, an error variant will
    /// be returned describing the underlying error.
    async fn open_file_writable(&self, path: &Path) -> io::Result<Self::File>;

    /// Opens a file for writing, creating it if it does not already exist, but atomically.
    ///
    /// This opens the file in "append" mode, such that the starting position in the file will be
    /// set to the end of the file: the file will not be truncated.  Additionally, the file is
    /// readable.
    ///
    /// # Errors
    ///
    /// If the file already existed, then an error will be returned with an `ErrorKind` of `AlreadyExists`.
    ///
    /// If a general I/O error occurred when attempting to open the file for writing, an error variant will
    /// be returned describing the underlying error.
    async fn open_file_writable_atomic(&self, path: &Path) -> io::Result<Self::File>;

    /// Opens a file for reading, creating it if it does not exist.
    ///
    /// Files will be opened at the logical end position.
    ///
    /// # Errors
    ///
    /// If an I/O error occurred when attempting to open the file for reading, an error variant will
    /// be returned describing the underlying error.
    async fn open_file_readable(&self, path: &Path) -> io::Result<Self::File>;

    /// Opens a file as a readable memory-mapped region.
    ///
    /// # Errors
    ///
    /// If an I/O error occurred when attempting to open the file for reading, or attempting to
    /// memory map the file, an error variant will be returned describing the underlying error.
    async fn open_mmap_readable(&self, path: &Path) -> io::Result<Self::MemoryMap>;

    /// Opens a file as a writable memory-mapped region.
    ///
    /// # Errors
    ///
    /// If an I/O error occurred when attempting to open the file for reading, or attempting to
    /// memory map the file, an error variant will be returned describing the underlying error.
    async fn open_mmap_writable(&self, path: &Path) -> io::Result<Self::MutableMemoryMap>;

    /// Durably truncates a file to `size`, creating it when it does not exist.
    ///
    /// This is separate from [`Filesystem::open_file_writable`] because writable data files are
    /// opened in append mode. On Windows, append-only handles cannot resize a file.
    ///
    /// # Errors
    ///
    /// If `size` is greater than the current file size, or an I/O error occurs while truncating or
    /// synchronizing the file, an error is returned.
    async fn truncate_file(&self, path: &Path, size: u64) -> io::Result<()>;

    /// Deletes a file.
    ///
    /// # Errors
    ///
    /// If an I/O error occurred when attempting to delete the file, an error variant will be
    /// returned describing the underlying error.
    fn delete_file<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = io::Result<()>> + Send + 'a;

    /// Lists files in a directory.
    ///
    /// # Errors
    ///
    /// If an I/O error occurred when attempting to list the directory, an error variant will be
    /// returned describing the underlying error.
    fn list_files<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = io::Result<Vec<PathBuf>>> + Send + 'a;

    /// Returns whether the buffer should spawn its periodic stale data file cleanup task.
    fn supports_background_cleanup(&self) -> bool {
        true
    }

    /// Binds files opened through this value to the current disk-buffer session.
    ///
    /// Test filesystems do not launch detached writes and therefore need no session binding.
    fn bind_buffer_session(&mut self, _session_guard: Weak<dyn Send + Sync>) {}
}

pub trait AsyncFile: AsyncRead + AsyncWrite + Send + Sync + Unpin {
    /// Whether a cancelled write may still commit bytes in the background.
    fn has_pending_write(&self) -> bool {
        false
    }

    /// Writes at most one chunk that the caller can account for and resume after a short write.
    ///
    /// Implementations used for production data files must not report bytes as written until the
    /// underlying filesystem write has completed. A blocking kernel write cannot be cancelled
    /// portably, so production implementations keep a cancelled syscall owned by the buffer
    /// session until it returns. The default is suitable for test files and other `AsyncWrite`
    /// implementations that already provide that guarantee.
    async fn write_resumable(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write(buf).await
    }

    /// Queries metadata about the underlying file.
    ///
    /// # Errors
    ///
    /// If an I/O error occurred when attempting to get the metadata for the file, an error variant
    /// will be returned describing the underlying error.
    async fn metadata(&self) -> io::Result<Metadata>;

    /// Truncates the underlying file to the specified size.
    ///
    /// # Errors
    /// If `size` is greater than the current file size, or an I/O error occurred when attempting to
    /// truncate the file, an error variant will be returned describing the underlying error.
    async fn truncate(&self, size: u64) -> io::Result<()>;

    /// Attempts to synchronize all OS-internal data, and metadata, to disk.
    ///
    /// This function will attempt to ensure that all in-memory data reaches the filesystem before returning.
    ///
    /// This can be used to handle errors that would otherwise only be caught when the File is closed. Dropping a file will ignore errors in synchronizing this in-memory data.
    ///
    /// # Errors
    /// If an I/O error occurred when attempting to synchronize the file data and metadata to disk,
    /// an error variant will be returned describing the underlying error.
    async fn sync_all(&self) -> io::Result<()>;
}

pub trait ReadableMemoryMap: AsRef<[u8]> + Send + Sync {}

pub trait WritableMemoryMap: ReadableMemoryMap {
    /// Flushes outstanding memory map modifications to disk.
    ///
    /// When this method returns with a non-error result, all outstanding changes to a file-backed
    /// memory map are guaranteed to be durably stored. The file’s metadata (including last
    /// modification timestamp) may not be updated.
    fn flush(&self) -> io::Result<()>;
}

/// A normal filesystem used for production operations.
///
/// Uses Tokio's `File` for asynchronous file reading/writing, and `memmap2` for memory-mapped files.
#[derive(Clone, Default)]
pub struct ProductionFilesystem {
    session_guard: Option<Weak<dyn Send + Sync>>,
    #[cfg(test)]
    write_gate: Option<Arc<TestWriteGate>>,
}

/// Production file handle with session-owned blocking data writes.
pub struct ProductionFile {
    inner: tokio::fs::File,
    blocking_writer: Option<Arc<std::fs::File>>,
    pending_write: Option<PendingWrite>,
    session_guard: Option<Weak<dyn Send + Sync>>,
    #[cfg(test)]
    write_gate: Option<Arc<TestWriteGate>>,
}

struct PendingWrite {
    task: tokio::task::JoinHandle<io::Result<usize>>,
}

impl ProductionFile {
    fn new(
        inner: tokio::fs::File,
        session_guard: Option<Weak<dyn Send + Sync>>,
        #[cfg(test)] write_gate: Option<Arc<TestWriteGate>>,
    ) -> Self {
        Self {
            inner,
            blocking_writer: None,
            pending_write: None,
            session_guard,
            #[cfg(test)]
            write_gate,
        }
    }

    async fn blocking_writer(&mut self) -> io::Result<Arc<std::fs::File>> {
        if let Some(file) = &self.blocking_writer {
            return Ok(Arc::clone(file));
        }

        let file = Arc::new(self.inner.try_clone().await?.into_std().await);
        self.blocking_writer = Some(Arc::clone(&file));
        Ok(file)
    }

    #[cfg(test)]
    pub(crate) fn has_blocking_writer(&self) -> bool {
        self.blocking_writer.is_some()
    }
}

impl fmt::Debug for ProductionFilesystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProductionFilesystem")
            .finish_non_exhaustive()
    }
}

impl ProductionFilesystem {
    fn production_file(&self, file: tokio::fs::File) -> ProductionFile {
        ProductionFile::new(
            file,
            self.session_guard.clone(),
            #[cfg(test)]
            self.write_gate.clone(),
        )
    }
}

#[cfg(test)]
pub(crate) struct TestWriteGate {
    released: Mutex<bool>,
    release: std::sync::Condvar,
    started: std::sync::atomic::AtomicBool,
    started_notify: tokio::sync::Notify,
}

#[cfg(test)]
pub(crate) struct StalledWrites {
    pub(crate) filesystem: ProductionFilesystem,
    pub(crate) gate: Arc<TestWriteGate>,
}

#[cfg(test)]
impl TestWriteGate {
    fn new() -> Self {
        Self {
            released: Mutex::new(false),
            release: std::sync::Condvar::new(),
            started: std::sync::atomic::AtomicBool::new(false),
            started_notify: tokio::sync::Notify::new(),
        }
    }

    fn wait(&self) {
        self.started
            .store(true, std::sync::atomic::Ordering::Release);
        self.started_notify.notify_waiters();
        let mut released = self.released.lock().unwrap();
        while !*released {
            released = self.release.wait(released).unwrap();
        }
    }

    pub(crate) async fn wait_until_started(&self) {
        while !self.started.load(std::sync::atomic::Ordering::Acquire) {
            self.started_notify.notified().await;
        }
    }

    pub(crate) fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.release.notify_all();
    }
}

#[cfg(test)]
impl ProductionFilesystem {
    pub(crate) fn with_stalled_writes() -> StalledWrites {
        let gate = Arc::new(TestWriteGate::new());
        StalledWrites {
            filesystem: Self {
                write_gate: Some(Arc::clone(&gate)),
                ..Self::default()
            },
            gate,
        }
    }
}

impl fmt::Debug for ProductionFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProductionFile")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl AsyncRead for ProductionFile {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for ProductionFile {
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

impl Filesystem for ProductionFilesystem {
    type File = ProductionFile;
    type MemoryMap = memmap2::Mmap;
    type MutableMemoryMap = memmap2::MmapMut;

    async fn open_file_writable(&self, path: &Path) -> io::Result<Self::File> {
        let file = create_writable_file_options(false)
            .append(true)
            .open(path)
            .await?;
        Ok(self.production_file(file))
    }

    async fn open_file_writable_atomic(&self, path: &Path) -> io::Result<Self::File> {
        let file = create_writable_file_options(true)
            .append(true)
            .open(path)
            .await?;
        Ok(self.production_file(file))
    }

    async fn open_file_readable(&self, path: &Path) -> io::Result<Self::File> {
        let file = open_readable_file_options().open(path).await?;
        Ok(self.production_file(file))
    }

    async fn open_mmap_readable(&self, path: &Path) -> io::Result<Self::MemoryMap> {
        let file = open_readable_file_options().open(path).await?;
        let std_file = file.into_std().await;
        unsafe { memmap2::Mmap::map(&std_file) }
    }

    async fn open_mmap_writable(&self, path: &Path) -> io::Result<Self::MutableMemoryMap> {
        let file = open_writable_file_options().open(path).await?;

        let std_file = file.into_std().await;
        unsafe { memmap2::MmapMut::map_mut(&std_file) }
    }

    async fn truncate_file(&self, path: &Path, size: u64) -> io::Result<()> {
        // Windows append handles can write to a file, but cannot resize it. Open a separate
        // read/write handle for truncation.
        let file = create_writable_file_options(false).open(path).await?;
        AsyncFile::truncate(&file, size).await?;
        AsyncFile::sync_all(&file).await
    }

    fn delete_file<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = io::Result<()>> + Send + 'a {
        tokio::fs::remove_file(path)
    }

    #[allow(clippy::manual_async_fn)]
    fn list_files<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = io::Result<Vec<PathBuf>>> + Send + 'a {
        async move {
            let mut entries = tokio::fs::read_dir(path).await?;
            let mut files = Vec::new();

            while let Some(entry) = entries.next_entry().await? {
                if entry.file_type().await?.is_file() {
                    files.push(entry.path());
                }
            }

            Ok(files)
        }
    }

    fn bind_buffer_session(&mut self, session_guard: Weak<dyn Send + Sync>) {
        self.session_guard = Some(session_guard);
    }
}

/// Builds a set of `OpenOptions` for opening a file as readable/writable.
fn open_writable_file_options() -> OpenOptions {
    let mut open_options = OpenOptions::new();
    open_options.read(true).write(true);

    #[cfg(unix)]
    {
        open_options.mode(FILE_MODE_OWNER_RW_GROUP_RO);
    }

    open_options
}

/// Builds a set of `OpenOptions` for opening a file as readable/writable, creating it if it does
/// not already exist.
///
/// When `create_atomic` is set to `true`, this ensures that the operation only succeeds if the
/// subsequent call to `open` is able to create the file, ensuring that another process did not
/// create it before us. Otherwise, the normal create mode is configured, which creates the file if
/// it does not exist but does not throw an error if it already did.
///
/// On Unix platforms, file permissions will be set so that only the owning user of the file can
/// write to it, the owning group can read it, and the file is inaccessible otherwise.
fn create_writable_file_options(create_atomic: bool) -> OpenOptions {
    let mut open_options = open_writable_file_options();

    #[cfg(unix)]
    {
        open_options.mode(FILE_MODE_OWNER_RW_GROUP_RO);
    }

    if create_atomic {
        open_options.create_new(true);
    } else {
        open_options.create(true);
    }

    open_options
}

/// Builds a set of `OpenOptions` for opening a file as readable.
fn open_readable_file_options() -> OpenOptions {
    let mut open_options = OpenOptions::new();
    open_options.read(true);
    open_options
}

impl AsyncFile for ProductionFile {
    fn has_pending_write(&self) -> bool {
        self.pending_write.is_some()
    }

    async fn write_resumable(&mut self, buf: &[u8]) -> io::Result<usize> {
        let write_len = buf.len().min(RESUMABLE_WRITE_CHUNK_SIZE);

        if self.pending_write.is_none() {
            let file = self.blocking_writer().await?;
            let session_guard = self.session_guard.as_ref().and_then(Weak::upgrade);
            let buf = buf[..write_len].to_vec();
            #[cfg(test)]
            let write_gate = self.write_gate.clone();
            let task = tokio::task::spawn_blocking(move || {
                // Keep the buffer session locked until a detached syscall has actually returned.
                let _session_guard = session_guard;
                #[cfg(test)]
                if let Some(gate) = write_gate {
                    gate.wait();
                }
                let mut file = &*file;
                #[allow(clippy::disallowed_methods)]
                file.write(&buf)
            });
            self.pending_write = Some(PendingWrite { task });
        }

        let result = (&mut self.pending_write.as_mut().unwrap().task)
            .await
            .map_err(io::Error::other)?;
        self.pending_write = None;
        result
    }

    async fn metadata(&self) -> io::Result<Metadata> {
        let metadata = self.inner.metadata().await?;
        Ok(Metadata {
            len: metadata.len(),
        })
    }

    async fn truncate(&self, size: u64) -> io::Result<()> {
        let current_size = self.inner.metadata().await?.len();
        if size > current_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot extend a file through the truncation API",
            ));
        }

        self.inner.set_len(size).await
    }

    async fn sync_all(&self) -> io::Result<()> {
        self.inner.sync_all().await
    }
}

impl AsyncFile for tokio::fs::File {
    async fn metadata(&self) -> io::Result<Metadata> {
        let metadata = self.metadata().await?;
        Ok(Metadata {
            len: metadata.len(),
        })
    }

    async fn truncate(&self, size: u64) -> io::Result<()> {
        let current_size = self.metadata().await?.len();
        if size > current_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot extend a file through the truncation API",
            ));
        }

        self.set_len(size).await
    }

    async fn sync_all(&self) -> io::Result<()> {
        self.sync_all().await
    }
}

impl ReadableMemoryMap for memmap2::Mmap {}

impl ReadableMemoryMap for memmap2::MmapMut {}

impl WritableMemoryMap for memmap2::MmapMut {
    fn flush(&self) -> io::Result<()> {
        self.flush()
    }
}
