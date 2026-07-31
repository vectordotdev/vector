use std::{
    cmp,
    collections::HashMap,
    fmt,
    future::{Future, ready},
    io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::variants::disk_v2::{
    Filesystem,
    backed_archive::BackedArchive,
    io::{AsyncFile, Metadata, ReadableMemoryMap, WritableMemoryMap},
    record::Record,
};

fn io_err_already_exists() -> io::Error {
    io::Error::new(io::ErrorKind::AlreadyExists, "file already exists")
}

fn io_err_not_found() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "file not found")
}

fn io_err_permission_denied() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, "permission denied")
}

struct FileInner {
    buf: Option<Vec<u8>>,
    return_eof_on_next_read: bool,
    error_at_eof: bool,
}

impl FileInner {
    fn consume_buf(&mut self) -> Vec<u8> {
        self.buf.take().expect("tried to consume buf, but empty")
    }

    fn return_buf(&mut self, buf: Vec<u8>) {
        let previous = self.buf.replace(buf);
        assert!(previous.is_none());
    }
}

impl Default for FileInner {
    fn default() -> Self {
        Self {
            buf: Some(Vec::new()),
            return_eof_on_next_read: false,
            error_at_eof: false,
        }
    }
}

impl fmt::Debug for FileInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let buf_debug = match &self.buf {
            None => String::from("(none)"),
            Some(buf) => format!("({} bytes)", buf.len()),
        };

        f.debug_struct("FileInner")
            .field("buf", &buf_debug)
            .field("return_eof_on_next_read", &self.return_eof_on_next_read)
            .field("error_at_eof", &self.error_at_eof)
            .finish()
    }
}

#[derive(Clone)]
pub struct TestFile {
    inner: Arc<Mutex<FileInner>>,
    is_writable: bool,
    read_pos: usize,
}

impl TestFile {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FileInner::default())),
            is_writable: false,
            read_pos: 0,
        }
    }

    fn set_readable(&mut self) {
        self.is_writable = false;
    }

    fn set_writable(&mut self) {
        self.is_writable = true;
    }

    fn as_mmap(&self) -> TestMmap {
        let inner = Arc::clone(&self.inner);
        inner.into()
    }
}

impl fmt::Debug for TestFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner.lock();
        f.debug_struct("TestFile")
            .field("data", &inner)
            .field("writable", &self.is_writable)
            .field("read_pos", &self.read_pos)
            .finish()
    }
}

pub struct TestMmap {
    inner: Arc<Mutex<FileInner>>,
    buf: Option<Vec<u8>>,
}

impl From<Arc<Mutex<FileInner>>> for TestMmap {
    fn from(inner: Arc<Mutex<FileInner>>) -> Self {
        let buf = {
            let mut guard = inner.lock().expect("poisoned");
            guard.consume_buf()
        };

        Self {
            inner,
            buf: Some(buf),
        }
    }
}

impl Drop for TestMmap {
    fn drop(&mut self) {
        let buf = self.buf.take().expect("buf must exist");
        let mut inner = self.inner.lock().expect("poisoned");
        inner.return_buf(buf);
    }
}

impl AsRef<[u8]> for TestMmap {
    fn as_ref(&self) -> &[u8] {
        self.buf.as_ref().expect("mmap buf consumed").as_slice()
    }
}

impl ReadableMemoryMap for TestMmap {}

