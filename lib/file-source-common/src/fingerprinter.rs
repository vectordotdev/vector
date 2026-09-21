use std::{
    io::{ErrorKind, Result, SeekFrom},
    path::Path,
    time,
};

use crc::Crc;
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, File},
    io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeekExt, BufReader},
};
use vector_common::compression::gzip_multiple_decoder;
use vector_common::constants::GZIP_MAGIC;

use crate::{
    AsyncFileInfo, internal_events::FileSourceInternalEvents, metadata_ext::PortableFileExt,
};

const FINGERPRINT_CRC: Crc<u64> = Crc::<u64>::new(&crc::CRC_64_ECMA_182);

#[derive(Debug, Clone)]
pub struct Fingerprinter {
    strategy: FingerprintStrategy,
    max_line_length: usize,
    ignore_not_found: bool,
    buffer: Vec<u8>,
}

trait ResizeSlice<T> {
    /// Slice until [..`size`] and resize with default values if needed to avoid panics
    fn resize_slice_mut(&mut self, size: usize) -> &mut [T];
}

impl ResizeSlice<u8> for Vec<u8> {
    fn resize_slice_mut(&mut self, size: usize) -> &mut [u8] {
        if size > self.len() {
            self.resize_with(size, Default::default);
        }

        &mut self[..size]
    }
}

#[derive(Debug, Clone)]
pub enum FingerprintStrategy {
    FirstLinesChecksum {
        ignored_header_bytes: usize,
        lines: usize,
    },
    DevInode,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum FileFingerprint {
    #[serde(alias = "first_line_checksum")]
    FirstLinesChecksum(u64),
    DevInode(u64, u64),
}

#[derive(Debug, Copy, Clone)]
enum SupportedCompressionAlgorithms {
    Gzip,
}

impl SupportedCompressionAlgorithms {
    fn values() -> Vec<SupportedCompressionAlgorithms> {
        // Enumerate these from smallest magic_header_bytes to largest
        vec![SupportedCompressionAlgorithms::Gzip]
    }

    fn magic_header_bytes(&self) -> &'static [u8] {
        match self {
            SupportedCompressionAlgorithms::Gzip => GZIP_MAGIC,
        }
    }
}

trait UncompressedReader {
    async fn check(fp: &mut File) -> Result<Option<SupportedCompressionAlgorithms>>;
    async fn reader<'a>(fp: &'a mut File) -> Result<Box<dyn AsyncBufRead + Unpin + Send + 'a>>;
}

struct UncompressedReaderImpl;
impl UncompressedReader for UncompressedReaderImpl {
    /// Checks a file for supported compression algorithms by searching for
    /// supported magic header bytes.
    ///
    /// If an error occurs during reading, the file handler may become unusable,
    /// as the cursor position of the file may not be reset.
    ///
    /// # Arguments
    /// - `fp`: A mutable reference to the file to check.
    ///
    /// # Returns
    /// - `Ok(Some(algorithm))` if a supported compression algorithm is detected.
    /// - `Ok(None)` if no supported compression algorithm is detected.
    /// - `Err(std::io::Error)` if an I/O error occurs.
    async fn check(fp: &mut File) -> Result<Option<SupportedCompressionAlgorithms>> {
        let mut algorithm: Option<SupportedCompressionAlgorithms> = None;
        for compression_algorithm in SupportedCompressionAlgorithms::values() {
            // magic headers for algorithms can be of different lengths, and using a buffer too long could exceed the length of the file
            // so instantiate and check the various sizes in monotonically increasing order
            let magic_header_bytes = compression_algorithm.magic_header_bytes();

            let mut magic = vec![0u8; magic_header_bytes.len()];

            fp.seek(SeekFrom::Start(0)).await?;
            let result = fp.read_exact(&mut magic).await;

            if let Err(err) = result {
                fp.seek(SeekFrom::Start(0)).await?;
                return Err(err);
            }

            if magic == magic_header_bytes {
                algorithm = Some(compression_algorithm);
                break;
            }
        }
        fp.seek(SeekFrom::Start(0)).await?;
        Ok(algorithm)
    }

    async fn reader<'a>(fp: &'a mut File) -> Result<Box<dyn AsyncBufRead + Unpin + Send + 'a>> {
        // To support new compression algorithms, add them below
        match Self::check(fp).await? {
            Some(SupportedCompressionAlgorithms::Gzip) => Ok(Box::new(BufReader::new(
                gzip_multiple_decoder(BufReader::new(fp)),
            ))),
            // No compression, or read the raw bytes
            None => Ok(Box::new(BufReader::new(fp))),
        }
    }
}

async fn skip_first_n_bytes<R: AsyncBufRead + Unpin + Send>(
    reader: &mut R,
    n: usize,
) -> Result<()> {
    // We cannot simply seek the file by n because the file may be compressed;
    // to skip the first n decompressed bytes, we decompress up to n and discard the output.
    let mut skipped_bytes = 0;
    while skipped_bytes < n {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            // Still inside the ignored header, so no prefix bytes were sampled at all.
            return Err(incomplete_prefix_error(0));
        }
        let bytes_to_skip = std::cmp::min(chunk.len(), n - skipped_bytes);
        reader.consume(bytes_to_skip);
        skipped_bytes += bytes_to_skip;
    }
    Ok(())
}

impl Fingerprinter {
    pub fn new(
        strategy: FingerprintStrategy,
        max_line_length: usize,
        ignore_not_found: bool,
    ) -> Fingerprinter {
        let buffer = vec![0u8; max_line_length];

        Fingerprinter {
            strategy,
            max_line_length,
            ignore_not_found,
            buffer,
        }
    }

    /// Returns the `FileFingerprint` of a file, depending on `Fingerprinter::strategy`.
    #[cfg(test)]
    pub(crate) async fn fingerprint(&mut self, path: &Path) -> Result<FileFingerprint> {
        self.fingerprint_observing_identity(path, &mut None, &mut None, PrefixWanted::Yes, false)
            .await
    }

