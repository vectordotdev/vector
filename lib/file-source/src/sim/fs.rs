use std::{
    collections::{BTreeMap, HashMap},
    io::{self, ErrorKind, SeekFrom},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use chrono::{DateTime, Utc};
use file_source_common::{AsyncFile, Clock, FileIdentity, FileMetadata, FileSystem};
use glob::Pattern;
use tokio::io::{AsyncRead, AsyncSeek, ReadBuf};

use crate::{
    paths_provider::PathsProvider,
    scheduler::{FsEvent, FsEventSource},
};

use super::clock::SimClock;

/// Controls how [`SimFileSystem`] delivers [`FsEvent`] hints to [`SimFsEventSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventDelivery {
    /// Deliver all `Created`/`Modified`/`Removed` events normally (like healthy `inotify`/`kqueue`).
    #[default]
    Normal,
    /// Silently drop all events (models NFS/SMB mounts where OS notifications do not fire).
    DropAll,
    /// Deterministically drop every `drop_every_n`-th event (models flaky/coalesced OS notifications).
    Flaky { drop_every_n: u64 },
}

/// Operation counters collected by [`SimFileSystem`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FsStats {
    pub opens: u64,
    pub reads: u64,
    pub eof_reads: u64,
    pub read_bytes: u64,
    pub seeks: u64,
    pub stats: u64,
    pub removes: u64,
    pub renames: u64,
    pub writes: u64,
    pub globs: u64,
}

#[derive(Debug)]
struct SimInode {
    dev: u64,
    ino: u64,
    data: Vec<u8>,
    created: DateTime<Utc>,
    modified: DateTime<Utc>,
    is_dir: bool,
    readable: bool,
    open_handles: usize,
    link_count: usize,
}

#[derive(Debug)]
struct SimFsInner {
    clock: SimClock,
    next_ino: u64,
    inodes: HashMap<u64, SimInode>,
    entries: BTreeMap<PathBuf, u64>,
    symlinks: BTreeMap<PathBuf, PathBuf>,
    stats: FsStats,
    next_write_error: Option<ErrorKind>,
    /// If set, the next `write_file_sync` writes only the first `N` bytes and
    /// returns an I/O error (modeling a crash / `ENOSPC` mid-checkpoint write).
    next_partial_write_crash: Option<usize>,
    /// If true, `remove_file` fails with `PermissionDenied` while any handle is open
    /// (approximating Windows delete-sharing restrictions).
    strict_delete_while_open: bool,
    /// If true, freed inode numbers are recycled on subsequent file creation.
    inode_reuse: bool,
    freed_inos: Vec<u64>,
    /// If set, caps the number of bytes returned per `AsyncRead::poll_read` call
    /// (modeling short reads / torn kernel buffer visibility).
    max_bytes_per_read: Option<usize>,
    /// Queued filesystem change notifications for [`SimFsEventSource`].
    fs_events: Vec<FsEvent>,
    event_delivery: EventDelivery,
    event_counter: u64,
}

/// Deterministic in-memory [`FileSystem`] modeling inodes, directory entries,
/// symlinks, open file descriptors, timestamps, fault injection, and syscall counters.
#[derive(Debug, Clone)]
pub struct SimFileSystem {
    inner: Arc<Mutex<SimFsInner>>,
}

