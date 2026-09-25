use std::{
    collections::VecDeque,
    io::{self, SeekFrom},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use tokio::{
    fs::{self, File},
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, BufReader},
};
use tracing::debug;
use vector_common::compression::gzip_multiple_decoder;
use vector_common::constants::GZIP_MAGIC;

use crate::{CheckpointsView, FilePosition, ReadFrom};
use file_source_common::FileFingerprint;
use file_source_common::PortableFileExt;
use file_source_common::{
    buffer::bounded::{BoundedLineReader, ReadOutcome},
    AsyncFileInfo,
};

/// Physical identity of an opened file, independent of its path and checkpoint fingerprint.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct FileIdentity {
    device: u64,
    inode: u64,
}

impl FileIdentity {
    pub(super) async fn at_path(path: &std::path::Path) -> io::Result<Self> {
        let file = File::open(path).await?;
        Ok(Self::from_file_info(&file.file_info().await?))
    }

    fn from_file_info(info: &impl PortableFileExt) -> Self {
        Self {
            device: info.portable_dev(),
            inode: info.portable_ino(),
        }
    }
}

/// Contiguous delivery progress for acknowledgements and automatic file deletion.
/// A new instance after truncation isolates acknowledgements for old contents.
#[derive(Debug)]
pub struct DeliveryProgress {
    state: Mutex<DeliveryState>,
}

#[derive(Debug)]
struct DeliveryState {
    offset: FilePosition,
    failed: bool,
    file_id: Option<FileFingerprint>,
    discarded: VecDeque<(FilePosition, FilePosition)>,
}

impl DeliveryState {
    fn advance(&mut self, offset: FilePosition) {
        self.offset = self.offset.max(offset);
        while self
            .discarded
            .front()
            .is_some_and(|(start, _)| *start <= self.offset)
        {
            self.offset = self.offset.max(self.discarded.pop_front().unwrap().1);
        }
    }
}

impl DeliveryProgress {
    fn new(offset: FilePosition) -> Self {
        Self {
            state: Mutex::new(DeliveryState {
                offset,
                failed: false,
                file_id: None,
                discarded: VecDeque::new(),
            }),
        }
    }

    /// Update the current checkpoint identity, including acknowledgements queued before a rekey.
    pub fn checkpoint(
        &self,
        checkpoints: &CheckpointsView,
        file_id: FileFingerprint,
        offset: FilePosition,
    ) {
        let mut state = self.state.lock().unwrap();
        if !state.failed {
            state.advance(offset);
            checkpoints.update(state.file_id.unwrap_or(file_id), state.offset);
        }
    }

    #[cfg(test)]
    fn delivered(&self, offset: FilePosition) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.failed {
            return false;
        }
        state.advance(offset);
        true
    }

    pub fn failed(&self) {
        self.state.lock().unwrap().failed = true;
    }

    pub(super) fn rekey(
        &self,
        old: FileFingerprint,
        new: FileFingerprint,
        checkpoints: &CheckpointsView,
    ) {
        let mut state = self.state.lock().unwrap();
        checkpoints.update_key(old, new);
        state.file_id = Some(new);
    }

    pub(super) fn discard(
        &self,
        start: FilePosition,
        end: FilePosition,
        file_id: FileFingerprint,
        checkpoints: &CheckpointsView,
    ) {
        let mut state = self.state.lock().unwrap();
        if state.failed {
            return;
        }
        if let Some((_, previous_end)) = state
            .discarded
            .back_mut()
            .filter(|(_, previous_end)| *previous_end == start)
        {
            *previous_end = end;
        } else {
            state.discarded.push_back((start, end));
        }
        let offset = state.offset;
        state.advance(offset);
        if state.offset != offset {
            checkpoints.update(state.file_id.unwrap_or(file_id), state.offset);
        }
    }

    fn covers(&self, offset: FilePosition) -> bool {
        let state = self.state.lock().unwrap();
        !state.failed && state.offset == offset
    }
}

/// The `RawLine` struct is a thin wrapper around the bytes that have been read
/// in order to retain the context of where in the file they have been read from.
///
/// The offset field contains the byte offset of the beginning of the line within
/// the file that it was read from.
#[derive(Debug)]
pub struct RawLine {
    pub offset: FilePosition,
    pub bytes: Bytes,
}