    /// The `FileFingerprint` of a file, also reporting the identity of the handle it read.
    ///
    /// The identity is taken from the descriptor the read already holds, so it costs no extra open
    /// and describes the file the result actually came from -- re-stat-ing the path afterwards could
    /// name a replacement. `identity` is left untouched when the file could not be opened.
    async fn fingerprint_observing_identity(
        &mut self,
        path: &Path,
        identity: &mut Option<(u64, u64)>,
        prefix: &mut Option<PartialPrefix>,
        want_prefix: PrefixWanted,
        capture_identity: bool,
    ) -> Result<FileFingerprint> {
        use FileFingerprint::*;

        match self.strategy {
            FingerprintStrategy::DevInode => {
                let file_handle = open_regular_file(path).await?;
                let file_info = file_handle.file_info().await?;
                let dev = file_info.portable_dev();
                let ino = file_info.portable_ino();
                *identity = Some((dev, ino));
                Ok(DevInode(dev, ino))
            }
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes,
                lines,
            } => {
                let buffer = self.buffer.resize_slice_mut(self.max_line_length);
                let mut fp = open_regular_file(path).await?;

                // The compression probe is inside, not before: it reads the magic bytes, so it fails
                // on a file shorter than they are -- exactly the short file whose identity the
                // caller needs. Leaving it outside skipped the capture below for that case.
                let read = async {
                    let mut reader = UncompressedReaderImpl::reader(&mut fp).await?;
                    skip_first_n_bytes(&mut reader, ignored_header_bytes).await?;
                    fingerprinter_read_until(reader, b'\n', lines, buffer).await
                }
                .await;
                match read {
                    Ok(bytes_read) => {
                        if capture_identity && let Ok(file_info) = fp.file_info().await {
                            *identity = Some((file_info.portable_dev(), file_info.portable_ino()));
                        }
                        // The same bytes the checksum is taken over. A rewrite that has *grown* into
                        // a complete fingerprint still begins with the partial prefix seen while it
                        // was short, which is what tells it from a different rewrite that completed.
                        if want_prefix == PrefixWanted::Yes {
                            *prefix = Some(PartialPrefix(buffer[..bytes_read].into()));
                        }
                        Ok(FirstLinesChecksum(
                            FINGERPRINT_CRC.checksum(&buffer[..bytes_read]),
                        ))
                    }
                    Err(error) => {
                        // Only once the read has failed, from the handle it already holds: on Windows
                        // this duplicates the handle and issues a syscall, which must not be paid for
                        // every file on every reconciliation just to serve the rare short-file branch.
                        if let Ok(file_info) = fp.file_info().await {
                            *identity = Some((file_info.portable_dev(), file_info.portable_ino()));
                        }
                        Err(error)
                    }
                }
            }
        }
    }

    /// [`Self::fingerprint_or_emit`], but distinguishing *why* no fingerprint was produced.
    pub async fn fingerprint_or_emit_detailed(
        &mut self,
        path: &Path,
        known_small_files: &mut crate::KnownSmallFiles,
        emitter: &impl FileSourceInternalEvents,
        want_prefix: PrefixWanted,
    ) -> FingerprintOutcome {
        self.fingerprint_or_emit_detailed_inner(
            path,
            known_small_files,
            emitter,
            want_prefix,
            false,
        )
        .await
        .0
    }

    /// [`Self::fingerprint_or_emit_detailed`], also returning the identity of the descriptor used
    /// for fingerprinting when it can be obtained. This avoids opening the same candidate again
    /// just to compare it with a retired reader; callers should use it only when that lookup is
    /// needed, since Windows obtains the identity with an additional handle query.
    pub async fn fingerprint_or_emit_detailed_with_identity(
        &mut self,
        path: &Path,
        known_small_files: &mut crate::KnownSmallFiles,
        emitter: &impl FileSourceInternalEvents,
        want_prefix: PrefixWanted,
    ) -> (FingerprintOutcome, Option<(u64, u64)>) {
        self.fingerprint_or_emit_detailed_inner(path, known_small_files, emitter, want_prefix, true)
            .await
    }

    async fn fingerprint_or_emit_detailed_inner(
        &mut self,
        path: &Path,
        known_small_files: &mut crate::KnownSmallFiles,
        emitter: &impl FileSourceInternalEvents,
        want_prefix: PrefixWanted,
        capture_identity: bool,
    ) -> (FingerprintOutcome, Option<(u64, u64)>) {
        // If short files are present, capture the identity of each successful read from the file
        // handle already used for fingerprinting. It lets the successful path clear an incomplete
        // entry recorded through another spelling without canonicalizing every matched path.
        let capture_identity = capture_identity || !known_small_files.is_empty();
        // Taken from the descriptor this function already opened, so no extra open is needed. The
        // probe is skipped on the common path and requested for pending short-file cleanup or the
        // retired-reader lookup.
        let mut stat_before = None;
        let mut read_identity = None;
        let mut read_prefix = None;
        let metadata = match fs::metadata(path).await {
            Ok(metadata) => {
                if metadata.is_file() {
                    stat_before = Some((metadata.len(), metadata.modified().ok()));
                    self.fingerprint_observing_identity(
                        path,
                        &mut read_identity,
                        &mut read_prefix,
                        want_prefix,
                        capture_identity,
                    )
                    .await
                    .map(Some)
                } else {
                    Ok(None)
                }
            }
            Err(e) => Err(e),
        };

        // One entry per *file*, not per spelling. The glob pass can yield a relative path while
        // notify reports an absolute one, and a glob over a symlinked directory can name a file the
        // backend reports by its canonical target -- all of which must collapse to one key, or
        // `remove_after` deletes a file that has become valid through a spelling nobody cleaned up.
        //
        // Canonicalizing resolves both, and is done once per call here rather than by sweeping the
        // whole map on every success: that sweep was O(N^2) in syscalls for a burst of N short files.
        // The lexical fallback covers a path that cannot be canonicalized (it may already be gone),
        // where an absolute spelling is still better than none.
        // Successful reads need no cleanup when there is no small-file state.
        match &metadata {
            Ok(Some(fingerprint)) if known_small_files.is_empty() => {
                return (
                    FingerprintOutcome::Fingerprinted(*fingerprint, read_prefix),
                    read_identity,
                );
            }
            Ok(None) if known_small_files.is_empty() => {
                return (FingerprintOutcome::Absent, read_identity);
            }
            _ => {}
        }

        if let Ok(Some(fingerprint)) = &metadata
            && let Some(read_identity) = read_identity
        {
            // A complete read through an alias makes the previous short-file record stale too.
            // The descriptor identity is authoritative for the bytes just fingerprinted, unlike a
            // second path lookup which could race with a replacement.
            known_small_files.remove_by_read_identity(read_identity);
            // The configured spelling may now name a different inode than the one previously
            // recorded there. Only then resolve its canonical key, to preserve the existing
            // replacement cleanup semantics without paying for unrelated successful paths.
            if known_small_files.contains_path(path) {
                let key = match fs::canonicalize(path).await {
                    Ok(canonical) => canonical,
                    Err(_) => crate::normalize_path_key(path),
                };
                known_small_files.remove(&key, path);
            }
            return (
                FingerprintOutcome::Fingerprinted(*fingerprint, read_prefix),
                Some(read_identity),
            );
        }

        let key = match fs::canonicalize(path).await {
            Ok(canonical) => canonical,
            Err(_) => crate::normalize_path_key(path),
        };

        match metadata {
            Ok(Some(fingerprint)) => {
                // Enough data to fingerprint: forget it, under both the identity it was recorded as
                // and this path -- the two can differ once a file has been replaced.
                known_small_files.remove(&key, path);
                (
                    FingerprintOutcome::Fingerprinted(fingerprint, read_prefix),
                    read_identity,
                )
            }
            // Not a regular file: a directory or device now occupies the path.
            Ok(None) => {
                known_small_files.remove(&key, path);
                (FingerprintOutcome::Absent, read_identity)
            }
            Err(error) => {
                let absent = error.kind() == ErrorKind::NotFound;
                match error.kind() {
                    ErrorKind::UnexpectedEof => {
                        // Best-effort: the fingerprint was read before `key` was resolved, so an
                        // atomic replacement in between would file the old inode's incomplete result
                        // under the replacement's identity -- and `remove_after` would then unlink a
                        // complete file. Comparing the stat taken before the read catches the
                        // replacements that change length or mtime; one that matches both within a
                        // timestamp tick still slips through, which would need the fingerprinting
                        // handle itself to close.
                        let still_the_same_file = fs::metadata(path)
                            .await
                            .is_ok_and(|now| stat_before == Some((now.len(), now.modified().ok())));
                        if still_the_same_file
                            // Recorded under its canonical identity, but removable by the path the
                            // configuration named: `remove_after` must not unlink a symlink's target.
                            && known_small_files.insert(
                                key,
                                path,
                                time::Instant::now(),
                                read_identity,
                            )
                        {
                            emitter.emit_file_checksum_failed(path);
                        }
                        // Read from the buffer the strategy just filled, so no second read of the
                        // file is needed.
                        //
                        // `None` when the EOF came from somewhere that did not report how much it
                        // had sampled -- a decompressor, say. That is *unknown*, not empty: an empty
                        // prefix is extended by every other prefix, so passing one off as a reading
                        // would silently answer "same rewrite" to every comparison.
                        let prefix = incomplete_prefix_len(&error)
                            .map(|len| PartialPrefix(self.buffer[..len].into()));
                        return (FingerprintOutcome::Incomplete(prefix), read_identity);
                    }
                    ErrorKind::NotFound => {
                        if !self.ignore_not_found {
                            emitter.emit_file_fingerprint_read_error(path, error);
                        }
                    }
                    _ => {
                        emitter.emit_file_fingerprint_read_error(path, error);
                    }
                };
                // For scenarios other than UnexpectedEOF, remove the path from the small files map.
                known_small_files.remove(&key, path);
                if absent {
                    (FingerprintOutcome::Absent, read_identity)
                } else {
                    (FingerprintOutcome::Failed, read_identity)
                }
            }
        }
    }

    /// Verify the rare replacement-open handoff against the descriptor used for fingerprinting.
    /// Ordinary reconciliation does not pay for this additional identity query.
    pub async fn fingerprint_matches_identity(
        &mut self,
        path: &Path,
        expected: Option<FileFingerprint>,
        identity: (u64, u64),
    ) -> bool {
        let mut observed_identity = None;
        let result = self
            .fingerprint_observing_identity(
                path,
                &mut observed_identity,
                &mut None,
                PrefixWanted::No,
                true,
            )
            .await;
        observed_identity == Some(identity)
            && match result {
                Ok(fingerprint) => Some(fingerprint) == expected,
                Err(error) => expected.is_none() && error.kind() == ErrorKind::UnexpectedEof,
            }
    }

    /// As [`Self::fingerprint_or_emit_detailed`], for callers that only need the fingerprint.
    pub async fn fingerprint_or_emit(
        &mut self,
        path: &Path,
        known_small_files: &mut crate::KnownSmallFiles,
        emitter: &impl FileSourceInternalEvents,
    ) -> Option<FileFingerprint> {
        self.fingerprint_or_emit_detailed(path, known_small_files, emitter, PrefixWanted::No)
            .await
            .fingerprint()
    }
}