impl SimFileSystem {
    #[must_use]
    pub fn new(clock: SimClock) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SimFsInner {
                clock,
                next_ino: 1,
                inodes: HashMap::new(),
                entries: BTreeMap::new(),
                symlinks: BTreeMap::new(),
                stats: FsStats::default(),
                next_write_error: None,
                next_partial_write_crash: None,
                strict_delete_while_open: false,
                inode_reuse: false,
                freed_inos: Vec::new(),
                max_bytes_per_read: None,
                fs_events: Vec::new(),
                event_delivery: EventDelivery::Normal,
                event_counter: 0,
            })),
        }
    }

    /// Returns all currently linked file and symlink paths in deterministic sorted order.
    #[must_use]
    pub fn all_paths(&self) -> Vec<PathBuf> {
        let inner = self.inner.lock().unwrap();
        let mut paths: Vec<PathBuf> = inner
            .entries
            .keys()
            .chain(inner.symlinks.keys())
            .cloned()
            .collect();
        paths.sort();
        paths
    }

    /// Returns a [`SimFsEventSource`] connected to this simulated filesystem.
    #[must_use]
    pub fn event_source(&self) -> SimFsEventSource {
        SimFsEventSource { fs: self.clone() }
    }

    /// Configures how filesystem notifications are delivered (normal vs. dropped/NFS).
    pub fn set_event_delivery(&self, mode: EventDelivery) {
        self.inner.lock().unwrap().event_delivery = mode;
    }

    /// Injects an `FsEvent::Overflow` (modeling `IN_Q_OVERFLOW`).
    pub fn inject_event_overflow(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.fs_events.clear();
        inner.fs_events.push(FsEvent::Overflow);
    }

    #[must_use]
    pub fn stats(&self) -> FsStats {
        self.inner.lock().unwrap().stats
    }

    pub fn reset_stats(&self) {
        self.inner.lock().unwrap().stats = FsStats::default();
    }

    /// Returns the total number of currently open file handles across all inodes.
    #[must_use]
    pub fn active_open_handles(&self) -> usize {
        self.inner
            .lock()
            .unwrap()
            .inodes
            .values()
            .map(|n| n.open_handles)
            .sum()
    }

    pub fn set_strict_delete_while_open(&self, strict: bool) {
        self.inner.lock().unwrap().strict_delete_while_open = strict;
    }

    pub fn enable_inode_reuse(&self, enabled: bool) {
        self.inner.lock().unwrap().inode_reuse = enabled;
    }

    pub fn set_max_bytes_per_read(&self, limit: Option<usize>) {
        self.inner.lock().unwrap().max_bytes_per_read = limit;
    }

    #[allow(dead_code)]
    pub fn inject_next_write_error(&self, err: Option<ErrorKind>) {
        self.inner.lock().unwrap().next_write_error = err;
    }

    /// Injects a partial write crash on the next `write_file_sync`: writes only
    /// the first `truncate_len` bytes and returns an I/O error.
    pub fn inject_partial_write_crash(&self, truncate_len: Option<usize>) {
        self.inner.lock().unwrap().next_partial_write_crash = truncate_len;
    }

    /// Creates a new empty file at `path` (replacing any existing directory entry,
    /// leaving existing open handles pointing to the old inode) and returns its inode number.
    pub fn create_file(&self, path: impl Into<PathBuf>) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        let path = path.into();
        let now = inner.clock.now_utc();
        let ino = allocate_ino(&mut inner);

        inner.symlinks.remove(&path);
        if let Some(old_ino) = inner.entries.insert(path.clone(), ino) {
            unlink_inode(&mut inner, old_ino);
        }

        inner.inodes.insert(
            ino,
            SimInode {
                dev: 1,
                ino,
                data: Vec::new(),
                created: now,
                modified: now,
                is_dir: false,
                readable: true,
                open_handles: 0,
                link_count: 1,
            },
        );
        push_event(&mut inner, FsEvent::Created(path));
        ino
    }

    /// Creates or atomically updates a symlink `link -> target`.
    pub fn set_symlink(&self, link: impl Into<PathBuf>, target: impl Into<PathBuf>) {
        let mut inner = self.inner.lock().unwrap();
        let link = link.into();
        if let Some(old_ino) = inner.entries.remove(&link) {
            unlink_inode(&mut inner, old_ino);
        }
        inner.symlinks.insert(link.clone(), target.into());
        push_event(&mut inner, FsEvent::Modified(link));
    }

    /// Overwrites bytes at `offset` in-place (extending with zero bytes if needed),
    /// modeling preallocated / NULL-padded files.
    pub fn write_at(&self, path: impl AsRef<Path>, offset: usize, bytes: &[u8]) {
        let mut inner = self.inner.lock().unwrap();
        let now = inner.clock.now_utc();
        if let Some(ino) = resolve_ino(&inner, path.as_ref())
            && let Some(node) = inner.inodes.get_mut(&ino)
        {
            let end = offset + bytes.len();
            if node.data.len() < end {
                node.data.resize(end, 0);
            }
            node.data[offset..end].copy_from_slice(bytes);
            node.modified = now;
            push_event(&mut inner, FsEvent::Modified(path.as_ref().to_path_buf()));
        }
    }

    /// Creates a file (if absent) and appends `bytes` to it, returning its inode number.
    pub fn append_bytes(&self, path: impl AsRef<Path>, bytes: &[u8]) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        let path = path.as_ref();
        let now = inner.clock.now_utc();
        let ino = if let Some(existing) = resolve_ino(&inner, path) {
            existing
        } else {
            let ino = allocate_ino(&mut inner);
            inner.entries.insert(path.to_path_buf(), ino);
            inner.inodes.insert(
                ino,
                SimInode {
                    dev: 1,
                    ino,
                    data: Vec::new(),
                    created: now,
                    modified: now,
                    is_dir: false,
                    readable: true,
                    open_handles: 0,
                    link_count: 1,
                },
            );
            ino
        };

        let node = inner.inodes.get_mut(&ino).expect("inode exists");
        node.data.extend_from_slice(bytes);
        node.modified = now;
        push_event(&mut inner, FsEvent::Modified(path.to_path_buf()));
        ino
    }

    /// Appends `bytes` directly to an inode (modeling a writer holding an open fd
    /// even after the file has been renamed or unlinked).
    pub fn append_to_ino(&self, ino: u64, bytes: &[u8]) {
        let mut inner = self.inner.lock().unwrap();
        let now = inner.clock.now_utc();
        if let Some(node) = inner.inodes.get_mut(&ino) {
            node.data.extend_from_slice(bytes);
            node.modified = now;
        }
    }

    /// Truncates the file at `path` in-place to `new_len` bytes (modeling `copytruncate`).
    pub fn truncate_file(&self, path: impl AsRef<Path>, new_len: usize) {
        let mut inner = self.inner.lock().unwrap();
        let now = inner.clock.now_utc();
        if let Some(ino) = resolve_ino(&inner, path.as_ref())
            && let Some(node) = inner.inodes.get_mut(&ino)
        {
            node.data.resize(new_len, 0);
            node.modified = now;
        }
    }

    /// Renames `from` to `to` (atomic directory entry move; open handles keep pointing to the same inode).
    pub fn rename_file(&self, from: impl AsRef<Path>, to: impl Into<PathBuf>) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let ino = inner
            .entries
            .remove(from.as_ref())
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "source not found"))?;
        let to = to.into();
        inner.symlinks.remove(&to);
        if let Some(replaced_ino) = inner.entries.insert(to, ino) {
            unlink_inode(&mut inner, replaced_ino);
        }
        Ok(())
    }

    /// Unlinks `path` from the directory or symlink table. If handles are still open,
    /// the inode stays alive until the last handle is dropped (Unix semantics).
    pub fn unlink_file(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.symlinks.remove(path.as_ref()).is_some() {
            return Ok(());
        }
        let ino = inner
            .entries
            .remove(path.as_ref())
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "file not found"))?;
        unlink_inode(&mut inner, ino);
        Ok(())
    }

    /// Returns whether `path` currently exists in the directory or symlink table.
    #[must_use]
    pub fn exists(&self, path: impl AsRef<Path>) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.entries.contains_key(path.as_ref()) || inner.symlinks.contains_key(path.as_ref())
    }

    /// Sets the `modified` timestamp on the file at `path`.
    pub fn set_modified(&self, path: impl AsRef<Path>, modified: DateTime<Utc>) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ino) = resolve_ino(&inner, path.as_ref())
            && let Some(node) = inner.inodes.get_mut(&ino)
        {
            node.modified = modified;
        }
    }

    /// Sets the `created` timestamp on the file at `path`.
    pub fn set_created(&self, path: impl AsRef<Path>, created: DateTime<Utc>) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ino) = resolve_ino(&inner, path.as_ref())
            && let Some(node) = inner.inodes.get_mut(&ino)
        {
            node.created = created;
        }
    }

    /// Toggles read permission on `path` (for `PermissionDenied` fault injection).
    pub fn set_readable(&self, path: impl AsRef<Path>, readable: bool) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ino) = resolve_ino(&inner, path.as_ref())
            && let Some(node) = inner.inodes.get_mut(&ino)
        {
            node.readable = readable;
        }
    }

    /// Creates a [`SimPathsProvider`] matching the given include/exclude glob patterns.
    #[must_use]
    pub fn paths_provider(&self, include: &[&str], exclude: &[&str]) -> SimPathsProvider {
        SimPathsProvider {
            fs: self.clone(),
            include: include
                .iter()
                .map(|p| Pattern::new(p).expect("valid include pattern"))
                .collect(),
            exclude: exclude
                .iter()
                .map(|p| Pattern::new(p).expect("valid exclude pattern"))
                .collect(),
        }
    }
}

