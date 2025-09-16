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
    io::{
        AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt,
        BufReader,
    },
};
use vector_common::constants::GZIP_MAGIC;

use crate::{
    AsyncFileInfo, internal_events::FileSourceInternalEvents, metadata_ext::PortableFileExt,
};

pub const FINGERPRINT_CRC: Crc<u64> = Crc::<u64>::new(&crc::CRC_64_ECMA_182);
const LEGACY_FINGERPRINT_CRC: Crc<u64> = Crc::<u64>::new(&crc::CRC_64_XZ);

#[derive(Debug, Clone)]
pub struct Fingerprinter {
    pub strategy: FingerprintStrategy,
    pub max_line_length: usize,
    pub ignore_not_found: bool,
}

#[derive(Debug, Clone)]
pub enum FingerprintStrategy {
    Checksum {
        bytes: usize,
        ignored_header_bytes: usize,
        lines: usize,
    },
    FirstLinesChecksum {
        ignored_header_bytes: usize,
        lines: usize,
    },
    DevInode,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum FileFingerprint {
    #[serde(rename = "checksum")]
    BytesChecksum(u64),
    #[serde(alias = "first_line_checksum")]
    FirstLinesChecksum(u64),
    DevInode(u64, u64),
    Unknown(u64),
}

impl FileFingerprint {
    pub async fn as_legacy(&self) -> u64 {
        use FileFingerprint::*;

        match self {
            BytesChecksum(c) => *c,
            FirstLinesChecksum(c) => *c,
            DevInode(dev, ino) => {
                let mut buf = Vec::with_capacity(std::mem::size_of_val(dev) * 2);
                buf.write_all(&dev.to_be_bytes())
                    .await
                    .expect("writing to array");
                buf.write_all(&ino.to_be_bytes())
                    .await
                    .expect("writing to array");
                FINGERPRINT_CRC.checksum(&buf[..])
            }
            Unknown(c) => *c,
        }
    }
}

impl From<u64> for FileFingerprint {
    fn from(c: u64) -> Self {
        FileFingerprint::Unknown(c)
    }
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
    pub fn new(strategy: FingerprintStrategy) -> Self {
        Self {
            strategy,
            max_line_length: 1024,
            ignore_not_found: false,
        }
    }
    pub async fn get_fingerprint_of_file(
        &self,
        path: &Path,
        buffer: &mut Vec<u8>,
    ) -> Result<FileFingerprint> {
        use FileFingerprint::*;

        match self.strategy {
            FingerprintStrategy::DevInode => {
                let file_handle = File::open(path).await?;
                let file_info = file_handle.file_info().await?;
                let dev = file_info.portable_dev();
                let ino = file_info.portable_ino();
                Ok(DevInode(dev, ino))
            }
            FingerprintStrategy::Checksum {
                ignored_header_bytes,
                bytes: _,
                lines,
            }
            | FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes,
                lines,
            } => {
                buffer.resize(self.max_line_length, 0u8);
                let mut fp = File::open(path).await?;
                let mut reader = UncompressedReaderImpl::reader(&mut fp).await?;

                skip_first_n_bytes(&mut reader, ignored_header_bytes).await?;
                let bytes_read = fingerprinter_read_until(reader, b'\n', lines, buffer).await?;
                let fingerprint = FINGERPRINT_CRC.checksum(&buffer[..bytes_read]);
                Ok(FirstLinesChecksum(fingerprint))
            }
        }
    }

    pub async fn get_fingerprint_or_log_error(
        &self,
        path: &Path,
        buffer: &mut Vec<u8>,
        known_small_files: &mut HashMap<PathBuf, time::Instant>,
        emitter: &impl FileSourceInternalEvents,
    ) -> Option<FileFingerprint> {
        let metadata = match fs::metadata(path).await {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    self.get_fingerprint_of_file(path, buffer).await.map(Some)
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

    pub async fn get_bytes_checksum(
        &self,
        path: &Path,
        buffer: &mut Vec<u8>,
    ) -> Result<Option<FileFingerprint>> {
        match self.strategy {
            FingerprintStrategy::Checksum {
                bytes,
                ignored_header_bytes,
                lines: _,
            } => {
                buffer.resize(bytes, 0u8);
                let mut fp = File::open(path).await?;
                fp.seek(SeekFrom::Start(ignored_header_bytes as u64))
                    .await?;
                // Make sure we don't exceed the buffer size
                let bytes_to_read = std::cmp::min(bytes, buffer.len());
                if bytes_to_read == 0 {
                    // Buffer is empty, return a default fingerprint
                    return Ok(Some(FileFingerprint::BytesChecksum(0)));
                }

                // Read as much as we can
                let bytes_read = fp.read(&mut buffer[..bytes_to_read]).await?;
                let fingerprint = FINGERPRINT_CRC.checksum(&buffer[..bytes_read]);
                Ok(Some(FileFingerprint::BytesChecksum(fingerprint)))
            }
            _ => Ok(None),
        }
    }

    /// Calculates checksums using strategy pre-0.14.0
    /// <https://github.com/vectordotdev/vector/issues/8182>
    pub async fn get_legacy_checksum(
        &self,
        path: &Path,
        buffer: &mut Vec<u8>,
    ) -> Result<Option<FileFingerprint>> {
        match self.strategy {
            FingerprintStrategy::Checksum {
                ignored_header_bytes,
                bytes: _,
                lines,
            }
            | FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes,
                lines,
            } => {
                buffer.resize(self.max_line_length, 0u8);
                let mut fp = File::open(path).await?;
                fp.seek(SeekFrom::Start(ignored_header_bytes as u64))
                    .await?;
                fingerprinter_read_until_and_zerofill_buf(fp, b'\n', lines, buffer).await?;
                let fingerprint = LEGACY_FINGERPRINT_CRC.checksum(&buffer[..]);
                Ok(Some(FileFingerprint::FirstLinesChecksum(fingerprint)))
            }
            _ => Ok(None),
        }
    }
    /// For upgrades from legacy strategy version
    /// <https://github.com/vectordotdev/vector/issues/15700>
    pub async fn get_legacy_first_lines_checksum(
        &self,
        path: &Path,
        buffer: &mut Vec<u8>,
    ) -> Result<Option<FileFingerprint>> {
        match self.strategy {
            FingerprintStrategy::Checksum {
                ignored_header_bytes,
                bytes: _,
                lines,
            }
            | FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes,
                lines,
            } => {
                buffer.resize(self.max_line_length, 0u8);
                let mut fp = File::open(path).await?;
                fp.seek(SeekFrom::Start(ignored_header_bytes as u64))
                    .await?;
                fingerprinter_read_until_and_zerofill_buf(fp, b'\n', lines, buffer).await?;
                let fingerprint = FINGERPRINT_CRC.checksum(&buffer[..]);
                Ok(Some(FileFingerprint::FirstLinesChecksum(fingerprint)))
            }
            _ => Ok(None),
        }
    }
}

