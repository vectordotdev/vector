use super::{fingerprinter::FileFingerprint, FilePosition};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use glob::glob;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

const TMP_FILE_NAME: &str = "checkpoints.new.json";
const STABLE_FILE_NAME: &str = "checkpoints.json";

/// This enum represents the file format of checkpoints persisted to disk. Right now there is only
/// one variant, but any incompatible changes will require and additional variant to be added here
/// and handled anywhere that we transit this format.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "version", rename_all = "snake_case")]
enum State {
    #[serde(rename = "1")]
    V1 { checkpoints: Vec<Checkpoint> },
}

/// A simple JSON-friendly struct of the fingerprint/position pair, since fingerprints as objects
/// cannot be keys in a plain JSON map.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct Checkpoint {
    fingerprint: FileFingerprint,
    position: FilePosition,
    modified: DateTime<Utc>,
}

pub struct Checkpointer {
    directory: PathBuf,
    tmp_file_path: PathBuf,
    stable_file_path: PathBuf,
    glob_string: String,
    checkpoints: Arc<CheckpointsView>,
}

/// A thread-safe handle for reading and writing checkpoints in-memory across multiple threads.
#[derive(Debug, Default)]
pub struct CheckpointsView {
    checkpoints: DashMap<FileFingerprint, FilePosition>,
    modified_times: DashMap<FileFingerprint, DateTime<Utc>>,
}

impl CheckpointsView {
    pub fn update(&self, fng: FileFingerprint, pos: FilePosition) {
        self.checkpoints.insert(fng, pos);
        self.modified_times.insert(fng, Utc::now());
    }

    pub fn get(&self, fng: FileFingerprint) -> Option<FilePosition> {
        self.checkpoints.get(&fng).map(|r| *r.value())
    }

    fn load(&self, checkpoint: Checkpoint) {
        self.checkpoints
            .insert(checkpoint.fingerprint, checkpoint.position);
        self.modified_times
            .insert(checkpoint.fingerprint, checkpoint.modified);
    }

    fn set_state(&self, state: State, ignore_before: Option<DateTime<Utc>>) {
        match state {
            State::V1 { checkpoints } => {
                for checkpoint in checkpoints {
                    if let Some(ignore_before) = ignore_before {
                        if checkpoint.modified < ignore_before {
                            continue;
                        }
                    }
                    self.load(checkpoint);
                }
            }
        }
    }

    fn get_state(&self) -> State {
        State::V1 {
            checkpoints: self
                .checkpoints
                .iter()
                .map(|entry| {
                    let fingerprint = entry.key();
                    let position = entry.value();
                    Checkpoint {
                        fingerprint: *fingerprint,
                        position: *position,
                        modified: self
                            .modified_times
                            .get(fingerprint)
                            .map(|r| *r.value())
                            .unwrap_or_else(Utc::now),
                    }
                })
                .collect(),
        }
    }

    fn maybe_upgrade(&self, fresh: impl Iterator<Item = FileFingerprint>) {
        for fng in fresh {
            if let Some((_, pos)) = self
                .checkpoints
                .remove(&FileFingerprint::Unknown(fng.to_legacy()))
            {
                self.update(fng, pos);
            }
        }
    }
}

impl Checkpointer {
    pub fn new(data_dir: &Path) -> Checkpointer {
        let directory = data_dir.join("checkpoints");
        let glob_string = directory.join("*").to_string_lossy().into_owned();
        let tmp_file_path = data_dir.join(TMP_FILE_NAME);
        let stable_file_path = data_dir.join(STABLE_FILE_NAME);

        Checkpointer {
            directory,
            glob_string,
            tmp_file_path,
            stable_file_path,
            checkpoints: Arc::new(CheckpointsView::default()),
        }
    }

    pub fn view(&self) -> Arc<CheckpointsView> {
        Arc::clone(&self.checkpoints)
    }

