use crate::{metadata_ext::PortableFileExt, FileSourceInternalEvents};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

#[derive(Clone)]
pub enum Fingerprinter {
    Checksum {
        bytes: usize,
        ignored_header_bytes: usize,
    },
    FirstLineChecksum {
        max_line_length: usize,
        ignored_header_bytes: usize,
    },
    DevInode,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize)]
pub enum FileFingerprint {
    Checksum(u64),
    FirstLineChecksum(u64),
    DevInode(u64, u64),
    Unknown(u64),
}

impl FileFingerprint {
    pub fn to_legacy(&self) -> u64 {
        use FileFingerprint::*;

        match self {
            Checksum(c) => *c,
            FirstLineChecksum(c) => *c,
            DevInode(dev, ino) => {
                let mut buf = Vec::with_capacity(std::mem::size_of_val(dev) * 2);
                buf.write_all(&dev.to_be_bytes()).expect("writing to array");
                buf.write_all(&ino.to_be_bytes()).expect("writing to array");
                crc::crc64::checksum_ecma(&buf[..])
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

impl Fingerprinter {
    pub fn get_fingerprint_of_file(
        &self,
        path: &PathBuf,
        buffer: &mut Vec<u8>,
    ) -> Result<FileFingerprint, io::Error> {
        use FileFingerprint::*;

        match *self {
            Fingerprinter::DevInode => {
                let file_handle = File::open(path)?;
                let dev = file_handle.portable_dev()?;
                let ino = file_handle.portable_ino()?;
                Ok(DevInode(dev, ino))
            }
            Fingerprinter::Checksum {
                ignored_header_bytes,
                bytes,
            } => {
                buffer.resize(bytes, 0u8);
                let mut fp = fs::File::open(path)?;
                fp.seek(io::SeekFrom::Start(ignored_header_bytes as u64))?;
                fp.read_exact(&mut buffer[..bytes])?;
                let fingerprint = crc::crc64::checksum_ecma(&buffer[..]);
                Ok(Checksum(fingerprint))
            }
            Fingerprinter::FirstLineChecksum {
                max_line_length,
                ignored_header_bytes,
            } => {
                buffer.resize(max_line_length, 0u8);
                let mut fp = fs::File::open(path)?;
                fp.seek(SeekFrom::Start(ignored_header_bytes as u64))?;
                fingerprinter_read_until(fp, b'\n', buffer)?;
                let fingerprint = crc::crc64::checksum_ecma(&buffer[..]);
                Ok(FirstLineChecksum(fingerprint))
            }
        }
    }

    pub fn get_fingerprint_or_log_error(
        &self,
        path: &PathBuf,
        buffer: &mut Vec<u8>,
        known_small_files: &mut HashSet<PathBuf>,
        emitter: &impl FileSourceInternalEvents,
    ) -> Option<FileFingerprint> {
        self.get_fingerprint_of_file(path, buffer)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::UnexpectedEof {
                    if !known_small_files.contains(path) {
                        emitter.emit_file_checksum_failed(path);
                        known_small_files.insert(path.clone());
                    }
                } else {
                    emitter.emit_file_fingerprint_read_failed(path, error);
                }
            })
            .ok()
    }
}

fn fingerprinter_read_until(mut r: impl Read, delim: u8, mut buf: &mut [u8]) -> io::Result<()> {
    while !buf.is_empty() {
        let read = match r.read(buf) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EOF reached")),
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        if let Some(pos) = buf[..read].iter().position(|&c| c == delim) {
            for el in &mut buf[(pos + 1)..] {
                *el = 0;
            }
            break;
        }

        buf = &mut buf[read..];
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::Fingerprinter;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_checksum_fingerprint() {
        let fingerprinter = Fingerprinter::Checksum {
            bytes: 256,
            ignored_header_bytes: 0,
        };

        let target_dir = tempdir().unwrap();
        let enough_data = vec![b'x'; 256];
        let not_enough_data = vec![b'x'; 199];
        let empty_path = target_dir.path().join("empty.log");
        let big_enough_path = target_dir.path().join("big_enough.log");
        let duplicate_path = target_dir.path().join("duplicate.log");
        let not_big_enough_path = target_dir.path().join("not_big_enough.log");
        fs::write(&empty_path, &[]).unwrap();
        fs::write(&big_enough_path, &enough_data).unwrap();
        fs::write(&duplicate_path, &enough_data).unwrap();
        fs::write(&not_big_enough_path, &not_enough_data).unwrap();

        let mut buf = Vec::new();
        assert!(fingerprinter
            .get_fingerprint_of_file(&empty_path, &mut buf)
            .is_err());
        assert!(fingerprinter
            .get_fingerprint_of_file(&big_enough_path, &mut buf)
            .is_ok());
        assert!(fingerprinter
            .get_fingerprint_of_file(&not_big_enough_path, &mut buf)
            .is_err());
        assert_eq!(
            fingerprinter
                .get_fingerprint_of_file(&big_enough_path, &mut buf)
                .unwrap(),
            fingerprinter
                .get_fingerprint_of_file(&duplicate_path, &mut buf)
                .unwrap(),
        );
    }

    #[test]
    fn test_first_line_checksum_fingerprint() {
        let max_line_length = 64;
        let fingerprinter = Fingerprinter::FirstLineChecksum {
            max_line_length,
            ignored_header_bytes: 0,
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
        let incomlete_line = prepare_test("incomlete_line.log", b"missing newline char");
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
        assert!(run(&incomlete_line).is_err());
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
    fn test_inode_fingerprint() {
        let fingerprinter = Fingerprinter::DevInode;

        let target_dir = tempdir().unwrap();
        let small_data = vec![b'x'; 1];
        let medium_data = vec![b'x'; 256];
        let empty_path = target_dir.path().join("empty.log");
        let small_path = target_dir.path().join("small.log");
        let medium_path = target_dir.path().join("medium.log");
        let duplicate_path = target_dir.path().join("duplicate.log");
        fs::write(&empty_path, &[]).unwrap();
        fs::write(&small_path, &small_data).unwrap();
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
}
