use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use uuid::Uuid;

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

pub struct Log {
    dir: PathBuf,
    current_segment: Segment,
    current_offset: u64,
}

impl Log {
    fn new(dir: PathBuf) -> io::Result<Log> {
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

pub struct Coordinator {
    data_dir: PathBuf,
    logs: BTreeMap<PathBuf, BTreeMap<Uuid, PathBuf>>,
}

impl Coordinator {
    pub fn new<T: AsRef<Path>>(dir: T) -> Coordinator {
        Coordinator {
            data_dir: dir.as_ref().to_path_buf(),
            logs: BTreeMap::new(),
        }
    }

    pub fn create_log(&mut self, topic: &str) -> io::Result<Log> {
        let dir = self.data_dir.join(topic);
        ::std::fs::create_dir_all(&dir)?;
        debug!("creating log at {:?}", dir);
        let log = Log::new(dir.clone())?;
        self.logs.insert(dir, BTreeMap::new());
        Ok(log)
    }

    pub fn build_consumer(&self, topic: &str) -> io::Result<Consumer> {
        let dir = self.data_dir.join(topic);
        debug!("building consumer for log at {:?}", dir);
        Consumer::new(dir)
    }

    fn set_offset(&mut self, log: &Path, consumer: &Uuid, segment: &Path) {
        if let Some(offsets) = self.logs.get_mut(log) {
            offsets.insert(consumer.to_owned(), segment.to_path_buf());
        }
    }

    pub fn enforce_retention(&mut self) -> io::Result<()> {
        for (dir, offsets) in &self.logs {
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
fn get_segment_paths(dir: &Path) -> io::Result<impl Iterator<Item = PathBuf>> {
    ::std::fs::read_dir(dir)?
        .map(|r| r.map(|entry| entry.path()))
        .collect::<Result<Vec<PathBuf>, _>>()
        .map(|r| r.into_iter())
}

pub struct Consumer {
    id: Uuid,
    topic_dir: PathBuf,
    file: BufReader<File>,
    current_path: PathBuf,
}

impl Consumer {
    fn new(topic_dir: PathBuf) -> io::Result<Consumer> {
        let latest_segment = get_segment_paths(&topic_dir)?
            .max()
            .expect("i don't know how to deal with empty dirs yet");

        let mut file = BufReader::new(OpenOptions::new().read(true).open(&latest_segment)?);
        let _pos = file.seek(SeekFrom::End(0))?;

        Ok(Consumer {
            id: Uuid::new_v4(),
            topic_dir,
            file,
            current_path: latest_segment,
        })
    }

    pub fn poll(&mut self) -> io::Result<Vec<Vec<u8>>> {
        let mut records = Vec::new();
        loop {
            match self.file.read_u32::<BigEndian>() {
                Ok(len) => {
                    let mut record = vec![0; len as usize];
                    self.file.read_exact(&mut record[..])?;
                    records.push(record);
                    if records.len() > 10_000 {
                        break;
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    if self.maybe_advance_segment()? {
                        continue;
                    } else {
                        break;
                    }
                }
                Err(e) => return Err(e),
            }
        }
        if records.is_empty() {
            trace!("sleeping!");
            ::std::thread::sleep(::std::time::Duration::from_millis(100));
        }
        Ok(records)
    }

    fn maybe_advance_segment(&mut self) -> io::Result<bool> {
        let mut segments = ::std::fs::read_dir(&self.topic_dir)?
            .map(|r| r.map(|entry| entry.path()))
            .collect::<Result<Vec<PathBuf>, _>>()?;
        segments.sort();

        let next_segment = segments
            .into_iter()
            .skip_while(|path| path != &self.current_path)
            .nth(1);

        if let Some(path) = next_segment {
            self.file = BufReader::new(OpenOptions::new().read(true).open(&path)?);
            self.current_path = path;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn commit_offsets(&self, coordinator: &mut Coordinator) {
        coordinator.set_offset(&self.topic_dir, &self.id, &self.current_path);
    }
}