fn push_event(inner: &mut SimFsInner, event: FsEvent) {
    inner.event_counter = inner.event_counter.wrapping_add(1);
    match inner.event_delivery {
        EventDelivery::Normal => inner.fs_events.push(event),
        EventDelivery::DropAll => {}
        EventDelivery::Flaky { drop_every_n } => {
            if drop_every_n == 0 || !inner.event_counter.is_multiple_of(drop_every_n) {
                inner.fs_events.push(event);
            }
        }
    }
}

/// Simulated [`FsEventSource`] connected to a [`SimFileSystem`].
pub struct SimFsEventSource {
    fs: SimFileSystem,
}

impl FsEventSource for SimFsEventSource {
    fn drain_events(&mut self, out: &mut Vec<FsEvent>) {
        let mut inner = self.fs.inner.lock().unwrap();
        out.append(&mut inner.fs_events);
    }
}

fn allocate_ino(inner: &mut SimFsInner) -> u64 {
    if inner.inode_reuse
        && let Some(reused) = inner.freed_inos.pop()
    {
        return reused;
    }
    let ino = inner.next_ino;
    inner.next_ino += 1;
    ino
}

fn resolve_ino(inner: &SimFsInner, path: &Path) -> Option<u64> {
    let mut current = path.to_path_buf();
    for _ in 0..16 {
        if let Some(&ino) = inner.entries.get(&current) {
            return Some(ino);
        }
        let target = inner.symlinks.get(&current)?;
        current.clone_from(target);
    }
    None
}