/// Whether the caller will use the prefix of a *successful* fingerprint.
///
/// Copying the sampled bytes is wasted on the common path -- a stable file, fingerprinted every
/// reconciliation, whose prefix is dropped unread. Only a path already tracked by a watcher can
/// need one, to tell a rewrite still being written from a different one that completed.
///
/// An *incomplete* fingerprint always carries its prefix: that is the rewrite-in-progress case the
/// comparison exists for, and it is rare by nature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixWanted {
    /// Nothing tracks this path yet, so no comparison can be made against it.
    No,
    /// A watcher tracks this path and may need to compare what was read.
    Yes,
}

/// The bytes read from a file that was too short to fingerprint.
///
/// Bounded by `max_line_length`, since that is all the strategy ever reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialPrefix(std::sync::Arc<[u8]>);

impl PartialPrefix {
    /// Whether `self` could be this prefix still being written, rather than a new rewrite.
    ///
    /// A rewrite that is still in progress only ever *extends* what was seen before, so anything
    /// that is not an extension is different content. It cannot be conclusive: a second rewrite
    /// beginning with the same bytes -- a shared log header -- is indistinguishable from growth by
    /// content alone, which is why the caller also treats a shrink as a rewrite.
    pub fn continues(&self, earlier: &Self) -> bool {
        self.0.starts_with(&earlier.0)
    }
}

