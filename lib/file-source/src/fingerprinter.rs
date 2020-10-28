use crate::{metadata_ext::PortableFileExt, FileFingerprint, FileSourceInternalEvents};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Seek, Write};
use std::path::PathBuf;

#[derive(Clone)]
pub enum Fingerprinter {
    Checksum {
        bytes: usize,
        ignored_header_bytes: usize,
    },
    FirstLineChecksum {
        max_line_length: usize,
    },
    DevInode,
}

impl Fingerprinter {
    pub fn get_fingerprint_of_file(
        &self,
        path: &PathBuf,
        buffer: &mut Vec<u8>,
    ) -> Result<FileFingerprint, io::Error> {
        match *self {
            Fingerprinter::DevInode => {
                let file_handle = File::open(path)?;
                let dev = file_handle.portable_dev()?;
                let ino = file_handle.portable_ino()?;
                buffer.clear();
                buffer.write_all(&dev.to_be_bytes())?;
                buffer.write_all(&ino.to_be_bytes())?;
            }
            Fingerprinter::Checksum {
                ignored_header_bytes,
                bytes,
            } => {
                let i = ignored_header_bytes as u64;
                let b = bytes;
                buffer.resize(b, 0u8);
                let mut fp = fs::File::open(path)?;
                fp.seek(io::SeekFrom::Start(i))?;
                fp.read_exact(&mut buffer[..b])?;
            }
            Fingerprinter::FirstLineChecksum { max_line_length } => {
                buffer.resize(max_line_length, 0u8);
                let fp = fs::File::open(path)?;
                fingerprinter_read_until(fp, b'\n', buffer)?;
            }
        }
        let fingerprint = crc::crc64::checksum_ecma(&buffer[..]);
        Ok(fingerprint)
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

        if let Some((pos, _)) = buf[..read].iter().enumerate().find(|(_, &c)| c == delim) {
            for el in &mut buf[(pos + 1)..] {
                *el = 0;
            }
            break;
        }

        buf = &mut buf[read..];
    }
    Ok(())
}