fn unlink_inode(inner: &mut SimFsInner, ino: u64) {
    let should_remove = if let Some(node) = inner.inodes.get_mut(&ino) {
        node.link_count = node.link_count.saturating_sub(1);
        node.link_count == 0 && node.open_handles == 0
    } else {
        false
    };
    if should_remove {
        inner.inodes.remove(&ino);
        if inner.inode_reuse {
            inner.freed_inos.push(ino);
        }
    }
}

/// Deterministic [`PathsProvider`] backed by [`SimFileSystem`].
pub struct SimPathsProvider {
    fs: SimFileSystem,
    include: Vec<Pattern>,
    exclude: Vec<Pattern>,
}

impl PathsProvider for SimPathsProvider {
    type IntoIter = Vec<PathBuf>;

    fn paths(&self) -> Self::IntoIter {
        let mut inner = self.fs.inner.lock().unwrap();
        inner.stats.globs += 1;
        inner
            .entries
            .keys()
            .chain(inner.symlinks.keys())
            .filter(|path| {
                let s = path.to_string_lossy();
                self.include.iter().any(|p| p.matches(&s))
                    && !self.exclude.iter().any(|p| p.matches(&s))
            })
            .cloned()
            .collect()
    }
}

/// Open handle into a [`SimFileSystem`] inode.
pub struct SimFile {
    fs: SimFileSystem,
    ino: u64,
    cursor: u64,
}