/// Why [`Fingerprinter::fingerprint_or_emit_detailed`] did or did not produce a fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FingerprintOutcome {
    Fingerprinted(FileFingerprint, Option<PartialPrefix>),
    /// Fewer complete lines than the strategy needs. For an already-tracked path this is the
    /// signature of an in-place rewrite, and the only case where rewinding a reader is correct.
    ///
    /// Carries the partial prefix that was read, which is what tells a *second* rewrite from the
    /// first one still being written: the same rewrite only ever extends its prefix.
    Incomplete(Option<PartialPrefix>),
    /// The path is gone, or no longer a regular file. The watcher on it must be left unfindable so
    /// the normal grace period reaps it.
    Absent,
    /// An I/O or decode error on a path that still exists -- the file may be readable and unchanged,
    /// so neither rewrite recovery nor reaping may be inferred.
    Failed,
}

impl FingerprintOutcome {
    pub fn fingerprint(&self) -> Option<FileFingerprint> {
        match self {
            Self::Fingerprinted(fingerprint, _) => Some(*fingerprint),
            Self::Incomplete(_) | Self::Absent | Self::Failed => None,
        }
    }

    pub fn is_incomplete(&self) -> bool {
        matches!(self, Self::Incomplete(_))
    }

    /// The partial prefix read from a file too short to fingerprint, which identifies *which*
    /// rewrite is in progress.
    pub fn partial_prefix(&self) -> Option<&PartialPrefix> {
        match self {
            Self::Incomplete(prefix) => prefix.as_ref(),
            Self::Fingerprinted(_, prefix) => prefix.as_ref(),
            Self::Absent | Self::Failed => None,
        }
    }

    /// Whether the path is gone or is no longer a regular file, so a watcher on it must stay
    /// unfindable rather than being kept alive on a dead inode.
    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }
}

/// Open a path without allowing a race to turn a regular-file read into a blocking FIFO read.
/// The metadata check in `fingerprint_or_emit` is only a fast path; the descriptor is checked too
/// because the path can change between the stat and the open.
async fn open_regular_file(path: &Path) -> Result<File> {
    #[cfg(unix)]
    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .await?;
    #[cfg(not(unix))]
    let file = File::open(path).await?;

    if !file.metadata().await?.is_file() {
        return Err(std::io::Error::new(
            ErrorKind::InvalidInput,
            "path is not a regular file",
        ));
    }

    Ok(file)
}

/// How many bytes of the partial prefix had been read when the file ran out.
///
/// Carried inside the error so the reader's signature stays `Result<usize>`: every other caller
/// treats `UnexpectedEof` as "too short" and is unaffected.
#[derive(Debug)]
struct IncompletePrefix(usize);

impl std::fmt::Display for IncompletePrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EOF reached after {} bytes", self.0)
    }
}

impl std::error::Error for IncompletePrefix {}

fn incomplete_prefix_error(read: usize) -> std::io::Error {
    std::io::Error::new(ErrorKind::UnexpectedEof, IncompletePrefix(read))
}

/// The prefix length recorded by [`incomplete_prefix_error`], if this is such an error.
fn incomplete_prefix_len(error: &std::io::Error) -> Option<usize> {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<IncompletePrefix>())
        .map(|prefix| prefix.0)
}

async fn fingerprinter_read_until(
    mut r: impl AsyncRead + Unpin + Send,
    delim: u8,
    mut count: usize,
    mut buf: &mut [u8],
) -> Result<usize> {
    let mut total_read = 0;
    'main: while !buf.is_empty() {
        let read = match r.read(buf).await {
            // `total_read` rides along on the error: the bytes already in the caller's buffer are
            // the rewrite's partial prefix, which is how a *further* rewrite is told from this one
            // still being written. Without it the caller cannot tell how much of the buffer is live.
            Ok(0) => return Err(incomplete_prefix_error(total_read)),
            Ok(n) => n,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        for (pos, &c) in buf[..read].iter().enumerate() {
            if c == delim {
                if count <= 1 {
                    total_read += pos + 1;
                    break 'main;
                } else {
                    count -= 1;
                }
            }
        }
        total_read += read;
        buf = &mut buf[read..];
    }
    Ok(total_read)
}

#[cfg(test)]
mod test {
    use std::{fs, io::Error, path::Path, time::Duration};

    use async_compression::tokio::bufread::GzipEncoder;
    use bytes::BytesMut;
    use tempfile::{TempDir, tempdir};

    use super::{FileSourceInternalEvents, FingerprintStrategy, Fingerprinter};

    use tokio::io::AsyncReadExt;

