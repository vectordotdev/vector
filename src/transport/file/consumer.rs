use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use super::{get_segment_paths, Coordinator};
use byteorder::{BigEndian, ReadBytesExt};
use uuid::Uuid;

// A Consumer provides read access to a Log by essentially tailing the Segment files. It starts at
// the end of the youngest segment file available when it's created, and then reads from that
// position at every `poll`. When there is data it simply reads and returns each record, and when
// it gets an EOF it will check whether there is a newer segment file it should move on to. The
// "offsets" it keeps track of are really just positions in the segment files and it requires
// a reference to the Coordinator in order to "commit" them.
pub struct Consumer {
    id: Uuid,
    topic_dir: PathBuf,
    file: BufReader<File>,
    current_path: PathBuf,
}

impl Consumer {
    pub fn new(topic_dir: PathBuf) -> io::Result<Consumer> {
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
            thread::sleep(Duration::from_millis(100));
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