/// The `FileWatcher` struct defines the state machine which reads
/// from a file path, transparently handling file rollovers as is common for logs.
///
/// Plain files retain their handle so writes remain readable after a rename.
/// Compressed files retain their decoder and handle until EOF so decompression
/// can continue across read batches.
///
/// The `FileWatcher` is expected to live for the lifetime of the file
/// path. `FileServer` is responsible for clearing away `FileWatchers` which no
/// longer exist.
pub struct FileWatcher {
    pub path: PathBuf,
    findable: bool,
    file_position: FilePosition,
    identity: FileIdentity,
    is_dead: bool,
    reached_eof: bool,
    last_read_success: Instant,
    idle_since: Option<Instant>,
    line_reader: BoundedLineReader,
    max_line_bytes: usize,
    line_delimiter: Bytes,
    buf: BytesMut,
    reader: FileReader,
    pub(super) delivery_progress: Option<Arc<DeliveryProgress>>,
    compressed: bool,
    deletion_allowed: bool,
    opened_length: u64,
    opened_modified: Option<SystemTime>,
    prefix_length: Option<usize>,
    generation_prefix: Bytes,
    record_start: FilePosition,
    pub(super) discarded: Option<(FilePosition, FilePosition)>,
}

enum FileReader {
    Plain(BufReader<File>),
    Gzip(Box<dyn AsyncBufRead + Send + Unpin>),
    Empty,
}

impl FileWatcher {
    /// Create a new `FileWatcher` without reading its contents.
    ///
    /// The input path will be used by `FileWatcher` to prime its state
    /// machine. A `FileWatcher` tracks _only one_ file. This function returns
    /// None if the path does not exist or is not readable by the current process.
    pub async fn new(
        path: PathBuf,
        read_from: ReadFrom,
        ignore_before: Option<DateTime<Utc>>,
        max_line_bytes: usize,
        line_delimiter: Bytes,
    ) -> Result<FileWatcher, io::Error> {
        let f = fs::File::open(&path).await?;
        let file_info = f.file_info().await?;
        let identity = FileIdentity::from_file_info(&file_info);
        #[cfg(unix)]
        let metadata = file_info;
        #[cfg(windows)]
        let metadata = f.metadata().await?;

        let mut reader = BufReader::new(f);

        let too_old = if let (Some(ignore_before), Ok(modified_time)) = (
            ignore_before,
            metadata.modified().map(DateTime::<Utc>::from),
        ) {
            modified_time < ignore_before
        } else {
            false
        };

        let gzipped = is_gzipped(&mut reader).await?;

        // Determine the actual position at which we should start reading
        let (reader, file_position): (FileReader, FilePosition) =
            match (gzipped, too_old, read_from) {
                (true, true, _) => {
                    debug!(
                        message = "Not reading gzipped file older than `ignore_older`.",
                        ?path,
                    );
                    (FileReader::Empty, 0)
                }
                (true, _, ReadFrom::Checkpoint(file_position)) => {
                    debug!(
                        message = "Not re-reading gzipped file with existing stored offset.",
                        ?path,
                        %file_position
                    );
                    (FileReader::Empty, file_position)
                }
                // TODO: This may become the default, leading us to stop reading gzipped files that
                // we were reading before. Should we merge this and the next branch to read
                // compressed file from the beginning even when `read_from = "end"` (implicitly via
                // default or explicitly via config)?
                (true, _, ReadFrom::End) => {
                    debug!(
                        message = "Can't read from the end of already-compressed file.",
                        ?path,
                    );
                    (FileReader::Empty, 0)
                }
                (true, false, ReadFrom::Beginning) => (
                    FileReader::Gzip(Box::new(BufReader::new(gzip_multiple_decoder(reader)))),
                    0,
                ),
                (false, true, _) => {
                    let pos = reader.seek(SeekFrom::End(0)).await.unwrap();
                    (FileReader::Plain(reader), pos)
                }
                (false, false, ReadFrom::Checkpoint(file_position)) => {
                    let pos = reader.seek(SeekFrom::Start(file_position)).await.unwrap();
                    (FileReader::Plain(reader), pos)
                }
                (false, false, ReadFrom::Beginning) => {
                    let pos = reader.seek(SeekFrom::Start(0)).await.unwrap();
                    (FileReader::Plain(reader), pos)
                }
                (false, false, ReadFrom::End) => {
                    let pos = reader.seek(SeekFrom::End(0)).await.unwrap();
                    (FileReader::Plain(reader), pos)
                }
            };

        let ts = metadata
            .modified()
            .ok()
            .and_then(|mtime| mtime.elapsed().ok())
            .and_then(|diff| Instant::now().checked_sub(diff))
            .unwrap_or_else(Instant::now);

        Ok(FileWatcher {
            path: path.clone(),
            findable: true,
            file_position,
            identity,
            is_dead: false,
            reached_eof: false,
            last_read_success: ts,
            idle_since: None,
            line_reader: BoundedLineReader::new(line_delimiter.clone(), max_line_bytes),
            max_line_bytes,
            line_delimiter,
            buf: BytesMut::new(),
            deletion_allowed: !matches!(reader, FileReader::Empty),
            reader,
            delivery_progress: None,
            compressed: gzipped,
            opened_length: metadata.len(),
            opened_modified: metadata.modified().ok(),
            prefix_length: None,
            generation_prefix: Bytes::new(),
            record_start: file_position,
            discarded: None,
        })
    }

