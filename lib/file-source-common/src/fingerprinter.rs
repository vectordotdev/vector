use std::{
    collections::HashMap,
    io::{ErrorKind, Result, SeekFrom},
    path::{Path, PathBuf},
    time,
};

use async_compression::tokio::bufread::GzipDecoder;
use crc::Crc;
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, File},
    io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeekExt, BufReader},
};
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
                GzipDecoder::new(BufReader::new(fp)),
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

    /// Returns the `FileFingerprint` of a file, depending on `Fingerprinter::strategy`
    pub(crate) async fn fingerprint(&mut self, path: &Path) -> Result<FileFingerprint> {
        use FileFingerprint::*;

        match self.strategy {
            FingerprintStrategy::DevInode => {
                let file_handle = File::open(path).await?;
                let file_info = file_handle.file_info().await?;
                let dev = file_info.portable_dev();
                let ino = file_info.portable_ino();
                Ok(DevInode(dev, ino))
            }
            FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes,
                lines,
            } => {
                let buffer = self.buffer.resize_slice_mut(self.max_line_length);
                let mut fp = File::open(path).await?;
                let mut reader = UncompressedReaderImpl::reader(&mut fp).await?;

                skip_first_n_bytes(&mut reader, ignored_header_bytes).await?;
                let bytes_read = fingerprinter_read_until(reader, b'\n', lines, buffer).await?;
                let fingerprint = FINGERPRINT_CRC.checksum(&buffer[..bytes_read]);
                Ok(FirstLinesChecksum(fingerprint))
            }
        }
    }

    pub async fn fingerprint_or_emit(
        &mut self,
        path: &Path,
        known_small_files: &mut HashMap<PathBuf, time::Instant>,
        emitter: &impl FileSourceInternalEvents,
    ) -> Option<FileFingerprint> {
        let metadata = match fs::metadata(path).await {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    self.fingerprint(path).await.map(Some)
                } else {
                    Ok(None)
                }
            }
            Err(e) => Err(e),
        };

        metadata
            .inspect(|_| {
                // Drop the path from the small files map if we've got enough data to fingerprint it.
                known_small_files.remove(&path.to_path_buf());
            })
            .map_err(|error| {
                match error.kind() {
                    ErrorKind::UnexpectedEof => {
                        if !known_small_files.contains_key(path) {
                            emitter.emit_file_checksum_failed(path);
                            known_small_files.insert(path.to_path_buf(), time::Instant::now());
                        }
                        return;
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
                known_small_files.remove(&path.to_path_buf());
            })
            .ok()
            .flatten()
    }
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
            Ok(0) => return Err(std::io::Error::new(ErrorKind::UnexpectedEof, "EOF reached")),
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
    use std::{collections::HashMap, fs, io::Error, path::Path, time::Duration};

    use async_compression::tokio::bufread::GzipEncoder;
    use bytes::BytesMut;
    use tempfile::{TempDir, tempdir};

    use super::{FileSourceInternalEvents, FingerprintStrategy, Fingerprinter};

    use tokio::io::AsyncReadExt;

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

        let mut small_files = HashMap::new();
        assert!(
            fingerprinter
                .fingerprint_or_emit(target_dir.path(), &mut small_files, &NoErrors)
                .await
                .is_none()
        );
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

        fn emit_path_globbing_failed(&self, _: &Path, _: &Error) {
            panic!()
        }

        fn emit_file_line_too_long(&self, _: &BytesMut, _: usize, _: usize) {
            panic!()
        }
    }
}