    /// Regression test for a bug found in review: the key is a canonical identity, and for a symlink
    /// that resolves to its *target*. If the target then disappears, canonicalization fails and the
    /// key falls back to the link's own path -- so one configured path ends up with two entries, both
    /// carrying it as `removal_path`, and `remove_after` could unlink a valid replacement appearing
    /// there. Verified against `fs::canonicalize`: key `target.log` before, `link.log` after.
    #[cfg(unix)]
    #[tokio::test]
    async fn stale_small_file_entry_is_dropped_when_the_key_changes() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("target.log");
        let link = dir.path().join("link.log");
        fs::write(&target, b"partial").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let mut fingerprinter = Fingerprinter::new(
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 1,
            },
            1024,
            true,
        );
        let mut known_small_files = crate::KnownSmallFiles::default();

        // Recorded under the target's canonical identity.
        assert!(
            fingerprinter
                .fingerprint_or_emit(&link, &mut known_small_files, &AllowsShortFiles)
                .await
                .is_none()
        );
        assert_eq!(known_small_files.len(), 1);
        assert!(
            known_small_files.contains_identity(&target.canonicalize().unwrap()),
            "test setup requires the key to be the canonical target: {known_small_files:?}"
        );

        // The link is retargeted at a *different* short file, so the key for the same configured
        // path changes while the entry stays relevant. (Removing the target instead yields
        // `NotFound`, which the existing error handling already cleans up.)
        let other = dir.path().join("other.log");
        fs::write(&other, b"short").unwrap();
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&other, &link).unwrap();
        assert!(
            fingerprinter
                .fingerprint_or_emit(&link, &mut known_small_files, &AllowsShortFiles)
                .await
                .is_none()
        );
        assert_eq!(
            known_small_files.len(),
            1,
            "one configured path must never hold two entries: {known_small_files:?}"
        );
    }

    /// Regression test for a bug found in review: keying by canonical identity made the map key the
    /// symlink's *target*, and `remove_after` unlinks the key -- so it would delete the target,
    /// a file outside the configured include path and possibly shared with something else. The
    /// configured path is carried alongside the identity for exactly this reason.
    #[cfg(unix)]
    #[tokio::test]
    async fn small_file_records_the_configured_path_for_removal() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("target.log");
        let link = dir.path().join("link.log");
        fs::write(&target, b"partial").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let mut fingerprinter = Fingerprinter::new(
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 1,
            },
            1024,
            true,
        );
        let mut known_small_files = crate::KnownSmallFiles::default();

        // Observed through the symlink, which is what the include pattern named.
        assert!(
            fingerprinter
                .fingerprint_or_emit(&link, &mut known_small_files, &AllowsShortFiles)
                .await
                .is_none()
        );

        let identity = target.canonicalize().unwrap();
        assert!(
            known_small_files.contains_identity(&identity),
            "the entry is keyed by canonical identity, so every spelling collapses onto one"
        );
        assert_eq!(
            known_small_files.removal_path(&identity),
            Some(link.as_path()),
            "removal must target the configured path, not the symlink's target"
        );
    }

    /// Regression test for a bug found in review: `known_small_files` keys unified only lexical
    /// spelling differences, so a glob including a symlinked path and a notify backend reporting the
    /// canonical target produced two entries for one file. A completed fingerprint removed only the
    /// entry it was called with, leaving the other to make `remove_after` delete a file that had
    /// since become valid.
    ///
    /// Keys are canonical identities now, so the aliases never diverge in the first place.
    #[cfg(unix)]
    #[tokio::test]
    async fn successful_fingerprint_clears_symlink_aliases() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("app.log");
        let link = dir.path().join("link.log");
        // Unterminated: this is what lands in `known_small_files`.
        fs::write(&target, b"partial").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let mut fingerprinter = Fingerprinter::new(
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 1,
            },
            1024,
            true,
        );
        let mut known_small_files = crate::KnownSmallFiles::default();

        // Both spellings fail to fingerprint, each recording its own entry.
        assert!(
            fingerprinter
                .fingerprint_or_emit(&link, &mut known_small_files, &AllowsShortFiles)
                .await
                .is_none()
        );
        assert!(
            fingerprinter
                .fingerprint_or_emit(&target, &mut known_small_files, &AllowsShortFiles)
                .await
                .is_none()
        );
        // The two spellings collapse onto one key rather than producing two entries: keying by
        // canonical identity prevents the divergence instead of sweeping it up afterwards.
        assert_eq!(
            known_small_files.len(),
            1,
            "aliasing spellings must share one entry: {known_small_files:?}"
        );

        // Completing the line makes the file valid; every alias must go, not just the one named.
        fs::write(&target, b"partial\n").unwrap();
        assert!(
            fingerprinter
                .fingerprint_or_emit(&target, &mut known_small_files, &AllowsShortFiles)
                .await
                .is_some()
        );
        assert!(
            known_small_files.is_empty(),
            "a stale alias would let remove_after delete a now-valid file: {known_small_files:?}"
        );
    }

    /// A different configured spelling can finish the same inode's short record. Its descriptor
    /// identity must clear the entry without resolving every successful path through the filesystem.
    #[cfg(unix)]
    #[tokio::test]
    async fn successful_fingerprint_through_an_unrecorded_alias_clears_small_file_state() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("app.log");
        let link = dir.path().join("link.log");
        fs::write(&target, b"partial").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let mut fingerprinter = Fingerprinter::new(
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 1,
            },
            1024,
            true,
        );
        let mut known_small_files = crate::KnownSmallFiles::default();
        assert!(
            fingerprinter
                .fingerprint_or_emit(&link, &mut known_small_files, &AllowsShortFiles)
                .await
                .is_none()
        );
        assert_eq!(known_small_files.len(), 1);

        fs::write(&target, b"partial\n").unwrap();
        assert!(
            fingerprinter
                .fingerprint_or_emit(&target, &mut known_small_files, &AllowsShortFiles)
                .await
                .is_some()
        );
        assert!(
            known_small_files.is_empty(),
            "a completed alias must not leave the configured symlink eligible for removal"
        );
    }

    #[tokio::test]
    async fn successful_replacement_clears_stale_small_file_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.log");
        let replacement = dir.path().join("replacement.log");
        fs::write(&path, b"partial").unwrap();

        let mut fingerprinter = Fingerprinter::new(
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 1,
            },
            1024,
            true,
        );
        let mut known_small_files = crate::KnownSmallFiles::default();
        assert!(
            fingerprinter
                .fingerprint_or_emit(&path, &mut known_small_files, &AllowsShortFiles)
                .await
                .is_none()
        );

        fs::write(&replacement, b"complete\n").unwrap();
        fs::remove_file(&path).unwrap();
        fs::rename(&replacement, &path).unwrap();
        assert!(
            fingerprinter
                .fingerprint_or_emit(&path, &mut known_small_files, &AllowsShortFiles)
                .await
                .is_some()
        );
        assert!(
            known_small_files.is_empty(),
            "a successful replacement must not leave stale path state that could unlink it"
        );
    }

    pub async fn gzip(data: &[u8]) -> Vec<u8> {
        let mut encoder = GzipEncoder::new(data);

        let mut out = Vec::new();
        encoder.read_to_end(&mut out).await.expect("Failed to read");
        out
    }
    fn read_byte_content(target_dir: &TempDir, file: &str) -> Vec<u8> {
        use std::{fs::File, io::Read};

        let path = target_dir.path().join(file);
        let mut file = File::open(path).unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();
        content
    }

    #[tokio::test]
    async fn test_checksum_fingerprint() {
        let mut fingerprinter = Fingerprinter::new(
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 1,
            },
            1024,
            false,
        );

        let target_dir = tempdir().unwrap();
        let mut full_line_data = vec![b'x'; 256];
        full_line_data.push(b'\n');
        let not_full_line_data = vec![b'x'; 199];
        let empty_path = target_dir.path().join("empty.log");
        let full_line_path = target_dir.path().join("full_line.log");
        let duplicate_path = target_dir.path().join("duplicate.log");
        let not_full_line_path = target_dir.path().join("not_full_line.log");
        fs::write(&empty_path, []).unwrap();
        fs::write(&full_line_path, &full_line_data).unwrap();
        fs::write(&duplicate_path, &full_line_data).unwrap();
        fs::write(&not_full_line_path, not_full_line_data).unwrap();

        assert!(fingerprinter.fingerprint(&empty_path).await.is_err());
        assert!(fingerprinter.fingerprint(&full_line_path).await.is_ok());
        assert!(
            fingerprinter
                .fingerprint(&not_full_line_path)
                .await
                .is_err()
        );
        assert_eq!(
            fingerprinter.fingerprint(&full_line_path).await.unwrap(),
            fingerprinter.fingerprint(&duplicate_path).await.unwrap(),
        );
    }

    #[tokio::test]
    async fn test_first_line_checksum_fingerprint() {
        let max_line_length = 64;
        let mut fingerprinter = Fingerprinter::new(
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 1,
            },
            max_line_length,
            false,
        );

        let target_dir = tempdir().unwrap();
        let prepare_test = |file: &str, contents: &[u8]| {
            let path = target_dir.path().join(file);
            fs::write(&path, contents).unwrap();
            path
        };
        let prepare_test_long = |file: &str, amount| {
            prepare_test(
                file,
                b"hello world "
                    .iter()
                    .cloned()
                    .cycle()
                    .clone()
                    .take(amount)
                    .collect::<Box<_>>()
                    .as_ref(),
            )
        };

        let empty = prepare_test("empty.log", b"");
        let incomplete_line = prepare_test("incomplete_line.log", b"missing newline char");
        let one_line = prepare_test(
            "one_line_duplicate_compressed.log",
            &gzip(b"hello world\n").await,
        );
        let one_line_duplicate = prepare_test("one_line_duplicate.log", b"hello world\n");
        let one_line_duplicate_compressed = prepare_test(
            "one_line_duplicate_compressed.log",
            &gzip(b"hello world\n").await,
        );
        let one_line_continued =
            prepare_test("one_line_continued.log", b"hello world\nthe next line\n");
        let one_line_continued_compressed = prepare_test(
            "one_line_continued_compressed.log",
            &gzip(b"hello world\nthe next line\n").await,
        );
        let different_two_lines = prepare_test("different_two_lines.log", b"line one\nline two\n");

        let exactly_max_line_length =
            prepare_test_long("exactly_max_line_length.log", max_line_length);
        let exceeding_max_line_length =
            prepare_test_long("exceeding_max_line_length.log", max_line_length + 1);
        let incomplete_under_max_line_length_by_one = prepare_test_long(
            "incomplete_under_max_line_length_by_one.log",
            max_line_length - 1,
        );

        let mut run = async |path| fingerprinter.fingerprint(path).await;

        assert!(run(&empty).await.is_err());
        assert!(run(&incomplete_line).await.is_err());
        assert!(run(&incomplete_under_max_line_length_by_one).await.is_err());

        assert!(run(&one_line).await.is_ok());
        assert!(run(&one_line_duplicate).await.is_ok());
        assert!(run(&one_line_continued).await.is_ok());
        assert!(run(&different_two_lines).await.is_ok());
        assert!(run(&exactly_max_line_length).await.is_ok());
        assert!(run(&exceeding_max_line_length).await.is_ok());

        assert_eq!(
            run(&one_line).await.unwrap(),
            run(&one_line_duplicate_compressed).await.unwrap()
        );
        assert_eq!(
            run(&one_line).await.unwrap(),
            run(&one_line_continued_compressed).await.unwrap()
        );
        assert_eq!(
            run(&one_line).await.unwrap(),
            run(&one_line_duplicate_compressed).await.unwrap()
        );
        assert_eq!(
            run(&one_line).await.unwrap(),
            run(&one_line_continued_compressed).await.unwrap()
        );

        assert_ne!(
            run(&one_line).await.unwrap(),
            run(&different_two_lines).await.unwrap()
        );

        assert_eq!(
            run(&exactly_max_line_length).await.unwrap(),
            run(&exceeding_max_line_length).await.unwrap()
        );

        assert_ne!(
            read_byte_content(&target_dir, "one_line_duplicate.log"),
            read_byte_content(&target_dir, "one_line_duplicate_compressed.log")
        );

        assert_ne!(
            read_byte_content(&target_dir, "one_line_continued.log"),
            read_byte_content(&target_dir, "one_line_continued_compressed.log")
        );
    }

    #[tokio::test]
    async fn test_first_two_lines_checksum_fingerprint() {
        let max_line_length = 64;
        let mut fingerprinter = Fingerprinter::new(
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 2,
            },
            max_line_length,
            false,
        );

        let target_dir = tempdir().unwrap();
        let prepare_test = |file: &str, contents: &[u8]| {
            let path = target_dir.path().join(file);
            fs::write(&path, contents).unwrap();
            path
        };

        let incomplete_lines = prepare_test(
            "incomplete_lines.log",
            b"missing newline char\non second line",
        );
        let two_lines = prepare_test("two_lines.log", b"hello world\nfrom vector\n");
        let two_lines_duplicate =
            prepare_test("two_lines_duplicate.log", b"hello world\nfrom vector\n");
        let two_lines_continued = prepare_test(
            "two_lines_continued.log",
            b"hello world\nfrom vector\nthe next line\n",
        );
        let two_lines_duplicate_compressed = prepare_test(
            "two_lines_duplicate_compressed.log",
            &gzip(b"hello world\nfrom vector\n").await,
        );
        let two_lines_continued_compressed = prepare_test(
            "two_lines_continued_compressed.log",
            &gzip(b"hello world\nfrom vector\nthe next line\n").await,
        );

        let different_three_lines = prepare_test(
            "different_three_lines.log",
            b"line one\nline two\nine three\n",
        );

        let mut run = async move |path| fingerprinter.fingerprint(path).await;

        assert!(run(&incomplete_lines).await.is_err());

        assert!(run(&two_lines).await.is_ok());
        assert!(run(&two_lines_duplicate).await.is_ok());
        assert!(run(&two_lines_continued).await.is_ok());
        assert!(run(&different_three_lines).await.is_ok());

        assert_eq!(
            run(&two_lines).await.unwrap(),
            run(&two_lines_duplicate).await.unwrap()
        );
        assert_eq!(
            run(&two_lines).await.unwrap(),
            run(&two_lines_continued).await.unwrap()
        );
        assert_eq!(
            run(&two_lines).await.unwrap(),
            run(&two_lines_duplicate_compressed).await.unwrap()
        );
        assert_eq!(
            run(&two_lines).await.unwrap(),
            run(&two_lines_continued_compressed).await.unwrap()
        );

        assert_ne!(
            run(&two_lines).await.unwrap(),
            run(&different_three_lines).await.unwrap()
        );

        assert_ne!(
            read_byte_content(&target_dir, "two_lines_duplicate.log"),
            read_byte_content(&target_dir, "two_lines_duplicate_compressed.log")
        );
        assert_ne!(
            read_byte_content(&target_dir, "two_lines_continued.log"),
            read_byte_content(&target_dir, "two_lines_continued_compressed.log")
        );
    }

    #[tokio::test]
    async fn test_first_two_lines_checksum_fingerprint_with_headers() {
        let max_line_length = 64;
        let mut fingerprinter = Fingerprinter::new(
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 14,
                lines: 2,
            },
            max_line_length,
            false,
        );

        let target_dir = tempdir().unwrap();
        let prepare_test = |file: &str, contents: &[u8]| {
            let path = target_dir.path().join(file);
            fs::write(&path, contents).unwrap();
            path
        };

        let two_lines = prepare_test(
            "two_lines.log",
            b"some-header-1\nhello world\nfrom vector\n",
        );
        let two_lines_compressed_same_header = prepare_test(
            "two_lines_compressed_same_header.log",
            &gzip(b"some-header-1\nhello world\nfrom vector\n").await,
        );
        let two_lines_compressed_same_header_size = prepare_test(
            "two_lines_compressed_same_header_size.log",
            &gzip(b"some-header-2\nhello world\nfrom vector\n").await,
        );
        let two_lines_compressed_different_header_size = prepare_test(
            "two_lines_compressed_different_header_size.log",
            &gzip(b"some-header-22\nhellow world\nfrom vector\n").await,
        );

        let mut run = async move |path| fingerprinter.fingerprint(path).await;

        assert!(run(&two_lines).await.is_ok());
        assert_eq!(
            run(&two_lines).await.unwrap(),
            run(&two_lines_compressed_same_header).await.unwrap()
        );
        assert_eq!(
            run(&two_lines).await.unwrap(),
            run(&two_lines_compressed_same_header_size).await.unwrap()
        );
        assert_ne!(
            run(&two_lines).await.unwrap(),
            run(&two_lines_compressed_different_header_size)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn short_file_with_ignored_header_returns_eof() {
        let target_dir = tempdir().unwrap();
        let path = target_dir.path().join("short.log");
        fs::write(&path, b"short").unwrap();
        let mut fingerprinter = Fingerprinter::new(
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 10,
                lines: 1,
            },
            1024,
            false,
        );

        let result = tokio::time::timeout(Duration::from_secs(1), fingerprinter.fingerprint(&path))
            .await
            .expect("short files must not make header skipping loop at EOF");
        assert_eq!(
            result.unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }

    #[tokio::test]
    async fn test_inode_fingerprint() {
        let mut fingerprinter = Fingerprinter::new(FingerprintStrategy::DevInode, 42, false);

        let target_dir = tempdir().unwrap();
        let small_data = vec![b'x'; 1];
        let medium_data = vec![b'x'; 256];
        let empty_path = target_dir.path().join("empty.log");
        let small_path = target_dir.path().join("small.log");
        let medium_path = target_dir.path().join("medium.log");
        let duplicate_path = target_dir.path().join("duplicate.log");
        fs::write(&empty_path, []).unwrap();
        fs::write(&small_path, small_data).unwrap();
        fs::write(&medium_path, &medium_data).unwrap();
        fs::write(&duplicate_path, &medium_data).unwrap();

        assert!(fingerprinter.fingerprint(&empty_path).await.is_ok());
        assert!(fingerprinter.fingerprint(&small_path).await.is_ok());
        assert_ne!(
            fingerprinter.fingerprint(&medium_path).await.unwrap(),
            fingerprinter.fingerprint(&duplicate_path).await.unwrap()
        );
    }

    #[tokio::test]
    async fn no_error_on_dir() {
        let target_dir = tempdir().unwrap();
        let mut fingerprinter = Fingerprinter::new(
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 1,
            },
            1024,
            false,
        );

        let mut small_files = crate::KnownSmallFiles::default();
        assert!(
            fingerprinter
                .fingerprint_or_emit(target_dir.path(), &mut small_files, &NoErrors)
                .await
                .is_none()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ignores_fifo_without_blocking() {
        let target_dir = tempdir().unwrap();
        let fifo = target_dir.path().join("events.log");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo must be available on Unix test systems");
        assert!(status.success(), "mkfifo failed with status {status}");

        let mut fingerprinter = Fingerprinter::new(
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 1,
            },
            1024,
            false,
        );
        let mut small_files = crate::KnownSmallFiles::default();

        let fingerprint = tokio::time::timeout(
            Duration::from_secs(1),
            fingerprinter.fingerprint_or_emit(&fifo, &mut small_files, &NoErrors),
        )
        .await
        .expect("fingerprinting a FIFO must not block waiting for a writer");

        assert!(fingerprint.is_none());

        let direct_result =
            tokio::time::timeout(Duration::from_secs(1), fingerprinter.fingerprint(&fifo))
                .await
                .expect("direct fingerprinting a FIFO must not wait for a writer");
        assert!(direct_result.is_err());
    }

    #[test]
    fn test_monotonic_compression_algorithms() {
        // This test is necessary to handle an edge case where when assessing the magic header
        // bytes of a file to determine the compression algorithm, it's possible that the length of
        // the file is smaller than the size of the magic header bytes it's being assessed against.
        // While this could be an indication that the file is simply too small, it could also
        // just be that the compression header is a smaller one than the assessed algorithm.
        // Checking this with a guarantee on the monotonically increasing order assures that this edge case doesn't happen.
        let algos = super::SupportedCompressionAlgorithms::values();
        let mut smallest_byte_length = 0;
        for algo in algos {
            let magic_header_bytes = algo.magic_header_bytes();
            assert!(smallest_byte_length <= magic_header_bytes.len());
            smallest_byte_length = magic_header_bytes.len();
        }
    }
    #[derive(Clone)]
    struct NoErrors;

    impl FileSourceInternalEvents for NoErrors {
        fn emit_file_added(&self, _: &Path) {}

        fn emit_file_resumed(&self, _: &Path, _: u64) {}

        fn emit_file_watch_error(&self, _: &Path, _: Error) {
            panic!();
        }

        fn emit_file_unwatched(&self, _: &Path, _: bool) {}

        fn emit_file_deleted(&self, _: &Path) {}

        fn emit_file_delete_error(&self, _: &Path, _: Error) {
            panic!();
        }

        fn emit_file_fingerprint_read_error(&self, _: &Path, _: Error) {
            panic!();
        }

        fn emit_file_checkpointed(&self, _: usize, _: Duration) {}

        fn emit_file_checksum_failed(&self, _: &Path) {
            panic!();
        }

        fn emit_file_checkpoint_write_error(&self, _: Error) {
            panic!();
        }

        fn emit_files_open(&self, _: usize) {}

        fn emit_files_idle(&self, _: usize) {}

        fn emit_path_globbing_failed(&self, _: &Path, _: &Error) {
            panic!()
        }

        fn emit_file_line_too_long(&self, _: &BytesMut, _: usize, _: usize) {
            panic!()
        }
    }

    /// Like `NoErrors`, but tolerates the checksum failure a deliberately short file produces.
    /// Used by tests that deliberately fingerprint an incomplete file.
    #[derive(Clone)]
    struct AllowsShortFiles;

    impl FileSourceInternalEvents for AllowsShortFiles {
        // The one event a short file legitimately produces.
        fn emit_file_checksum_failed(&self, _: &Path) {}

        fn emit_file_added(&self, path: &Path) {
            NoErrors.emit_file_added(path);
        }
        fn emit_file_resumed(&self, path: &Path, offset: u64) {
            NoErrors.emit_file_resumed(path, offset);
        }
        fn emit_file_watch_error(&self, path: &Path, error: Error) {
            NoErrors.emit_file_watch_error(path, error);
        }
        fn emit_file_unwatched(&self, path: &Path, reached_eof: bool) {
            NoErrors.emit_file_unwatched(path, reached_eof);
        }
        fn emit_file_deleted(&self, path: &Path) {
            NoErrors.emit_file_deleted(path);
        }
        fn emit_file_delete_error(&self, path: &Path, error: Error) {
            NoErrors.emit_file_delete_error(path, error);
        }
        fn emit_file_fingerprint_read_error(&self, path: &Path, error: Error) {
            NoErrors.emit_file_fingerprint_read_error(path, error);
        }
        fn emit_file_checkpointed(&self, count: usize, duration: Duration) {
            NoErrors.emit_file_checkpointed(count, duration);
        }
        fn emit_file_checkpoint_write_error(&self, error: Error) {
            NoErrors.emit_file_checkpoint_write_error(error);
        }
        fn emit_files_open(&self, count: usize) {
            NoErrors.emit_files_open(count);
        }
        fn emit_files_idle(&self, count: usize) {
            NoErrors.emit_files_idle(count);
        }
        fn emit_path_globbing_failed(&self, path: &Path, error: &Error) {
            NoErrors.emit_path_globbing_failed(path, error);
        }
        fn emit_file_line_too_long(&self, buf: &BytesMut, max: usize, size: usize) {
            NoErrors.emit_file_line_too_long(buf, max, size);
        }
    }
}