    /// Encode a fingerprint to a file name, including legacy Unknown values
    ///
    /// For each of the non-legacy variants, prepend an identifier byte that falls outside of the
    /// hex range used by the legacy implementation. This allows them to be differentiated by
    /// simply peeking at the first byte.
    #[cfg(test)]
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
                    scan_fmt!(file_name, "i{x}.{x}.{}", [hex u64], [hex u64], FilePosition)
                        .unwrap();
                (DevInode(dev, ino), pos)
            }
            _ => {
                let (c, pos) = scan_fmt!(file_name, "{x}.{}", [hex u64], FilePosition).unwrap();
                (Unknown(c), pos)
            }
        }
    }

    #[cfg(test)]
    pub fn update_checkpoint(&mut self, fng: FileFingerprint, pos: FilePosition) {
        self.checkpoints.update(fng, pos);
    }

    #[cfg(test)]
    pub fn get_checkpoint(&self, fng: FileFingerprint) -> Option<FilePosition> {
        self.checkpoints.get(fng)
    }

    /// Scan through a given list of fresh fingerprints (i.e. not legacy Unknown) to see if any
    /// match an existing legacy fingerprint. If so, upgrade the existing fingerprint.
    pub fn maybe_upgrade(&mut self, fresh: impl Iterator<Item = FileFingerprint>) {
        self.checkpoints.maybe_upgrade(fresh)
    }

    /// Persist the current checkpoints state to disk, making our best effort to do so in an atomic
    /// way that allow for recovering the previous state in the event of a crash.
    pub fn write_checkpoints(&self) -> Result<usize, io::Error> {
        // First write the new checkpoints to a tmp file and flush it fully to disk. If vector
        // dies anywhere during this section, the existing stable file will still be in its current
        // valid state and we'll be able to recover.
        let mut f = io::BufWriter::new(fs::File::create(&self.tmp_file_path)?);
        serde_json::to_writer(&mut f, &self.checkpoints.get_state())?;
        f.into_inner()?.sync_all()?;

        // Once the temp file is fully flushed, rename the tmp file to replace the previous stable
        // file. This is an atomic operation on POSIX systems (and the stdlib claims to provide
        // equivalent behavior on Windows), which should prevent scenarios where we don't have at
        // least one full valid file to recover from.
        fs::rename(&self.tmp_file_path, &self.stable_file_path)?;

        Ok(self.checkpoints.checkpoints.len())
    }

    /// Write checkpoints to disk in the legacy format. Used for compatibility testing only.
    #[cfg(test)]
    pub fn write_legacy_checkpoints(&mut self) -> Result<usize, io::Error> {
        fs::remove_dir_all(&self.directory).ok();
        fs::create_dir_all(&self.directory)?;
        for c in self.checkpoints.checkpoints.iter() {
            fs::File::create(self.encode(*c.key(), *c.value()))?;
        }
        Ok(self.checkpoints.checkpoints.len())
    }

    /// Read persisted checkpoints from disk, preferring the new JSON file format but falling back
    /// to the legacy system when those files are found instead.
    pub fn read_checkpoints(&mut self, ignore_before: Option<DateTime<Utc>>) {
        // First try reading from the tmp file location. If this works, it means that the previous
        // process was interrupted in the process of checkpointing and the tmp file should contain
        // more recent data that should be preferred.
        match self.read_checkpoints_file(&self.tmp_file_path) {
            Ok(state) => {
                warn!(message = "Recovered checkpoint data from interrupted process.");
                self.checkpoints.set_state(state, ignore_before);

                // Try to move this tmp file to the stable location so we don't immediately overwrite
                // it when we next persist checkpoints.
                if let Err(error) = fs::rename(&self.tmp_file_path, &self.stable_file_path) {
                    warn!(message = "Error persisting recovered checkpoint file.", %error);
                }
                return;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // This is expected, so no warning needed
            }
            Err(error) => {
                error!(message = "Unable to recover checkpoint data from interrupted process.", %error);
            }
        }

        // Next, attempt to read checkpoints from the stable file location. This is the
        // expected location, so warn more aggressively if something goes wrong.
        match self.read_checkpoints_file(&self.stable_file_path) {
            Ok(state) => {
                info!(message = "Loaded checkpoint data.");
                self.checkpoints.set_state(state, ignore_before);
                return;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // This is expected, so no warning needed
            }
            Err(error) => {
                warn!(message = "Unable to load checkpoint data.", %error);
                return;
            }
        }

        // If we haven't returned yet, go ahead and look for the legacy files and try to read them.
        info!("Attempting to read legacy checkpoint files.");
        self.read_legacy_checkpoints(ignore_before);

        if self.write_checkpoints().is_ok() {
            fs::remove_dir_all(&self.directory).ok();
        }
    }

    fn read_checkpoints_file(&self, path: &Path) -> Result<State, io::Error> {
        let reader = io::BufReader::new(fs::File::open(path)?);
        serde_json::from_reader(reader).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn read_legacy_checkpoints(&mut self, ignore_before: Option<DateTime<Utc>>) {
        for path in glob(&self.glob_string).unwrap().flatten() {
            let mut mtime = None;
            if let Some(ignore_before) = ignore_before {
                if let Ok(Ok(modified)) = fs::metadata(&path).map(|metadata| metadata.modified()) {
                    let modified = DateTime::<Utc>::from(modified);
                    if modified < ignore_before {
                        fs::remove_file(path).ok();
                        continue;
                    }
                    mtime = Some(modified);
                }
            }
            let (fng, pos) = self.decode(&path);
            self.checkpoints.checkpoints.insert(fng, pos);
            if let Some(mtime) = mtime {
                self.checkpoints.modified_times.insert(fng, mtime);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::{
        Checkpoint, Checkpointer, FileFingerprint, FilePosition, STABLE_FILE_NAME, TMP_FILE_NAME,
    };
    use chrono::{Duration, Utc};
    use tempfile::tempdir;

    #[test]
    fn test_checkpointer_basics() {
        let fingerprints = vec![
            FileFingerprint::DevInode(1, 2),
            FileFingerprint::Checksum(3456),
            FileFingerprint::FirstLineChecksum(78910),
            FileFingerprint::Unknown(1337),
        ];
        for fingerprint in fingerprints {
            let position: FilePosition = 1234;
            let data_dir = tempdir().unwrap();
            let mut chkptr = Checkpointer::new(&data_dir.path());
            assert_eq!(
                chkptr.decode(&chkptr.encode(fingerprint, position)),
                (fingerprint, position)
            );
            chkptr.update_checkpoint(fingerprint, position);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
        }
    }

    #[test]
    fn test_checkpointer_expiration() {
        let newer = (
            FileFingerprint::DevInode(1, 2),
            Utc::now() - Duration::seconds(5),
        );
        let newish = (
            FileFingerprint::Checksum(3456),
            Utc::now() - Duration::seconds(10),
        );
        let oldish = (
            FileFingerprint::FirstLineChecksum(78910),
            Utc::now() - Duration::seconds(15),
        );
        let older = (
            FileFingerprint::Unknown(1337),
            Utc::now() - Duration::seconds(20),
        );
        let ignore_before = Some(Utc::now() - Duration::seconds(12));

        let position: FilePosition = 1234;
        let data_dir = tempdir().unwrap();

        // load and persist the checkpoints
        {
            let chkptr = Checkpointer::new(&data_dir.path());

            for (fingerprint, modified) in &[&newer, &newish, &oldish, &older] {
                chkptr.checkpoints.load(Checkpoint {
                    fingerprint: *fingerprint,
                    position,
                    modified: *modified,
                });
                assert_eq!(chkptr.get_checkpoint(*fingerprint), Some(position));
                chkptr.write_checkpoints().unwrap();
            }
        }

        // read them back and assert old are removed
        {
            let mut chkptr = Checkpointer::new(&data_dir.path());
            chkptr.read_checkpoints(ignore_before);

            assert_eq!(chkptr.get_checkpoint(newish.0), Some(position));
            assert_eq!(chkptr.get_checkpoint(newer.0), Some(position));
            assert_eq!(chkptr.get_checkpoint(oldish.0), None);
            assert_eq!(chkptr.get_checkpoint(older.0), None);
        }
    }

    #[test]
    fn test_checkpointer_restart() {
        let fingerprints = vec![
            FileFingerprint::DevInode(1, 2),
            FileFingerprint::Checksum(3456),
            FileFingerprint::FirstLineChecksum(78910),
            FileFingerprint::Unknown(1337),
        ];
        for fingerprint in fingerprints {
            let position: FilePosition = 1234;
            let data_dir = tempdir().unwrap();
            {
                let mut chkptr = Checkpointer::new(&data_dir.path());
                chkptr.update_checkpoint(fingerprint, position);
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

    #[test]
    fn test_checkpointer_fingerprint_upgrades() {
        let new_fingerprint = FileFingerprint::DevInode(1, 2);
        let old_fingerprint = FileFingerprint::Unknown(new_fingerprint.to_legacy());
        let position: FilePosition = 1234;

        let data_dir = tempdir().unwrap();
        {
            let mut chkptr = Checkpointer::new(&data_dir.path());
            chkptr.update_checkpoint(old_fingerprint, position);
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

    #[test]
    fn test_checkpointer_file_upgrades() {
        let fingerprint = FileFingerprint::DevInode(1, 2);
        let position: FilePosition = 1234;

        let data_dir = tempdir().unwrap();

        // Write out checkpoints in the legacy file format
        {
            let mut chkptr = Checkpointer::new(&data_dir.path());
            chkptr.update_checkpoint(fingerprint, position);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
            chkptr.write_legacy_checkpoints().unwrap();
        }

        // Ensure that the new files were not written but the old style of files were
        assert_eq!(false, data_dir.path().join(TMP_FILE_NAME).exists());
        assert_eq!(false, data_dir.path().join(STABLE_FILE_NAME).exists());
        assert_eq!(true, data_dir.path().join("checkpoints").is_dir());

        // Read from those old files, ensure the checkpoints were loaded properly, and then write
        // them normally (i.e. in the new format)
        {
            let mut chkptr = Checkpointer::new(&data_dir.path());
            chkptr.read_checkpoints(None);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
            chkptr.write_checkpoints().unwrap();
        }

        // Ensure that the stable file is present, the tmp file is not, and the legacy files have
        // been cleaned up
        assert_eq!(false, data_dir.path().join(TMP_FILE_NAME).exists());
        assert_eq!(true, data_dir.path().join(STABLE_FILE_NAME).exists());
        assert_eq!(false, data_dir.path().join("checkpoints").is_dir());

        // Ensure one last time that we can reread from the new files and get the same result
        {
            let mut chkptr = Checkpointer::new(&data_dir.path());
            chkptr.read_checkpoints(None);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
        }
    }
}
