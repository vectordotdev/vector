use std::{
    collections::HashSet,
    fs::{self, metadata, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use sha2::{Sha256, Digest};

use crc::Crc;
use serde::{Deserialize, Serialize};

use crate::{metadata_ext::PortableFileExt, FileSourceInternalEvents};

const FINGERPRINT_CRC: Crc<u64> = Crc::<u64>::new(&crc::CRC_64_ECMA_182);
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
    ChecksumWithPathSalt {
        ignored_header_bytes: usize,
        lines: usize,
    },
    DevInode,
    FullContentChecksum,
    ModificationTime,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum FileFingerprint {
    #[serde(rename = "checksum")]
    BytesChecksum(u64),
    #[serde(alias = "first_line_checksum")]
    FirstLinesChecksum(u64),
    ChecksumWithPathSalt(u64, u64),
    DevInode(u64, u64),
    FullContentChecksum(u64),
    ModificationTime(u64, u64),
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
            },
            ChecksumWithPathSalt(c, salt) => {
                let mut buf = Vec::with_capacity(std::mem::size_of_val(c));
                buf.write_all(&c.to_be_bytes()).expect("writing to array");
                let mut salt_buf = Vec::with_capacity(std::mem::size_of_val(salt));
                salt_buf.write_all(&salt.to_be_bytes()).expect("writing to array");
                crc64_with_salt(&buf[..], &salt_buf[..])
            },
            FullContentChecksum(c) => *c,
            ModificationTime(path, update_timestamp) => {
                let mut buf = Vec::with_capacity(std::mem::size_of_val(update_timestamp) + std::mem::size_of_val(path));
                buf.write_all(&path.to_be_bytes()).expect("writing to array");
                buf.write_all(&update_timestamp.to_be_bytes()).expect("writing to array");
                FINGERPRINT_CRC.checksum(&buf[..])
            },
            Unknown(c) => *c,
        }
    }
    pub fn read_from_beginning(&self) -> bool {
        use FileFingerprint::*;
        match self {
            BytesChecksum(_) => false,
            FirstLinesChecksum(_) => false,
            ChecksumWithPathSalt(_, _) => false,
            DevInode(_, _) => false,
            FullContentChecksum(_) => false,
            ModificationTime(_, _) => false,
            Unknown(_) => false,
        }
    }
}

impl From<u64> for FileFingerprint {
    fn from(c: u64) -> Self {
        FileFingerprint::Unknown(c)
    }
}