    pub(super) fn enable_delivery_tracking(&mut self) {
        if self.delivery_progress.is_none() {
            self.delivery_progress = Some(Arc::new(DeliveryProgress::new(self.file_position)));
        }
    }

    pub(super) async fn capture_generation_prefix(
        &mut self,
        length: Option<usize>,
    ) -> io::Result<()> {
        self.prefix_length = length;
        if let (Some(length), FileReader::Plain(reader)) = (length, &mut self.reader) {
            reader.seek(SeekFrom::Start(0)).await?;
            let mut prefix = Vec::new();
            let read = reader.take(length as u64).read_to_end(&mut prefix).await;
            reader.seek(SeekFrom::Start(self.file_position)).await?;
            read?;
            self.generation_prefix = Bytes::from(prefix);
        }
        Ok(())
    }

    pub(super) fn same_generation(&self, other: &Self) -> bool {
        self.prefix_length.is_some()
            && (!self.generation_prefix.is_empty() || self.file_position == 0)
            && !self.compressed
            && other.generation_prefix.starts_with(&self.generation_prefix)
    }

    pub(super) fn matches_identity(&self, identity: &FileIdentity) -> bool {
        self.identity == *identity
    }

    pub(super) fn finish_partial(&mut self) -> Option<RawLine> {
        match self.line_reader.finish(&mut self.buf) {
            ReadOutcome::Line => Some(RawLine {
                offset: self.record_start,
                bytes: self.buf.split().freeze(),
            }),
            ReadOutcome::Discarded => {
                self.discarded = Some((self.record_start, self.file_position));
                None
            }
            _ => None,
        }
    }

    pub(super) async fn ready_to_delete(&mut self, grace: Duration) -> io::Result<bool> {
        if !self.deletion_allowed
            || !self.reached_eof
            || !self.buf.is_empty()
            || self.last_read_success.elapsed() < grace
            || !self
                .delivery_progress
                .as_ref()
                .is_some_and(|p| p.covers(self.file_position))
        {
            return Ok(false);
        }
        // Reopen the path to ensure deletion still targets the inode we read.
        // The final identity check and unlink cannot be atomic against external renames.
        let file = File::open(&self.path).await?;
        let info = file.file_info().await?;
        if FileIdentity::from_file_info(&info) != self.identity {
            return Ok(false);
        }
        let metadata = file.metadata().await?;
        let modified = metadata.modified()?;
        if modified.elapsed().map_or(true, |age| age < grace) {
            return Ok(false);
        }
        Ok(if self.compressed {
            // Compressed byte lengths cannot be compared with decoded offsets.
            metadata.len() == self.opened_length && Some(modified) == self.opened_modified
        } else {
            metadata.len() == self.file_position
        })
    }

    pub async fn update_path(&mut self, path: PathBuf) -> io::Result<()> {
        let file_handle = File::open(&path).await?;
        let file_info = file_handle.file_info().await?;
        let identity = FileIdentity::from_file_info(&file_info);
        if identity != self.identity {
            // Carrying an offset to another inode does not establish that its
            // skipped prefix was delivered. Do not authorize deleting that file.
            self.deletion_allowed = false;
            if matches!(self.reader, FileReader::Plain(_)) {
                let mut reader = BufReader::new(file_handle);
                reader.seek(SeekFrom::Start(self.file_position)).await?;
                self.reader = FileReader::Plain(reader);
            }
            self.identity = identity;
        }
        self.path = path;
        Ok(())
    }