impl Drop for SimFile {
    fn drop(&mut self) {
        let mut inner = self.fs.inner.lock().unwrap();
        let should_remove = if let Some(node) = inner.inodes.get_mut(&self.ino) {
            node.open_handles = node.open_handles.saturating_sub(1);
            node.link_count == 0 && node.open_handles == 0
        } else {
            false
        };
        if should_remove {
            inner.inodes.remove(&self.ino);
            if inner.inode_reuse {
                inner.freed_inos.push(self.ino);
            }
        }
    }
}

impl AsyncRead for SimFile {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut inner = self.fs.inner.lock().unwrap();
        inner.stats.reads += 1;
        let max_per_read = inner.max_bytes_per_read.unwrap_or(usize::MAX);
        let Some(node) = inner.inodes.get(&self.ino) else {
            return Poll::Ready(Err(io::Error::new(ErrorKind::NotFound, "inode gone")));
        };
        let pos = usize::try_from(self.cursor).unwrap_or(usize::MAX);
        if pos >= node.data.len() {
            inner.stats.eof_reads += 1;
            return Poll::Ready(Ok(()));
        }
        let available = &node.data[pos..];
        let to_copy = available.len().min(buf.remaining()).min(max_per_read);
        buf.put_slice(&available[..to_copy]);
        inner.stats.read_bytes += to_copy as u64;
        drop(inner);
        self.cursor += to_copy as u64;
        Poll::Ready(Ok(()))
    }
}

impl AsyncSeek for SimFile {
    fn start_seek(mut self: Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
        let mut inner = self.fs.inner.lock().unwrap();
        inner.stats.seeks += 1;
        let len = inner
            .inodes
            .get(&self.ino)
            .map_or(0, |n| n.data.len() as u64);
        drop(inner);
        let new_pos: i128 = match position {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(offset) => i128::from(len) + i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.cursor) + i128::from(offset),
        };
        if new_pos < 0 {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "invalid seek to negative offset",
            ));
        }
        self.cursor = u64::try_from(new_pos).unwrap_or(u64::MAX);
        Ok(())
    }

    fn poll_complete(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        Poll::Ready(Ok(self.cursor))
    }
}

impl AsyncFile for SimFile {
    fn metadata(&self) -> impl std::future::Future<Output = io::Result<FileMetadata>> + Send {
        let result = {
            let mut inner = self.fs.inner.lock().unwrap();
            inner.stats.stats += 1;
            inner
                .inodes
                .get(&self.ino)
                .map(|node| FileMetadata {
                    len: node.data.len() as u64,
                    is_dir: node.is_dir,
                    created: Some(node.created),
                    modified: Some(node.modified),
                })
                .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "inode gone"))
        };
        std::future::ready(result)
    }

    fn identity(&self) -> impl std::future::Future<Output = io::Result<FileIdentity>> + Send {
        let result = {
            let inner = self.fs.inner.lock().unwrap();
            inner
                .inodes
                .get(&self.ino)
                .map(|node| FileIdentity {
                    dev: node.dev,
                    ino: node.ino,
                })
                .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "inode gone"))
        };
        std::future::ready(result)
    }
}

impl FileSystem for SimFileSystem {
    type File = SimFile;