/// Saved for backwards compatibility.
async fn fingerprinter_read_until_and_zerofill_buf(
    mut r: impl AsyncRead + Unpin + Send,
    delim: u8,
    mut count: usize,
    mut buf: &mut [u8],
) -> Result<()> {
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
                    for el in &mut buf[(pos + 1)..] {
                        *el = 0;
                    }
                    break 'main;
                } else {
                    count -= 1;
                }
            }
        }
        buf = &mut buf[read..];
    }
    Ok(())
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
    use std::{
        collections::HashMap,
        fs,
        io::{Error, Read, Write},
        path::Path,
        time::Duration,
    };

    use async_compression::tokio::bufread::GzipEncoder;
    use bytes::BytesMut;
    use tempfile::{TempDir, tempdir};

    use crate::internal_events::FileSourceInternalEvents;

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
        let fingerprinter = Fingerprinter {
            strategy: FingerprintStrategy::Checksum {
                bytes: 256,
                ignored_header_bytes: 0,
                lines: 1,
            },
            max_line_length: 1024,
            ignore_not_found: false,
        };

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

        let mut buf = Vec::new();
        assert!(
            fingerprinter
                .get_fingerprint_of_file(&empty_path, &mut buf)
                .await
                .is_err()
        );
        assert!(
            fingerprinter
                .get_fingerprint_of_file(&full_line_path, &mut buf)
                .await
                .is_ok()
        );
        assert!(
            fingerprinter
                .get_fingerprint_of_file(&not_full_line_path, &mut buf)
                .await
                .is_err()
        );
        assert_eq!(
            fingerprinter
                .get_fingerprint_of_file(&full_line_path, &mut buf)
                .await
                .unwrap(),
            fingerprinter
                .get_fingerprint_of_file(&duplicate_path, &mut buf)
                .await
                .unwrap(),
        );
    }

    #[tokio::test]
    async fn test_first_line_checksum_fingerprint() {
        let max_line_length = 64;
        let fingerprinter = Fingerprinter {
            strategy: FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 1,
            },
            max_line_length,
            ignore_not_found: false,
        };

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

        let mut buf = Vec::new();
        let mut run = async |path| fingerprinter.get_fingerprint_of_file(path, &mut buf).await;

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
        let fingerprinter = Fingerprinter {
            strategy: FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 2,
            },
            max_line_length,
            ignore_not_found: false,
        };

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

        let mut buf = Vec::new();
        let mut run = async move |path| fingerprinter.get_fingerprint_of_file(path, &mut buf).await;

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
        let fingerprinter = Fingerprinter {
            strategy: FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 14,
                lines: 2,
            },
            max_line_length,
            ignore_not_found: false,
        };

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

        let mut buf = Vec::new();
        let mut run = async move |path| fingerprinter.get_fingerprint_of_file(path, &mut buf).await;

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
        let fingerprinter = Fingerprinter {
            strategy: FingerprintStrategy::DevInode,
            max_line_length: 42,
            ignore_not_found: false,
        };

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

        let mut buf = Vec::new();
        assert!(
            fingerprinter
                .get_fingerprint_of_file(&empty_path, &mut buf)
                .await
                .is_ok()
        );
        assert!(
            fingerprinter
                .get_fingerprint_of_file(&small_path, &mut buf)
                .await
                .is_ok()
        );
        assert_ne!(
            fingerprinter
                .get_fingerprint_of_file(&medium_path, &mut buf)
                .await
                .unwrap(),
            fingerprinter
                .get_fingerprint_of_file(&duplicate_path, &mut buf)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn no_error_on_dir() {
        let target_dir = tempdir().unwrap();
        let fingerprinter = Fingerprinter {
            strategy: FingerprintStrategy::Checksum {
                bytes: 256,
                ignored_header_bytes: 0,
                lines: 1,
            },
            max_line_length: 1024,
            ignore_not_found: false,
        };

        let mut buf = Vec::new();
        let mut small_files = HashMap::new();
        assert!(
            fingerprinter
                .get_fingerprint_or_log_error(
                    target_dir.path(),
                    &mut buf,
                    &mut small_files,
                    &NoErrors
                )
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

        fn emit_path_globbing_failed(&self, _: &Path, _: &Error) {}

        fn emit_file_line_too_long(&self, _: &BytesMut, _: usize, _: usize) {}
    }
}