impl WritableMemoryMap for TestMmap {
    fn flush(&self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncRead for TestFile {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let new_read_pos = {
            let mut inner = self.inner.lock().expect("poisoned");
            if inner.return_eof_on_next_read {
                inner.return_eof_on_next_read = false;
                return Poll::Ready(Ok(()));
            }

            let cap = buf.remaining();
            let pos = self.read_pos;
            let available = inner.buf.as_ref().expect("file buf consumed").len() - pos;
            if available == 0 && inner.error_at_eof {
                return Poll::Ready(Err(io::Error::other("unexpected read at EOF")));
            }
            let n = cmp::min(cap, available);

            let to = pos + n;
            let src = inner.buf.as_mut().expect("file buf consumed");
            buf.put_slice(&src[pos..to]);
            to
        };

        self.read_pos = new_read_pos;

        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for TestFile {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if !self.is_writable {
            return Err(io_err_permission_denied()).into();
        }

        let mut inner = self.inner.lock().expect("poisoned");
        let dst = inner.buf.as_mut().expect("file buf consumed");
        dst.extend_from_slice(buf);

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if !self.is_writable {
            return Err(io_err_permission_denied()).into();
        }

        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if !self.is_writable {
            return Err(io_err_permission_denied()).into();
        }

        Poll::Ready(Ok(()))
    }
}

impl AsyncFile for TestFile {
    #[instrument(skip(self), level = "debug")]
    async fn metadata(&self) -> io::Result<Metadata> {
        let len = {
            let inner = self.inner.lock().expect("poisoned");
            inner.buf.as_ref().expect("file buf consumed").len()
        };

        Ok(Metadata { len: len as u64 })
    }

    async fn truncate(&self, size: u64) -> io::Result<()> {
        if !self.is_writable {
            return Err(io_err_permission_denied());
        }

        let size = usize::try_from(size)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "file size out of range"))?;
        let mut inner = self.inner.lock().expect("poisoned");
        let buf = inner.buf.as_mut().expect("file buf consumed");
        if size > buf.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot extend a file through the truncation API",
            ));
        }
        buf.truncate(size);

        Ok(())
    }

    async fn sync_all(&self) -> io::Result<()> {
        Ok(())
    }
}

// Inner state of the test filesystem.
#[derive(Debug, Default)]
struct FilesystemInner {
    files: HashMap<PathBuf, TestFile>,
}

impl FilesystemInner {
    #[instrument(skip(self), level = "debug")]
    fn open_file_writable(&mut self, path: &Path) -> TestFile {
        let file = self
            .files
            .entry(path.to_owned())
            .or_insert_with(TestFile::new);
        let mut new_file = file.clone();
        new_file.set_writable();

        new_file
    }

    #[instrument(skip(self), level = "debug")]
    fn open_file_writable_atomic(&mut self, path: &Path) -> Option<TestFile> {
        if self.files.contains_key(path) {
            None
        } else {
            let mut new_file = TestFile::new();
            new_file.set_writable();

            self.files.insert(path.to_owned(), new_file.clone());

            Some(new_file)
        }
    }

    fn open_file_readable(&mut self, path: &Path) -> Option<TestFile> {
        self.files.get(path).cloned().map(|mut f| {
            f.set_readable();
            f
        })
    }

    fn open_mmap_readable(&mut self, path: &Path) -> Option<TestMmap> {
        self.files.get(path).map(TestFile::as_mmap)
    }

    fn open_mmap_writable(&mut self, path: &Path) -> Option<TestMmap> {
        self.files.get(path).map(TestFile::as_mmap)
    }

    fn delete_file(&mut self, path: &Path) -> bool {
        self.files.remove(path).is_some()
    }

    fn list_files(&self, path: &Path) -> Vec<PathBuf> {
        self.files
            .keys()
            .filter(|file_path| file_path.parent() == Some(path))
            .cloned()
            .collect()
    }
}

/// A `Filesystem` that tracks files in memory and allows introspection from the outside.
pub struct TestFilesystem {
    inner: Arc<Mutex<FilesystemInner>>,
}

impl fmt::Debug for TestFilesystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner.lock().expect("poisoned");
        f.debug_struct("TestFilesystem")
            .field("files", &inner.files)
            .finish()
    }
}

impl Clone for TestFilesystem {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Default for TestFilesystem {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FilesystemInner::default())),
        }
    }
}

impl TestFilesystem {
    /// Makes one read report EOF without advancing its file position.
    pub(crate) fn return_eof_on_next_read(&self, path: &Path) {
        let inner = self.inner.lock().expect("poisoned");
        let file = inner.files.get(path).expect("file should exist");
        let mut file_inner = file.inner.lock().expect("poisoned");
        file_inner.return_eof_on_next_read = true;
    }