    pub(super) fn same_file(&self, other: &Self) -> bool {
        self.identity == other.identity
    }

    pub fn set_file_findable(&mut self, f: bool) {
        self.findable = f;
        if f {
            self.idle_since = None;
        }
    }

    pub fn file_findable(&self) -> bool {
        self.findable
    }

    pub fn set_dead(&mut self) {
        self.is_dead = true;
    }

    pub fn dead(&self) -> bool {
        self.is_dead
    }

    pub fn get_file_position(&self) -> FilePosition {
        self.file_position
    }

    pub fn reached_eof(&self) -> bool {
        self.reached_eof
    }

    /// Check once before each bounded read batch, including when the reader has
    /// buffered data. Seeking after a shrink discards that stale buffered data.
    pub(super) async fn check_for_truncation(&mut self) -> io::Result<()> {
        self.reached_eof = false;
        let shrunk = if let FileReader::Plain(reader) = &mut self.reader {
            reader.get_ref().metadata().await?.len() < self.file_position
        } else {
            false
        };
        if shrunk {
            self.rewind().await?;
        } else if self
            .prefix_length
            .is_some_and(|length| self.generation_prefix.len() < length)
        {
            // A tracked file can be smaller than its fingerprint after truncation.
            // Record its new prefix before reading, so rediscovery can distinguish
            // subsequent growth from another rewrite of the same inode.
            let previous = self.generation_prefix.clone();
            self.capture_generation_prefix(self.prefix_length).await?;
            if !self.generation_prefix.starts_with(&previous) {
                self.rewind().await?;
            }
        }
        Ok(())
    }

    pub(super) async fn rewind(&mut self) -> io::Result<()> {
        if let FileReader::Plain(reader) = &mut self.reader {
            reader.seek(SeekFrom::Start(0)).await?;
        }
        self.file_position = 0;
        self.record_start = 0;
        self.buf.clear();
        self.line_reader.reset();
        self.discarded = None;
        if let Some(progress) = &self.delivery_progress {
            progress.failed();
            self.delivery_progress = Some(Arc::new(DeliveryProgress::new(0)));
        }
        self.generation_prefix = Bytes::new();
        self.capture_generation_prefix(self.prefix_length).await
    }

    /// Read a single line from the underlying file
    ///
    /// This function will attempt to read a new line from its file, blocking,
    /// up to some maximum but unspecified amount of time.
    #[cfg(test)]
    async fn read_line(&mut self) -> io::Result<Option<RawLine>> {
        loop {
            let line = self.read_line_bounded(usize::MAX).await?;
            if self.discarded.take().is_none() {
                return Ok(line);
            }
        }
    }

    pub(super) async fn read_line_bounded(&mut self, budget: usize) -> io::Result<Option<RawLine>> {
        self.reached_eof = false;
        if self.is_dead {
            return Ok(None);
        }

        let reader: &mut (dyn AsyncBufRead + Send + Unpin) = match &mut self.reader {
            FileReader::Plain(reader) => reader,
            FileReader::Gzip(reader) => reader.as_mut(),
            FileReader::Empty => {
                self.reached_eof = true;
                return Ok(None);
            }
        };
        // Preserve whole-record reads for valid lines even with a tiny turn
        // budget. Skipping malformed input remains bounded by one maximum-size
        // record or the remaining turn budget, whichever is larger.
        let budget = budget.max(
            self.max_line_bytes
                .saturating_add(self.line_delimiter.len()),
        );
        let initial_position = self.file_position;
        let result = self
            .line_reader
            .read(reader, &mut self.file_position, &mut self.buf, budget)
            .await;
        if self.file_position != initial_position {
            // Partial and discarded records are activity too.
            self.idle_since = None;
        }
        match result {
            Ok(ReadOutcome::Line) => {
                self.reached_eof = false;
                self.record_start = self.file_position;
                self.track_read_success();
                let bytes = self.buf.split().freeze();
                // The call may finish a previously buffered record or skip
                // oversized records. Derive the start from the completed record.
                let offset =
                    self.file_position - bytes.len() as u64 - self.line_delimiter.len() as u64;

                debug!(
                    "read_line {}",
                    String::from_utf8_lossy(bytes::Buf::chunk(&bytes))
                );
                // Return all lines, including empty ones
                Ok(Some(RawLine { offset, bytes }))
            }
            Ok(ReadOutcome::Discarded) => {
                self.discarded = Some((self.record_start, self.file_position));
                self.record_start = self.file_position;
                self.track_read_success();
                Ok(None)
            }
            Ok(ReadOutcome::Yield) => Ok(None),
            Ok(ReadOutcome::Eof) => {
                if matches!(self.reader, FileReader::Gzip(_)) {
                    self.reader = FileReader::Empty;
                }
                // A renamed file can still receive writes through an open handle.
                // FileServer retires missing readers after an idle timeout at EOF.
                self.reached_eof = true;
                Ok(None)
            }

            Err(e) => {
                if let io::ErrorKind::NotFound = e.kind() {
                    self.set_dead();
                }
                Err(e)
            }
        }
    }

