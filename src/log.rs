use std::io;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::collections::BTreeMap;

use serde;
use serde_json;
use uuid::Uuid;
use byteorder::{BigEndian, WriteBytesExt, ReadBytesExt};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    message: String,
}

impl Record {
    pub fn new(msg: &str) -> Record {
        Record { message: msg.to_string() }
    }
}


struct Segment {
    file: File,
    position: u64,
}

impl Segment {
    fn new(dir: &Path, offset: u64) -> io::Result<Segment> {
        let filename = format!("{:08}.log", offset);
        let file = OpenOptions::new().append(true).create(true).open(dir.join(filename))?;
        Ok(Segment { file, position: 0 })
    }

    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.write_u32::<BigEndian>(bytes.len() as u32)?;
        self.file.write_all(bytes)?;
        self.position += 4 + bytes.len() as u64;
        Ok(())
    }
}

pub struct Log {
    dir: PathBuf,
    current_segment: Segment,
}

impl Log {
    fn new(dir: PathBuf) -> io::Result<Log> {
        assert!(dir.is_dir());
        let current_segment = Segment::new(&dir, 0)?;
        Ok(Log { dir, current_segment })
    }

    pub fn append(&mut self, records: &[Record]) -> io::Result<()> {
        for record in records {
            let encoded = serde_json::to_string(&record).expect("json encoding failure");
            self.current_segment.append(encoded.as_bytes())?;
        }
        Ok(())
    }

    pub fn roll_segment(&mut self) -> io::Result<()> {
        self.current_segment = Segment::new(&self.dir, 1)?;
        Ok(())
    }
}

#[derive(Default)]
pub struct Coordinator {
    logs: BTreeMap<PathBuf, BTreeMap<Uuid, PathBuf>>,
}

impl Coordinator {
    pub fn create_log<T: AsRef<Path>>(&mut self, path: T) -> io::Result<Log> {
        let dir = path.as_ref().to_path_buf();
        let log = Log::new(dir.clone())?;
        self.logs.insert(dir, BTreeMap::new());
        Ok(log)
    }

    fn set_offset(&mut self, log: &Path, consumer: &Uuid, segment: &Path) {
        if let Some(offsets) = self.logs.get_mut(log) {
            offsets.insert(consumer.to_owned(), segment.to_path_buf());
        }
    }

    pub fn enforce_retention(&mut self) -> io::Result<()> {
        for (dir, offsets) in self.logs.iter() {
            if let Some(min_segment) = offsets.values().min() {
                for old_segment in get_segment_paths(&dir)?.filter(|path| path < min_segment) {
                    ::std::fs::remove_file(old_segment)?;
                }
            }
        }
        Ok(())
    }
}

// if we put this in log we have to create one, which is side effect-y
fn get_segment_paths(dir: &Path) -> io::Result<impl Iterator<Item=PathBuf>> {
    ::std::fs::read_dir(dir)?
        .map(|r| r.map(|entry| entry.path()))
        .collect::<Result<Vec<PathBuf>, _>>()
        .map(|r| r.into_iter())
}

pub struct Consumer {
    id: Uuid,
    dir: PathBuf,
    file: File,
    current_path: PathBuf,
}

impl Consumer {
    pub fn new<T: AsRef<Path>>(path: T) -> io::Result<Consumer> {
        let dir = path.as_ref().to_path_buf();

        let latest_segment = get_segment_paths(&dir)?.max()
            .expect("i don't know how to deal with empty dirs yet");

        let mut file = OpenOptions::new().read(true).open(&latest_segment)?;
        let _pos = file.seek(SeekFrom::End(0))?;

        Ok(Consumer { id: Uuid::new_v4(), dir, file, current_path: latest_segment })
    }

    pub fn poll(&mut self) -> io::Result<Vec<Record>> {
        let mut records = Vec::new();
        loop {
            match self.file.read_u32::<BigEndian>() {
                Ok(_len) => {
                    let mut de = serde_json::Deserializer::from_reader(&mut self.file);
                    let record: Record = serde::Deserialize::deserialize(&mut de).expect("failed to deserialize json");
                    records.push(record);
                    // self.position.offset += 4 + len as u64;
                },
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    if self.maybe_advance_segment()? {
                        continue
                    } else {
                        break
                    }
                },
                Err(e) => {
                    return Err(e)
                },
            }
        }
        Ok(records)
    }

    fn maybe_advance_segment(&mut self) -> io::Result<bool> {
        let mut segments = ::std::fs::read_dir(&self.dir)?
            .map(|r| r.map(|entry| entry.path()))
            .collect::<Result<Vec<PathBuf>, _>>()?;
        segments.sort();

        let next_segment = segments.into_iter()
            .skip_while(|path| path != &self.current_path)
            .skip(1)
            .next();

        if let Some(path) = next_segment {
            self.file = OpenOptions::new().read(true).open(&path)?;
            self.current_path = path;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn commit_offsets(&self, coordinator: &mut Coordinator) {
        coordinator.set_offset(&self.dir, &self.id, &self.current_path);
    }
}
