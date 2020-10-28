use super::{FileFingerprint, FilePosition};
use glob::glob;
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time,
};

pub struct Checkpointer {
    directory: PathBuf,
    glob_string: String,
    checkpoints: HashMap<FileFingerprint, FilePosition>,
}

impl Checkpointer {
    pub fn new(data_dir: &Path) -> Checkpointer {
        let directory = data_dir.join("checkpoints");
        let glob_string = directory.join("*").to_string_lossy().into_owned();
        Checkpointer {
            directory,
            glob_string,
            checkpoints: HashMap::new(),
        }
    }

    fn encode(&self, fng: FileFingerprint, pos: FilePosition) -> PathBuf {
        self.directory.join(format!("{:x}.{}", fng, pos))
    }
    fn decode(&self, path: &Path) -> (FileFingerprint, FilePosition) {
        let file_name = &path.file_name().unwrap().to_string_lossy();
        scan_fmt!(file_name, "{x}.{}", [hex FileFingerprint], FilePosition).unwrap()
    }

    pub fn set_checkpoint(&mut self, fng: FileFingerprint, pos: FilePosition) {
        self.checkpoints.insert(fng, pos);
    }

    pub fn get_checkpoint(&self, fng: FileFingerprint) -> Option<FilePosition> {
        self.checkpoints.get(&fng).cloned()
    }

    pub fn write_checkpoints(&mut self) -> Result<usize, io::Error> {
        fs::remove_dir_all(&self.directory).ok();
        fs::create_dir_all(&self.directory)?;
        for (&fng, &pos) in self.checkpoints.iter() {
            fs::File::create(self.encode(fng, pos))?;
        }
        Ok(self.checkpoints.len())
    }

    pub fn read_checkpoints(&mut self, ignore_before: Option<time::SystemTime>) {
        for path in glob(&self.glob_string).unwrap().flatten() {
            if let Some(ignore_before) = ignore_before {
                if let Ok(Ok(modified)) = fs::metadata(&path).map(|metadata| metadata.modified()) {
                    if modified < ignore_before {
                        fs::remove_file(path).ok();
                        continue;
                    }
                }
            }
            let (fng, pos) = self.decode(&path);
            self.checkpoints.insert(fng, pos);
        }
    }
}

#[cfg(test)]
mod test {
    use super::{Checkpointer, FileFingerprint, FilePosition};
    use tempfile::tempdir;

    #[test]
    fn test_checkpointer_basics() {
        let fingerprint: FileFingerprint = 0x1234567890abcdef;
        let position: FilePosition = 1234;
        let data_dir = tempdir().unwrap();
        let mut chkptr = Checkpointer::new(&data_dir.path());
        assert_eq!(
            chkptr.decode(&chkptr.encode(fingerprint, position)),
            (fingerprint, position)
        );
        chkptr.set_checkpoint(fingerprint, position);
        assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
    }

    #[test]
    fn test_checkpointer_restart() {
        let fingerprint: FileFingerprint = 0x1234567890abcdef;
        let position: FilePosition = 1234;
        let data_dir = tempdir().unwrap();
        {
            let mut chkptr = Checkpointer::new(&data_dir.path());
            chkptr.set_checkpoint(fingerprint, position);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
            chkptr.write_checkpoints().ok();
        }
        {
            let mut chkptr = Checkpointer::new(&data_dir.path());
            assert_eq!(chkptr.get_checkpoint(fingerprint), None);
            chkptr.read_checkpoints(None);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
        }
    }
}