    /// Makes reads fail at EOF.
    pub(crate) fn error_at_eof(&self, path: &Path) {
        let inner = self.inner.lock().expect("poisoned");
        let file = inner.files.get(path).expect("file should exist");
        let mut file_inner = file.inner.lock().expect("poisoned");
        file_inner.error_at_eof = true;
    }

    /// Corrupts the checksum of the only record in a data file.
    pub(crate) fn corrupt_record_checksum(&self, path: &Path) {
        let inner = self.inner.lock().expect("poisoned");
        let file = inner.files.get(path).expect("file should exist");
        let mut file_inner = file.inner.lock().expect("poisoned");
        let buf = file_inner.buf.as_mut().expect("file buf consumed");
        let mut backed_record = BackedArchive::<_, Record>::from_backing(buf.as_mut_slice())
            .expect("archive should not fail");
        let record = backed_record.get_archive_mut();
        let projected_checksum = unsafe { record.map_unchecked_mut(|record| &mut record.checksum) };
        *projected_checksum.get_mut() ^= 1 << 15;
    }

    /// Appends a copy of a file's bytes without publishing writer progress.
    pub(crate) fn append_unpublished_file_contents(&self, path: &Path) {
        let inner = self.inner.lock().expect("poisoned");
        let file = inner.files.get(path).expect("file should exist");
        let mut file_inner = file.inner.lock().expect("poisoned");
        let buf = file_inner.buf.as_mut().expect("file buf consumed");
        buf.extend_from_within(..);
    }
}

impl Filesystem for TestFilesystem {
    type File = TestFile;
    type MemoryMap = TestMmap;
    type MutableMemoryMap = TestMmap;

    async fn open_file_writable(&self, path: &Path) -> io::Result<Self::File> {
        let mut inner = self.inner.lock().expect("poisoned");
        Ok(inner.open_file_writable(path))
    }

    async fn open_file_writable_atomic(&self, path: &Path) -> io::Result<Self::File> {
        let mut inner = self.inner.lock().expect("poisoned");
        match inner.open_file_writable_atomic(path) {
            Some(file) => Ok(file),
            None => Err(io_err_already_exists()),
        }
    }

    async fn open_file_readable(&self, path: &Path) -> io::Result<Self::File> {
        let mut inner = self.inner.lock().expect("poisoned");
        match inner.open_file_readable(path) {
            Some(file) => Ok(file),
            None => Err(io_err_not_found()),
        }
    }

    async fn open_mmap_readable(&self, path: &Path) -> io::Result<Self::MemoryMap> {
        let mut inner = self.inner.lock().expect("poisoned");
        match inner.open_mmap_readable(path) {
            Some(mmap) => Ok(mmap),
            None => Err(io_err_not_found()),
        }
    }

    async fn open_mmap_writable(&self, path: &Path) -> io::Result<Self::MutableMemoryMap> {
        let mut inner = self.inner.lock().expect("poisoned");
        match inner.open_mmap_writable(path) {
            Some(mmap) => Ok(mmap),
            None => Err(io_err_not_found()),
        }
    }

    async fn truncate_file(&self, path: &Path, size: u64) -> io::Result<()> {
        let file = {
            let mut inner = self.inner.lock().expect("poisoned");
            inner.open_file_writable(path)
        };
        file.truncate(size).await?;
        file.sync_all().await
    }

    fn delete_file<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = io::Result<()>> + Send + 'a {
        let mut inner = self.inner.lock().expect("poisoned");
        let result = if inner.delete_file(path) {
            Ok(())
        } else {
            Err(io_err_not_found())
        };
        ready(result)
    }

    fn list_files<'a>(
        &'a self,
        path: &'a Path,
    ) -> impl Future<Output = io::Result<Vec<PathBuf>>> + Send + 'a {
        let inner = self.inner.lock().expect("poisoned");
        ready(Ok(inner.list_files(path)))
    }

    fn supports_background_cleanup(&self) -> bool {
        false
    }
}
