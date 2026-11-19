use std::{future::Future, io, path::Path};

use chrono::{DateTime, Utc};
use tokio::{
    fs::{self, File},
    io::{AsyncRead, AsyncSeek},
};

use crate::metadata_ext::{AsyncFileInfo, PortableFileExt};

/// Portable file metadata needed by `file-source` and `file-source-common`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMetadata {
    pub len: u64,
    pub is_dir: bool,
    pub created: Option<DateTime<Utc>>,
    pub modified: Option<DateTime<Utc>>,
}

impl From<&std::fs::Metadata> for FileMetadata {
    fn from(m: &std::fs::Metadata) -> Self {
        Self {
            len: m.len(),
            is_dir: m.is_dir(),
            created: m.created().ok().map(DateTime::<Utc>::from),
            modified: m.modified().ok().map(DateTime::<Utc>::from),
        }
    }
}

/// Portable device + inode identity for an open file handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub dev: u64,
    pub ino: u64,
}

/// An open async file handle supporting reads, seeks, metadata, and identity queries.
pub trait AsyncFile: AsyncRead + AsyncSeek + Unpin + Send + 'static {
    /// Returns metadata for the open file handle (works even after unlink on Unix).
    fn metadata(&self) -> impl Future<Output = io::Result<FileMetadata>> + Send;

    /// Returns the portable `(dev, ino)` identity of the open file handle.
    fn identity(&self) -> impl Future<Output = io::Result<FileIdentity>> + Send;
}

impl AsyncFile for File {
    async fn metadata(&self) -> io::Result<FileMetadata> {
        let m = self.metadata().await?;
        Ok(FileMetadata::from(&m))
    }

    async fn identity(&self) -> io::Result<FileIdentity> {
        let info = self.file_info().await?;
        Ok(FileIdentity {
            dev: info.portable_dev(),
            ino: info.portable_ino(),
        })
    }
}

/// Abstraction over filesystem operations used by `file-source` and `file-source-common`.
pub trait FileSystem: Clone + Send + Sync + 'static {
    type File: AsyncFile;

    /// Opens a file in read-only mode.
    fn open(&self, path: &Path) -> impl Future<Output = io::Result<Self::File>> + Send;

    /// Queries metadata for a path (following symlinks, matching `tokio::fs::metadata`).
    fn metadata(&self, path: &Path) -> impl Future<Output = io::Result<FileMetadata>> + Send;

    /// Removes a file from the filesystem.
    fn remove_file(&self, path: &Path) -> impl Future<Output = io::Result<()>> + Send;

    /// Renames a file from `from` to `to`, replacing `to` if it exists.
    fn rename(&self, from: &Path, to: &Path) -> impl Future<Output = io::Result<()>> + Send;

    /// Creates/truncates `path`, writes all `data`, and syncs it to durable storage.
    fn write_file_sync(
        &self,
        path: &Path,
        data: Vec<u8>,
    ) -> impl Future<Output = io::Result<()>> + Send;
}

/// Production [`FileSystem`] backed by `tokio::fs` and `std::fs`.
#[derive(Debug, Clone, Copy, Default)]
pub struct OsFileSystem;

impl FileSystem for OsFileSystem {
    type File = File;

    async fn open(&self, path: &Path) -> io::Result<Self::File> {
        File::open(path).await
    }

    async fn metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        let m = fs::metadata(path).await?;
        Ok(FileMetadata::from(&m))
    }

    async fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path).await
    }

    async fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to).await
    }

    async fn write_file_sync(&self, path: &Path, data: Vec<u8>) -> io::Result<()> {
        use std::io::Write as _;

        let path = path.to_path_buf();
        // spawn_blocking shouldn't be needed: https://github.com/vectordotdev/vector/issues/23743
        tokio::task::spawn_blocking(move || -> io::Result<()> {
            let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
            f.write_all(&data)?;
            f.into_inner()?.sync_all()?;
            Ok(())
        })
        .await
        .map_err(io::Error::other)?
    }
}