    #[inline]
    fn track_read_success(&mut self) {
        self.last_read_success = Instant::now();
    }

    #[inline]
    pub fn last_read_success(&self) -> Instant {
        self.last_read_success
    }

    #[inline]
    pub(super) fn should_retire(&mut self, timeout: Duration) -> bool {
        if self.findable || !self.reached_eof {
            self.idle_since = None;
            return false;
        }
        self.idle_since.get_or_insert_with(Instant::now).elapsed() >= timeout
    }
}

async fn is_gzipped(r: &mut BufReader<File>) -> io::Result<bool> {
    let header_bytes = r.fill_buf().await?;
    // WARN: The paired `BufReader::consume` is not called intentionally. If we
    // do we'll chop a decent part of the potential gzip stream off.
    Ok(header_bytes.starts_with(GZIP_MAGIC))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rekey_redirects_pending_acknowledgements_and_preserves_completed_progress() {
        let checkpoints = CheckpointsView::default();
        let old = FileFingerprint::FirstBytesChecksum(1);
        let new = FileFingerprint::FirstBytesChecksum(2);
        let progress = DeliveryProgress::new(0);
        progress.checkpoint(&checkpoints, old, 4);
        progress.rekey(old, new, &checkpoints);
        assert_eq!(checkpoints.get(old), None);
        assert_eq!(checkpoints.get(new), Some(4));
        // This acknowledgement was queued before the fingerprint changed.
        progress.checkpoint(&checkpoints, old, 8);
        assert_eq!(checkpoints.get(old), None);
        assert_eq!(checkpoints.get(new), Some(8));
        progress.failed();
        progress.checkpoint(&checkpoints, old, 12);
        assert_eq!(checkpoints.get(new), Some(8));
    }

    #[test]
    fn discarded_records_wait_for_preceding_delivery() {
        let checkpoints = CheckpointsView::default();
        let id = FileFingerprint::FirstBytesChecksum(1);
        let progress = DeliveryProgress::new(0);
        progress.discard(4, 100, id, &checkpoints);
        progress.discard(100, 200, id, &checkpoints);
        assert_eq!(checkpoints.get(id), None);
        assert!(!progress.covers(200));
        progress.checkpoint(&checkpoints, id, 4);
        assert_eq!(checkpoints.get(id), Some(200));
        assert!(progress.covers(200));
        let failed = DeliveryProgress::new(0);
        failed.discard(4, 200, id, &checkpoints);
        failed.failed();
        failed.checkpoint(&checkpoints, id, 4);
        assert!(!failed.covers(200));
    }

    #[tokio::test]
    async fn end_position_is_already_covered_for_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.log");
        fs::write(&path, "old record\n").await.unwrap();
        let mut watcher =
            FileWatcher::new(path, ReadFrom::End, None, 1024, Bytes::from_static(b"\n"))
                .await
                .unwrap();
        watcher.enable_delivery_tracking();
        assert!(watcher.read_line().await.unwrap().is_none());
        assert!(watcher.ready_to_delete(Duration::ZERO).await.unwrap());
    }

    #[tokio::test]
    async fn generation_prefix_distinguishes_rewrite_from_post_truncation_growth() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.log");
        fs::write(&path, "old\n").await.unwrap();
        let mut old = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
        )
        .await
        .unwrap();
        old.capture_generation_prefix(Some(4)).await.unwrap();
        assert!(old.read_line().await.unwrap().is_some());
        fs::write(&path, "new contents\n").await.unwrap();
        let mut new = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
        )
        .await
        .unwrap();
        new.capture_generation_prefix(Some(4)).await.unwrap();
        assert!(old.same_file(&new));
        assert!(
            !old.same_generation(&new),
            "regrowth past the old offset is still a rewrite"
        );

        fs::write(&path, "n\n").await.unwrap();
        old.check_for_truncation().await.unwrap();
        assert_eq!(old.read_line().await.unwrap().unwrap().bytes, "n");
        fs::write(&path, "n\nnext\n").await.unwrap();
        let mut grown = FileWatcher::new(
            path,
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
        )
        .await
        .unwrap();
        grown.capture_generation_prefix(Some(4)).await.unwrap();
        assert!(
            old.same_generation(&grown),
            "already-read replacement must not replay"
        );
    }

    #[tokio::test]
    async fn retirement_flushes_partial_delimiter_and_discards_oversized_tail() {
        for (input, expected) in [("ok\r", Some("ok\r")), ("abc\r", None), ("oversized", None)] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("input.log");
            fs::write(&path, input).await.unwrap();
            let mut watcher = FileWatcher::new(
                path,
                ReadFrom::Beginning,
                None,
                3,
                Bytes::from_static(b"\r\n"),
            )
            .await
            .unwrap();
            assert!(watcher.read_line().await.unwrap().is_none());
            let line = watcher.finish_partial();
            assert_eq!(
                line.as_ref()
                    .map(|line| std::str::from_utf8(&line.bytes).unwrap()),
                expected
            );
            if let Some(line) = line {
                assert_eq!(line.offset, 0);
            } else {
                assert_eq!(watcher.discarded, Some((0, input.len() as u64)));
            }
        }
    }

    #[tokio::test]
    async fn idle_retirement_requires_missing_eof_and_resets_on_partial_bytes() {
        use tokio::io::AsyncWriteExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.log");
        fs::write(&path, "first\n").await.unwrap();
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
        )
        .await
        .unwrap();
        assert!(watcher.read_line().await.unwrap().is_some());
        watcher.set_file_findable(false);
        assert!(
            !watcher.should_retire(Duration::ZERO),
            "must establish EOF first"
        );
        assert!(watcher.read_line().await.unwrap().is_none());
        let timeout = Duration::from_secs(1);
        assert!(!watcher.should_retire(timeout));
        watcher.idle_since = Some(Instant::now() - Duration::from_secs(2));
        assert!(watcher.should_retire(timeout));

        // An unterminated append is activity even though it produces no event.
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .await
            .unwrap();
        file.write_all(b"partial").await.unwrap();
        file.sync_all().await.unwrap();
        assert!(watcher.read_line().await.unwrap().is_none());
        assert!(!watcher.should_retire(timeout));
        watcher.idle_since = Some(Instant::now() - Duration::from_secs(2));
        watcher.set_file_findable(true);
        assert!(
            !watcher.should_retire(Duration::ZERO),
            "discoverable files stay open"
        );
        watcher.set_file_findable(false);
        assert!(
            !watcher.should_retire(timeout),
            "rediscovery resets retirement"
        );
    }

    #[tokio::test]
    async fn offsets_follow_partial_and_discarded_records() {
        use tokio::io::AsyncWriteExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.log");
        for delimiter in ["\n", "\r\n"] {
            fs::write(&path, "abc").await.unwrap();
            let mut watcher = FileWatcher::new(
                path.clone(),
                ReadFrom::Beginning,
                None,
                6,
                Bytes::from(delimiter),
            )
            .await
            .unwrap();
            assert!(watcher.read_line().await.unwrap().is_none());
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .await
                .unwrap();
            file.write_all(format!("def{delimiter}oversized{delimiter}ok{delimiter}").as_bytes())
                .await
                .unwrap();
            file.sync_all().await.unwrap();
            let first = watcher.read_line().await.unwrap().unwrap();
            assert_eq!(first.bytes, "abcdef");
            assert_eq!(first.offset, 0);
            let second = watcher.read_line().await.unwrap().unwrap();
            assert_eq!(second.bytes, "ok");
            assert_eq!(
                second.offset,
                (6 + delimiter.len() + 9 + delimiter.len()) as u64
            );
        }
    }

    #[tokio::test]
    async fn deletion_requires_delivery_and_rechecks_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.log");
        fs::write(&path, "one\ntwo\n").await.unwrap();
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
        )
        .await
        .unwrap();
        watcher.enable_delivery_tracking();
        while watcher.read_line().await.unwrap().is_some() {}
        assert!(!watcher.ready_to_delete(Duration::ZERO).await.unwrap());
        let progress = watcher.delivery_progress.as_ref().unwrap().clone();
        progress.delivered(8);
        assert!(watcher.ready_to_delete(Duration::ZERO).await.unwrap());
        assert!(!watcher
            .ready_to_delete(Duration::from_secs(60))
            .await
            .unwrap());

        // Data appended after EOF must invalidate the deletion decision.
        use tokio::io::AsyncWriteExt;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .await
            .unwrap();
        file.write_all(b"three\n").await.unwrap();
        file.sync_all().await.unwrap();
        assert!(!watcher.ready_to_delete(Duration::ZERO).await.unwrap());
        while watcher.read_line().await.unwrap().is_some() {}
        progress.delivered(14);
        assert!(watcher.ready_to_delete(Duration::ZERO).await.unwrap());

        fs::rename(&path, dir.path().join("rotated.log"))
            .await
            .unwrap();
        fs::write(&path, "one\ntwo\nthree\n").await.unwrap();
        assert!(
            !watcher.ready_to_delete(Duration::ZERO).await.unwrap(),
            "replacement inode"
        );
        fs::remove_file(&path).await.unwrap();
        fs::rename(dir.path().join("rotated.log"), &path)
            .await
            .unwrap();
        progress.failed();
        progress.delivered(14);
        assert!(
            !watcher.ready_to_delete(Duration::ZERO).await.unwrap(),
            "failed earlier record"
        );
    }

    #[tokio::test]
    async fn truncation_isolates_delivery_acknowledgements() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.log");
        fs::write(&path, "old contents\n").await.unwrap();
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
        )
        .await
        .unwrap();
        watcher.enable_delivery_tracking();
        while watcher.read_line().await.unwrap().is_some() {}
        let old_progress = watcher.delivery_progress.as_ref().unwrap().clone();
        fs::write(&path, "new\n").await.unwrap();
        watcher.check_for_truncation().await.unwrap();
        while watcher.read_line().await.unwrap().is_some() {}
        assert!(!old_progress.delivered(4));
        assert!(!watcher.ready_to_delete(Duration::ZERO).await.unwrap());
        watcher.delivery_progress.as_ref().unwrap().delivered(4);
        assert!(watcher.ready_to_delete(Duration::ZERO).await.unwrap());
    }

    #[tokio::test]
    async fn truncation_discards_buffered_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.log");
        fs::write(&path, "old line\n".repeat(128)).await.unwrap();
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
        )
        .await
        .unwrap();
        watcher.check_for_truncation().await.unwrap();
        for _ in 0..16 {
            assert_eq!(
                watcher.read_line().await.unwrap().unwrap().bytes,
                "old line"
            );
        }
        let FileReader::Plain(reader) = &watcher.reader else {
            panic!("expected plain reader")
        };
        assert!(!reader.buffer().is_empty());
        fs::write(&path, "new\n").await.unwrap();
        watcher.check_for_truncation().await.unwrap();
        let line = watcher.read_line().await.unwrap().unwrap();
        assert_eq!(line.offset, 0);
        assert_eq!(line.bytes, "new");
        assert!(watcher.read_line().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn truncation_resets_oversized_record_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.log");
        fs::write(&path, "x".repeat(128)).await.unwrap();
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            8,
            Bytes::from_static(b"\n"),
        )
        .await
        .unwrap();
        assert!(watcher.read_line_bounded(64).await.unwrap().is_none());
        assert!(!watcher.reached_eof());
        fs::write(&path, "new\n").await.unwrap();
        watcher.check_for_truncation().await.unwrap();
        let line = watcher.read_line_bounded(64).await.unwrap().unwrap();
        assert_eq!(line.bytes, "new");
        assert_eq!(line.offset, 0);
    }

    #[tokio::test]
    async fn truncation_discards_partial_line_at_eof() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.log");
        fs::write(&path, "old\nunfinished").await.unwrap();
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
        )
        .await
        .unwrap();
        watcher.check_for_truncation().await.unwrap();
        assert_eq!(watcher.read_line().await.unwrap().unwrap().bytes, "old");
        assert!(watcher.read_line().await.unwrap().is_none());
        assert!(!watcher.buf.is_empty());
        fs::write(&path, "new\n").await.unwrap();
        watcher.check_for_truncation().await.unwrap();
        let line = watcher.read_line().await.unwrap().unwrap();
        assert_eq!(line.offset, 0);
        assert_eq!(line.bytes, "new");
        assert!(watcher.read_line().await.unwrap().is_none());
    }
}
