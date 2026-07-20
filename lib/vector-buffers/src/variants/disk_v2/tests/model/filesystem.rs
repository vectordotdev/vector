use std::{
    cmp,
    collections::{HashMap, HashSet},
    fmt, io,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use proptest::{
    prop_oneof,
    strategy::{Just, Strategy},
};

use super::action::TailPersistence;
use crate::variants::disk_v2::{
    Filesystem,
    io::{AsyncFile, Metadata, ReadableMemoryMap, WritableMemoryMap},
};

/// Byte written into a garbage boundary unit. Arbitrary value chosen to
/// distinguish from bytes already written by the model.
const GARBAGE_BYTE: u8 = 0xFF;

/// The largest write a filesystem persists atomically, setting the granularity at which a torn
/// writeback tears.
#[derive(Clone, Copy, Debug, Default)]
pub enum FsAtomicity {
    /// Writes are atomic per 512-byte sector. Torn writes lands on a 512-byte
    /// boundary.
    #[default]
    Sector,
    /// Writes are atomic per 4096-byte block. Torn write lands on a 4096-byte
    /// boundary.
    Block,
}

impl FsAtomicity {
    fn tear_granularity(self) -> usize {
        match self {
            FsAtomicity::Sector => 512,
            FsAtomicity::Block => 4096,
        }
    }
}

pub fn arb_fs_atomicity() -> impl Strategy<Value = FsAtomicity> {
    prop_oneof![Just(FsAtomicity::Sector), Just(FsAtomicity::Block)]
}

/// The largest `unit` boundary strictly below `len` — i.e. the offset that drops the trailing
/// (possibly partial) unit.
fn unit_floor_below(len: usize, unit: usize) -> usize {
    (len.saturating_sub(1) / unit) * unit
}

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
    // The live contents, as seen by the running reader and writer.
    buf: Option<Vec<u8>>,
    // The contents known to have reached disk via a barrier.
    durable: Vec<u8>,
}

impl FileInner {
    fn consume_buf(&mut self) -> Vec<u8> {
        self.buf.take().expect("tried to consume buf, but empty")
    }

    fn return_buf(&mut self, buf: Vec<u8>) {
        let previous = self.buf.replace(buf);
        assert!(previous.is_none());
    }

    fn persist(&mut self) {
        self.durable = self.buf.as_ref().expect("file buf consumed").clone();
    }
}

impl Default for FileInner {
    fn default() -> Self {
        Self {
            buf: Some(Vec::new()),
            durable: Vec::new(),
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

    // Rebuilds a file from its durable snapshot after a crash. The live contents equal the
    // durable contents, since that is all that reached disk.
    fn from_durable(durable: Vec<u8>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(FileInner {
                buf: Some(durable.clone()),
                durable,
            })),
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
        let buf = self.buf.as_ref().expect("mmap buf consumed");
        self.inner.lock().expect("poisoned").durable = buf.clone();
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
            let src = inner.buf.as_mut().expect("file buf consumed");

            let cap = buf.remaining();
            let pos = self.read_pos;
            let available = src.len() - pos;
            let n = cmp::min(cap, available);

            let to = pos + n;
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

    async fn sync_all(&self) -> io::Result<()> {
        self.inner.lock().expect("poisoned").persist();
        Ok(())
    }
}

// Inner state of the test filesystem.
#[derive(Debug, Default)]
struct FilesystemInner {
    atomicity: FsAtomicity,
    files: HashMap<PathBuf, TestFile>,
    // Paths whose directory entry has reached disk and so survive a crash.
    durable_names: HashSet<PathBuf>,
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

    // Models the OS flushing a data file's dirty tail to disk ahead of an
    // explicit `sync_all`.
    fn writeback_data(&mut self, path: &Path, tail: TailPersistence) {
        let Some(file) = self.files.get(path) else {
            return;
        };
        let unit = self.atomicity.tear_granularity();
        let mut inner = file.inner.lock().expect("poisoned");
        let live = inner.buf.as_ref().expect("data file buf consumed").clone();
        let durable_len = inner.durable.len();
        if live.len() <= durable_len {
            return;
        }
        let boundary = unit_floor_below(live.len(), unit).max(durable_len);
        inner.durable = match tail {
            TailPersistence::AllDirty => live,
            TailPersistence::TornAtBoundary => live[..boundary].to_vec(),
            TailPersistence::GarbageBoundary => {
                let mut durable = live[..boundary].to_vec();
                durable.resize(live.len(), GARBAGE_BYTE);
                durable
            }
        };
    }

    // Models the OS flushing one file's directory entry to disk.
    fn writeback_dir_entry(&mut self, path: &Path) {
        if self.files.contains_key(path) {
            self.durable_names.insert(path.to_owned());
        } else {
            self.durable_names.remove(path);
        }
    }

    // Discards everything that has not been forced durable by a barrier.
    fn crash(&mut self) {
        let live = std::mem::take(&mut self.files);
        for (path, file) in live {
            if self.durable_names.contains(&path) {
                let durable = file.inner.lock().expect("poisoned").durable.clone();
                self.files.insert(path, TestFile::from_durable(durable));
            }
        }
    }
}

/// A `Filesystem` that tracks files in memory and allows introspection from the outside.
pub struct TestFilesystem {
    inner: Arc<Mutex<FilesystemInner>>,
}

impl TestFilesystem {
    /// A filesystem with the given append atomicity.
    pub fn with_atomicity(atomicity: FsAtomicity) -> Self {
        Self {
            inner: Arc::new(Mutex::new(FilesystemInner {
                atomicity,
                ..Default::default()
            })),
        }
    }

    /// Models a hard crash, Discarding everything not forced durable by a
    /// barrier.
    pub fn crash(&self) {
        self.inner.lock().expect("poisoned").crash();
    }

    /// Models the OS flushing a data file's dirty tail ahead of an explicit
    /// `sync_all`.
    pub fn writeback_data(&self, path: &Path, tail: TailPersistence) {
        self.inner
            .lock()
            .expect("poisoned")
            .writeback_data(path, tail);
    }

    /// Models the OS flushing one file's directory entry ahead of a directory
    /// fsync.
    pub fn writeback_dir_entry(&self, path: &Path) {
        self.inner
            .lock()
            .expect("poisoned")
            .writeback_dir_entry(path);
    }
}

impl fmt::Debug for TestFilesystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner.lock().expect("poisoned");
        f.debug_struct("TestFilesystem")
            .field("atomicity", &inner.atomicity)
            .field("files", &inner.files)
            .finish()
    }
}

impl Clone for TestFilesystem {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Default for TestFilesystem {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FilesystemInner::default())),
        }
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

    async fn delete_file(&self, path: &Path) -> io::Result<()> {
        let mut inner = self.inner.lock().expect("poisoned");
        if inner.delete_file(path) {
            Ok(())
        } else {
            Err(io_err_not_found())
        }
    }
}
