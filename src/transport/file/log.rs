use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use super::get_segment_paths;
use byteorder::{BigEndian, WriteBytesExt};

// Log is like a Kafka topic and producer rolled into one. The combination isn't ideal, but comes
// somewhat naturally from this being a single-node system. It keeps track of its data directory
// (subdirectory of data directory of the Coordinator that created it), a writeable handle to its
// currently active segment file, and the current offset (i.e. count of records written over all
// segments).
pub struct Log {
    dir: PathBuf,
    current_segment: Segment,
    current_offset: u64,
}

impl Log {
    pub fn new(dir: PathBuf) -> io::Result<Log> {
        assert!(dir.is_dir());
        let current_segment = Segment::new(&dir, 0)?;
        Ok(Log {
            dir,
            current_segment,
            current_offset: 0,
        })
    }

    pub fn append(&mut self, records: &[&[u8]]) -> io::Result<()> {
        for record in records {
            self.current_offset += 1;
            if self.current_segment.position + record.len() as u64 > 64 * 1024 * 1024 {
                self.roll_segment()?;
            }
            self.current_segment.append(record)?;
            self.current_segment.flush()?;
        }
        Ok(())
    }

    pub fn roll_segment(&mut self) -> io::Result<()> {
        self.current_segment = Segment::new(&self.dir, self.current_offset)?;
        Ok(())
    }

    pub fn get_segments(&self) -> io::Result<impl Iterator<Item = PathBuf>> {
        get_segment_paths(&self.dir)
    }
}

struct Segment {
    file: BufWriter<File>,
    position: u64,
}

impl Segment {
    fn new(dir: &Path, offset: u64) -> io::Result<Segment> {
        let filename = format!("{:020}.log", offset);
        let file = BufWriter::new(
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(dir.join(filename))?,
        );
        Ok(Segment { file, position: 0 })
    }

    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.write_u32::<BigEndian>(bytes.len() as u32)?;
        self.file.write_all(bytes)?;
        self.position += 4 + bytes.len() as u64;
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
