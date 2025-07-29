use std::{
    collections::HashMap,
    fs::{self, metadata, File},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time,
};

use crc::Crc;
use flate2::bufread::GzDecoder;
use serde::{Deserialize, Serialize};
use vector_common::constants::GZIP_MAGIC;

use crate::{internal_events::FileSourceInternalEvents, metadata_ext::PortableFileExt};

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
    pub fn as_legacy(&self) -> u64 {
        use FileFingerprint::*;

        match self {
            BytesChecksum(c) => *c,
            FirstLinesChecksum(c) => *c,
            DevInode(dev, ino) => {
                let mut buf = Vec::with_capacity(std::mem::size_of_val(dev) * 2);
                buf.write_all(&dev.to_be_bytes()).expect("writing to array");
                buf.write_all(&ino.to_be_bytes()).expect("writing to array");
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
    fn check(fp: &mut File) -> Result<Option<SupportedCompressionAlgorithms>, std::io::Error>;
    fn reader<'a>(fp: &'a mut File) -> Result<Box<dyn BufRead + 'a>, std::io::Error>;
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
    fn check(fp: &mut File) -> Result<Option<SupportedCompressionAlgorithms>, std::io::Error> {
        let mut algorithm: Option<SupportedCompressionAlgorithms> = None;
        for compression_algorithm in SupportedCompressionAlgorithms::values() {
            // magic headers for algorithms can be of different lengths, and using a buffer too long could exceed the length of the file
            // so instantiate and check the various sizes in monotonically increasing order
            let magic_header_bytes = compression_algorithm.magic_header_bytes();

            let mut magic = vec![0u8; magic_header_bytes.len()];

            fp.seek(SeekFrom::Start(0))?;
            let result = fp.read_exact(&mut magic);

            if result.is_err() {
                fp.seek(SeekFrom::Start(0))?;
                return Err(result.unwrap_err());
            }

            if magic == magic_header_bytes {
                algorithm = Some(compression_algorithm);
                break;
            }
        }
        fp.seek(SeekFrom::Start(0))?;
        Ok(algorithm)
    }

    fn reader<'a>(fp: &'a mut File) -> Result<Box<dyn BufRead + 'a>, std::io::Error> {
        // To support new compression algorithms, add them below
        match Self::check(fp)? {
            Some(SupportedCompressionAlgorithms::Gzip) => {
                Ok(Box::new(BufReader::new(GzDecoder::new(BufReader::new(fp)))))
            }
            // No compression, or read the raw bytes
            None => Ok(Box::new(BufReader::new(fp))),
        }
    }
}

