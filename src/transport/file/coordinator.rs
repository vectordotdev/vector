use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{get_segment_paths, Consumer, Log};
use uuid::Uuid;

// Coordinator manages the set of logs that exist on each node, creating new ones and building
// consumers for existing logs. It also manages offsets, which for now are just dumb pairs of
// a file path and bytes position.
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
        fs::create_dir_all(&dir)?;
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

    pub fn set_offset(&mut self, log: &Path, consumer: &Uuid, segment: &Path) {
        if let Some(offsets) = self.logs.get_mut(log) {
            offsets.insert(consumer.to_owned(), segment.to_path_buf());
        }
    }

    pub fn enforce_retention(&mut self) -> io::Result<()> {
        for (dir, offsets) in &self.logs {
            if let Some(min_segment) = offsets.values().min() {
                for old_segment in get_segment_paths(&dir)?.filter(|path| path < min_segment) {
                    fs::remove_file(old_segment)?;
                }
            }
        }
        Ok(())
    }
}
