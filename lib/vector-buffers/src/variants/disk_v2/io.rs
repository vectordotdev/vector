use std::{
    future::Future,
    io,
    path::{Path, PathBuf},
};

use tokio::{
    fs::OpenOptions,
    io::{AsyncRead, AsyncWrite},
};

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
}

pub trait AsyncFile: AsyncRead + AsyncWrite + Send + Sync {
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
#[derive(Clone, Debug)]
pub struct ProductionFilesystem;

impl Filesystem for ProductionFilesystem {
    type File = tokio::fs::File;
    type MemoryMap = memmap2::Mmap;
    type MutableMemoryMap = memmap2::MmapMut;

    async fn open_file_writable(&self, path: &Path) -> io::Result<Self::File> {
        create_writable_file_options(false)
            .append(true)
            .open(path)
            .await
    }

    async fn open_file_writable_atomic(&self, path: &Path) -> io::Result<Self::File> {
        create_writable_file_options(true)
            .append(true)
            .open(path)
            .await
    }

    async fn open_file_readable(&self, path: &Path) -> io::Result<Self::File> {
        open_readable_file_options().open(path).await
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