fn skip_first_n_bytes<R: BufRead>(reader: &mut R, n: usize) -> io::Result<()> {
    // We cannot simply seek the file by n because the file may be compressed;
    // to skip the first n decompressed bytes, we decompress up to n and discard the output.
    let mut skipped_bytes = 0;
    while skipped_bytes < n {
        let chunk = reader.fill_buf()?;
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
    pub fn get_fingerprint_of_file(
        &self,
        path: &Path,
        buffer: &mut Vec<u8>,
    ) -> Result<FileFingerprint, io::Error> {
        use FileFingerprint::*;

        match self.strategy {
            FingerprintStrategy::DevInode => {
                let file_handle = File::open(path)?;
                let dev = file_handle.portable_dev()?;
                let ino = file_handle.portable_ino()?;
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
                let mut fp = fs::File::open(path)?;
                let mut reader = UncompressedReaderImpl::reader(&mut fp)?;

                skip_first_n_bytes(&mut reader, ignored_header_bytes)?;
                let bytes_read = fingerprinter_read_until(reader, b'\n', lines, buffer)?;
                let fingerprint = FINGERPRINT_CRC.checksum(&buffer[..bytes_read]);
                Ok(FirstLinesChecksum(fingerprint))
            }
        }
    }

    pub fn get_fingerprint_or_log_error(
        &self,
        path: &Path,
        buffer: &mut Vec<u8>,
        known_small_files: &mut HashMap<PathBuf, time::Instant>,
        emitter: &impl FileSourceInternalEvents,
    ) -> Option<FileFingerprint> {
        metadata(path)
            .and_then(|metadata| {
                if metadata.is_dir() {
                    Ok(None)
                } else {
                    self.get_fingerprint_of_file(path, buffer).map(Some)
                }
            })
            .inspect(|_| {
                // Drop the path from the small files map if we've got enough data to fingerprint it.
                known_small_files.remove(&path.to_path_buf());
            })
            .map_err(|error| {
                match error.kind() {
                    io::ErrorKind::UnexpectedEof => {
                        if !known_small_files.contains_key(path) {
                            emitter.emit_file_checksum_failed(path);
                            known_small_files.insert(path.to_path_buf(), time::Instant::now());
                        }
                        return;
                    }
                    io::ErrorKind::NotFound => {
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

    pub fn get_bytes_checksum(
        &self,
        path: &Path,
        buffer: &mut Vec<u8>,
    ) -> Result<Option<FileFingerprint>, io::Error> {
        match self.strategy {
            FingerprintStrategy::Checksum {
                bytes,
                ignored_header_bytes,
                lines: _,
            } => {
                buffer.resize(bytes, 0u8);
                let mut fp = fs::File::open(path)?;
                fp.seek(io::SeekFrom::Start(ignored_header_bytes as u64))?;

                // Make sure we don't exceed the buffer size
                let bytes_to_read = std::cmp::min(bytes, buffer.len());
                if bytes_to_read == 0 {
                    // Buffer is empty, return a default fingerprint
                    return Ok(Some(FileFingerprint::BytesChecksum(0)));
                }

                // Read as much as we can
                let bytes_read = fp.read(&mut buffer[..bytes_to_read])?;
                let fingerprint = FINGERPRINT_CRC.checksum(&buffer[..bytes_read]);
                Ok(Some(FileFingerprint::BytesChecksum(fingerprint)))
            }
            _ => Ok(None),
        }
    }

    /// Calculates checksums using strategy pre-0.14.0
    /// <https://github.com/vectordotdev/vector/issues/8182>
    pub fn get_legacy_checksum(
        &self,
        path: &Path,
        buffer: &mut Vec<u8>,
    ) -> Result<Option<FileFingerprint>, io::Error> {
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
                let mut fp = fs::File::open(path)?;
                fp.seek(SeekFrom::Start(ignored_header_bytes as u64))?;
                fingerprinter_read_until_and_zerofill_buf(fp, b'\n', lines, buffer)?;
                let fingerprint = LEGACY_FINGERPRINT_CRC.checksum(&buffer[..]);
                Ok(Some(FileFingerprint::FirstLinesChecksum(fingerprint)))
            }
            _ => Ok(None),
        }
    }
    /// For upgrades from legacy strategy version
    /// <https://github.com/vectordotdev/vector/issues/15700>
    pub fn get_legacy_first_lines_checksum(
        &self,
        path: &Path,
        buffer: &mut Vec<u8>,
    ) -> Result<Option<FileFingerprint>, io::Error> {
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
                let mut fp = fs::File::open(path)?;
                fp.seek(SeekFrom::Start(ignored_header_bytes as u64))?;
                fingerprinter_read_until_and_zerofill_buf(fp, b'\n', lines, buffer)?;
                let fingerprint = FINGERPRINT_CRC.checksum(&buffer[..]);
                Ok(Some(FileFingerprint::FirstLinesChecksum(fingerprint)))
            }
            _ => Ok(None),
        }
    }
}

/// Saved for backwards compatibility.
fn fingerprinter_read_until_and_zerofill_buf(
    mut r: impl Read,
    delim: u8,
    mut count: usize,
    mut buf: &mut [u8],
) -> io::Result<()> {
    'main: while !buf.is_empty() {
        let read = match r.read(buf) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EOF reached")),
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
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

fn fingerprinter_read_until(
    mut r: impl Read,
    delim: u8,
    mut count: usize,
    mut buf: &mut [u8],
) -> io::Result<usize> {
    let mut total_read = 0;
    'main: while !buf.is_empty() {
        let read = match r.read(buf) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EOF reached")),
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
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
        fs,
        io::{Error, Write},
        path::Path,
        time::Duration,
    };

    use bytes::BytesMut;
    use flate2::write::GzEncoder;
    use tempfile::tempdir;

    use crate::internal_events::FileSourceInternalEvents;

    use super::{FingerprintStrategy, Fingerprinter};

    // Used in tests
    #[allow(dead_code)]
    fn create_gzip_file(data: &mut [u8]) -> Vec<u8> {
        let mut buffer = vec![];
        let mut encoder = GzEncoder::new(&mut buffer, flate2::Compression::default());
        encoder.write_all(data).expect("Failed to write data");
        encoder
            .finish()
            .expect("Failed to finish encoding with gzip footer");
        buffer
    }

    #[test]
    fn test_checksum_fingerprint() {
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
        assert!(fingerprinter
            .get_fingerprint_of_file(&empty_path, &mut buf)
            .is_err());
        assert!(fingerprinter
            .get_fingerprint_of_file(&full_line_path, &mut buf)
            .is_ok());
        assert!(fingerprinter
            .get_fingerprint_of_file(&not_full_line_path, &mut buf)
            .is_err());
        assert_eq!(
            fingerprinter
                .get_fingerprint_of_file(&full_line_path, &mut buf)
                .unwrap(),
            fingerprinter
                .get_fingerprint_of_file(&duplicate_path, &mut buf)
                .unwrap(),
        );
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