    fn open(
        &self,
        path: &Path,
    ) -> impl std::future::Future<Output = io::Result<Self::File>> + Send {
        let result = (|| {
            let mut inner = self.inner.lock().unwrap();
            inner.stats.opens += 1;
            let ino = resolve_ino(&inner, path)
                .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "file not found"))?;
            let node = inner
                .inodes
                .get_mut(&ino)
                .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "inode not found"))?;
            if !node.readable {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "permission denied",
                ));
            }
            node.open_handles += 1;
            Ok(SimFile {
                fs: self.clone(),
                ino,
                cursor: 0,
            })
        })();
        std::future::ready(result)
    }

    fn metadata(
        &self,
        path: &Path,
    ) -> impl std::future::Future<Output = io::Result<FileMetadata>> + Send {
        let result = (|| {
            let mut inner = self.inner.lock().unwrap();
            inner.stats.stats += 1;
            let ino = resolve_ino(&inner, path)
                .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "file not found"))?;
            let node = inner
                .inodes
                .get(&ino)
                .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "inode not found"))?;
            Ok(FileMetadata {
                len: node.data.len() as u64,
                is_dir: node.is_dir,
                created: Some(node.created),
                modified: Some(node.modified),
            })
        })();
        std::future::ready(result)
    }

    fn remove_file(&self, path: &Path) -> impl std::future::Future<Output = io::Result<()>> + Send {
        let result = (|| {
            let mut inner = self.inner.lock().unwrap();
            inner.stats.removes += 1;
            if inner.symlinks.remove(path).is_some() {
                return Ok(());
            }
            let &ino = inner
                .entries
                .get(path)
                .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "file not found"))?;
            if inner.strict_delete_while_open
                && inner
                    .inodes
                    .get(&ino)
                    .is_some_and(|node| node.open_handles > 0)
            {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "file is open by another handle",
                ));
            }
            inner.entries.remove(path);
            unlink_inode(&mut inner, ino);
            Ok(())
        })();
        std::future::ready(result)
    }

    fn rename(
        &self,
        from: &Path,
        to: &Path,
    ) -> impl std::future::Future<Output = io::Result<()>> + Send {
        let result = (|| {
            let mut inner = self.inner.lock().unwrap();
            inner.stats.renames += 1;
            let ino = inner
                .entries
                .remove(from)
                .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "source not found"))?;
            inner.symlinks.remove(to);
            if let Some(replaced_ino) = inner.entries.insert(to.to_path_buf(), ino) {
                unlink_inode(&mut inner, replaced_ino);
            }
            Ok(())
        })();
        std::future::ready(result)
    }

    fn write_file_sync(
        &self,
        path: &Path,
        data: Vec<u8>,
    ) -> impl std::future::Future<Output = io::Result<()>> + Send {
        let result = (|| {
            let mut inner = self.inner.lock().unwrap();
            inner.stats.writes += 1;
            if let Some(kind) = inner.next_write_error.take() {
                return Err(io::Error::new(kind, "injected write error"));
            }
            let crash_len = inner.next_partial_write_crash.take();
            let written = match crash_len {
                Some(n) => data[..n.min(data.len())].to_vec(),
                None => data,
            };
            let now = inner.clock.now_utc();
            let ino = allocate_ino(&mut inner);
            inner.symlinks.remove(path);
            if let Some(old_ino) = inner.entries.insert(path.to_path_buf(), ino) {
                unlink_inode(&mut inner, old_ino);
            }
            inner.inodes.insert(
                ino,
                SimInode {
                    dev: 1,
                    ino,
                    data: written,
                    created: now,
                    modified: now,
                    is_dir: false,
                    readable: true,
                    open_handles: 0,
                    link_count: 1,
                },
            );
            if crash_len.is_some() {
                return Err(io::Error::new(
                    ErrorKind::WriteZero,
                    "injected partial write crash",
                ));
            }
            Ok(())
        })();
        std::future::ready(result)
    }
}
