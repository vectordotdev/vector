use super::{fingerprinter::FileFingerprint, FilePosition};
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

    /// Encode a fingerprint to a file name, including legacy Unknown values
    ///
    /// For each of the non-legacy variants, prepend an identifier byte that falls outside of the
    /// hex range used by the legacy implementation. This allows them to be differentiated by
    /// simply peeking at the first byte.
    fn encode(&self, fng: FileFingerprint, pos: FilePosition) -> PathBuf {
        use FileFingerprint::*;

        let path = match fng {
            Checksum(c) => format!("g{:x}.{}", c, pos),
            FirstLineChecksum(c) => format!("h{:x}.{}", c, pos),
            DevInode(dev, ino) => format!("i{:x}.{:x}.{}", dev, ino, pos),
            Unknown(x) => format!("{:x}.{}", x, pos),
        };
        self.directory.join(path)
    }

    /// Decode a fingerprint from a file name, accounting for unknowns due to the legacy
    /// implementation.
    ///
    /// The trick here is to rely on the hex encoding of the legacy format. Because hex encoding
    /// only allows [0-9a-f], we can use any character outside of that range as a magic byte
    /// identifier for the newer formats.
    fn decode(&self, path: &Path) -> (FileFingerprint, FilePosition) {
        use FileFingerprint::*;

        let file_name = &path.file_name().unwrap().to_string_lossy();
        match file_name.chars().next().expect("empty file name") {
            'g' => {
                let (c, pos) = scan_fmt!(file_name, "g{x}.{}", [hex u64], FilePosition).unwrap();
                (Checksum(c), pos)
            }
            'h' => {
                let (c, pos) = scan_fmt!(file_name, "h{x}.{}", [hex u64], FilePosition).unwrap();
                (FirstLineChecksum(c), pos)
            }
            'i' => {
                let (dev, ino, pos) =
                    scan_fmt!(file_name, "i{x}.{y}.{}", [hex u64], [hex u64], FilePosition)
                        .unwrap();
                (DevInode(dev, ino), pos)
            }
            _ => {
                let (c, pos) = scan_fmt!(file_name, "{x}.{}", [hex u64], FilePosition).unwrap();
                (Unknown(c), pos)
            }
        }
    }

    pub fn set_checkpoint(&mut self, fng: FileFingerprint, pos: FilePosition) {
        self.checkpoints.insert(fng, pos);
    }

    pub fn get_checkpoint(&self, fng: FileFingerprint) -> Option<FilePosition> {
        self.checkpoints.get(&fng).cloned()
    }

    /// Scan through a given list of fresh fingerprints (i.e. not legacy Unknown) to see if any
    /// match an existing legacy fingerprint. If so, upgrade the existing fingerprint.
    pub fn maybe_upgrade(&mut self, fresh: impl Iterator<Item = FileFingerprint>) {
        for fng in fresh {
            if let Some(pos) = self
                .checkpoints
                .remove(&FileFingerprint::Unknown(fng.to_legacy()))
            {
                self.checkpoints.insert(fng, pos);
            }
        }
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
        let fingerprint: FileFingerprint = 0x1234567890abcdef.into();
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
        let fingerprint: FileFingerprint = 0x1234567890abcdef.into();
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

    #[test]
    fn test_checkpointer_upgrades() {
        let new_fingerprint = FileFingerprint::DevInode(1, 2);
        let old_fingerprint = FileFingerprint::Unknown(new_fingerprint.to_legacy());
        let position: FilePosition = 1234;

        let data_dir = tempdir().unwrap();
        {
            let mut chkptr = Checkpointer::new(&data_dir.path());
            chkptr.set_checkpoint(old_fingerprint, position);
            assert_eq!(chkptr.get_checkpoint(old_fingerprint), Some(position));
            chkptr.write_checkpoints().ok();
        }
        {
            let mut chkptr = Checkpointer::new(&data_dir.path());
            chkptr.read_checkpoints(None);
            assert_eq!(chkptr.get_checkpoint(new_fingerprint), None);

            chkptr.maybe_upgrade(std::iter::once(new_fingerprint));

            assert_eq!(chkptr.get_checkpoint(new_fingerprint), Some(position));
            assert_eq!(chkptr.get_checkpoint(old_fingerprint), None);
        }
    }
}