impl Fingerprinter {
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
            FingerprintStrategy::FullContentChecksum => {
                let mut file = fs::File::open(&path)?;
                let mut hasher = Sha256::new();
                let _ = io::copy(&mut file, &mut hasher)?;
                let hash:[u8; 32] = hasher.finalize().try_into().unwrap();
                let fingerprint = FINGERPRINT_CRC.checksum(&hash);
                Ok(FullContentChecksum(fingerprint))
            }
            FingerprintStrategy::ModificationTime => {
                let metadata = fs::metadata(path)?;
                let update_timestamp = metadata.modified()?.duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap().as_secs();
                let path_str = path.to_str().unwrap();
                let mut hasher = DefaultHasher::new();
                path_str.hash(&mut hasher);
                Ok(ModificationTime(hasher.finish(), update_timestamp))
            }
            FingerprintStrategy::ChecksumWithPathSalt{ignored_header_bytes, lines} => {
                buffer.resize(self.max_line_length, 0u8);
                let mut fp = fs::File::open(path)?;
                fp.seek(SeekFrom::Start(ignored_header_bytes as u64))?;
                let bytes_read = fingerprinter_read_until(fp, b'\n', lines, buffer)?;
                let fingerprint = FINGERPRINT_CRC.checksum(&buffer[..bytes_read]);
                // handle path salt
                let path_str = path.to_str().unwrap();
                let mut hasher = DefaultHasher::new();
                path_str.hash(&mut hasher);
                Ok(ChecksumWithPathSalt(fingerprint, hasher.finish()))
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
                fp.seek(SeekFrom::Start(ignored_header_bytes as u64))?;
                let bytes_read = fingerprinter_read_until(fp, b'\n', lines, buffer)?;
                let fingerprint = FINGERPRINT_CRC.checksum(&buffer[..bytes_read]);
                Ok(FirstLinesChecksum(fingerprint))
            }
        }
    }

    pub fn get_fingerprint_or_log_error(
        &self,
        path: &Path,
        buffer: &mut Vec<u8>,
        known_small_files: &mut HashSet<PathBuf>,
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
            .map_err(|error| match error.kind() {
                io::ErrorKind::UnexpectedEof => {
                    if !known_small_files.contains(path) {
                        emitter.emit_file_checksum_failed(path);
                        known_small_files.insert(path.to_path_buf());
                    }
                }
                io::ErrorKind::NotFound => {
                    if !self.ignore_not_found {
                        emitter.emit_file_fingerprint_read_error(path, error);
                    }
                }
                _ => {
                    emitter.emit_file_fingerprint_read_error(path, error);
                }
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
                fp.read_exact(&mut buffer[..bytes])?;
                let fingerprint = FINGERPRINT_CRC.checksum(&buffer[..]);
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

pub fn crc64_with_salt(data: &[u8], salt: &[u8]) -> u64 {
    let crc = Crc::<u64>::new(&crc::CRC_64_XZ);

    // First hash the original data
    let mut digest = crc.digest();
    digest.update(data);
    let data_hash = digest.finalize();

    // Then hash the combination of the first hash and the salt
    let mut salted_digest = crc.digest();
    salted_digest.update(&data_hash.to_be_bytes());
    salted_digest.update(salt);

    salted_digest.finalize()
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
    use std::{collections::HashSet, fs, io::Error, path::Path, time::Duration};

    use tempfile::tempdir;

    use super::{FileSourceInternalEvents, FingerprintStrategy, Fingerprinter};

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

    #[test]
    fn test_first_line_checksum_fingerprint() {
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
        let one_line = prepare_test("one_line.log", b"hello world\n");
        let one_line_duplicate = prepare_test("one_line_duplicate.log", b"hello world\n");
        let one_line_continued =
            prepare_test("one_line_continued.log", b"hello world\nthe next line\n");
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
        let mut run = move |path| fingerprinter.get_fingerprint_of_file(path, &mut buf);

        assert!(run(&empty).is_err());
        assert!(run(&incomplete_line).is_err());
        assert!(run(&incomplete_under_max_line_length_by_one).is_err());

        assert!(run(&one_line).is_ok());
        assert!(run(&one_line_duplicate).is_ok());
        assert!(run(&one_line_continued).is_ok());
        assert!(run(&different_two_lines).is_ok());
        assert!(run(&exactly_max_line_length).is_ok());
        assert!(run(&exceeding_max_line_length).is_ok());

        assert_eq!(run(&one_line).unwrap(), run(&one_line_duplicate).unwrap());
        assert_eq!(run(&one_line).unwrap(), run(&one_line_continued).unwrap());

        assert_ne!(run(&one_line).unwrap(), run(&different_two_lines).unwrap());

        assert_eq!(
            run(&exactly_max_line_length).unwrap(),
            run(&exceeding_max_line_length).unwrap()
        );
    }

    #[test]
    fn test_first_two_lines_checksum_fingerprint() {
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
        let different_three_lines = prepare_test(
            "different_three_lines.log",
            b"line one\nline two\nine three\n",
        );

        let mut buf = Vec::new();
        let mut run = move |path| fingerprinter.get_fingerprint_of_file(path, &mut buf);

        assert!(run(&incomplete_lines).is_err());

        assert!(run(&two_lines).is_ok());
        assert!(run(&two_lines_duplicate).is_ok());
        assert!(run(&two_lines_continued).is_ok());
        assert!(run(&different_three_lines).is_ok());

        assert_eq!(run(&two_lines).unwrap(), run(&two_lines_duplicate).unwrap());
        assert_eq!(run(&two_lines).unwrap(), run(&two_lines_continued).unwrap());

        assert_ne!(
            run(&two_lines).unwrap(),
            run(&different_three_lines).unwrap()
        );
    }

    #[test]
    fn test_inode_fingerprint() {
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
        assert!(fingerprinter
            .get_fingerprint_of_file(&empty_path, &mut buf)
            .is_ok());
        assert!(fingerprinter
            .get_fingerprint_of_file(&small_path, &mut buf)
            .is_ok());
        assert_ne!(
            fingerprinter
                .get_fingerprint_of_file(&medium_path, &mut buf)
                .unwrap(),
            fingerprinter
                .get_fingerprint_of_file(&duplicate_path, &mut buf)
                .unwrap()
        );
    }

    #[test]
    fn no_error_on_dir() {
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
        let mut small_files = HashSet::new();
        assert!(fingerprinter
            .get_fingerprint_or_log_error(target_dir.path(), &mut buf, &mut small_files, &NoErrors)
            .is_none());
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
    }
}
