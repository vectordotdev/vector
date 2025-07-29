use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use glob::glob;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::{FileFingerprint, Fingerprinter};

const TMP_FILE_NAME: &str = "checkpoints.new.json";
pub const CHECKPOINT_FILE_NAME: &str = "checkpoints.json";
const EXPIRATION_GRACE_DURATION: chrono::Duration = chrono::Duration::seconds(60);

/// This enum represents the file format of checkpoints persisted to disk. Right
/// now there is only one variant, but any incompatible changes will require and
/// additional variant to be added here and handled anywhere that we transit
/// this format.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "version", rename_all = "snake_case")]
enum State {
    #[serde(rename = "1")]
    V1 { checkpoints: BTreeSet<Checkpoint> },
}

/// A simple JSON-friendly struct of the fingerprint/position pair, since
/// fingerprints as objects cannot be keys in a plain JSON map.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
struct Checkpoint {
    fingerprint: FileFingerprint,
    position: u64,
    modified: DateTime<Utc>,
}

pub struct Checkpointer {
    directory: PathBuf,
    tmp_file_path: PathBuf,
    stable_file_path: PathBuf,
    glob_string: String,
    checkpoints: Arc<CheckpointsView>,
    last: Mutex<Option<State>>,
}

/// A thread-safe handle for reading and writing checkpoints in-memory across
/// multiple threads.
#[derive(Debug, Default)]
pub struct CheckpointsView {
    checkpoints: DashMap<FileFingerprint, u64>,
    modified_times: DashMap<FileFingerprint, DateTime<Utc>>,
    removed_times: DashMap<FileFingerprint, DateTime<Utc>>,
}

impl CheckpointsView {
    pub fn update(&self, fng: FileFingerprint, pos: u64) {
        self.checkpoints.insert(fng, pos);
        self.modified_times.insert(fng, Utc::now());
        self.removed_times.remove(&fng);
    }

    pub fn get(&self, fng: FileFingerprint) -> Option<u64> {
        self.checkpoints.get(&fng).map(|r| *r.value())
    }

    pub fn set_dead(&self, fng: FileFingerprint) {
        self.removed_times.insert(fng, Utc::now());
    }

    pub fn update_key(&self, old: FileFingerprint, new: FileFingerprint) {
        if let Some((_, value)) = self.checkpoints.remove(&old) {
            self.checkpoints.insert(new, value);
        }

        if let Some((_, value)) = self.modified_times.remove(&old) {
            self.modified_times.insert(new, value);
        }

        if let Some((_, value)) = self.removed_times.remove(&old) {
            self.removed_times.insert(new, value);
        }
    }

    pub fn contains_bytes_checksums(&self) -> bool {
        self.checkpoints
            .iter()
            .any(|entry| matches!(entry.key(), FileFingerprint::BytesChecksum(_)))
    }

    pub fn remove_expired(&self) {
        let now = Utc::now();

        // Collect all of the expired keys. Removing them while iterating can
        // lead to deadlocks, the set should be small, and this is not a
        // performance-sensitive path.
        let to_remove = self
            .removed_times
            .iter()
            .filter(|entry| {
                let ts = entry.value();
                let duration = now - *ts;
                duration >= EXPIRATION_GRACE_DURATION
            })
            .map(|entry| *entry.key())
            .collect::<Vec<FileFingerprint>>();

        for fng in to_remove {
            self.checkpoints.remove(&fng);
            self.modified_times.remove(&fng);
            self.removed_times.remove(&fng);
        }
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

    fn maybe_upgrade(
        &self,
        path: &Path,
        fng: FileFingerprint,
        fingerprinter: &Fingerprinter,
        fingerprint_buffer: &mut Vec<u8>,
    ) {
        if let Ok(Some(old_checksum)) = fingerprinter.get_bytes_checksum(path, fingerprint_buffer) {
            self.update_key(old_checksum, fng)
        }

        // For the Unknown fingerprint, we need to check if there's a legacy fingerprint
        // that matches the new fingerprint's legacy value
        let legacy_value = fng.as_legacy();
        if let Some((_, pos)) = self
            .checkpoints
            .remove(&FileFingerprint::Unknown(legacy_value))
        {
            self.update(fng, pos);
        }

        if self.checkpoints.get(&fng).is_none() {
            if let Ok(Some(fingerprint)) =
                fingerprinter.get_legacy_checksum(path, fingerprint_buffer)
            {
                if let Some((_, pos)) = self.checkpoints.remove(&fingerprint) {
                    self.update(fng, pos);
                }
            }
            if let Ok(Some(fingerprint)) =
                fingerprinter.get_legacy_first_lines_checksum(path, fingerprint_buffer)
            {
                if let Some((_, pos)) = self.checkpoints.remove(&fingerprint) {
                    self.update(fng, pos);
                }
            }
        }
    }
}

impl Checkpointer {
    pub fn new(data_dir: &Path) -> Checkpointer {
        let directory = data_dir.join("checkpoints");
        let glob_string = directory.join("*").to_string_lossy().into_owned();
        let tmp_file_path = data_dir.join(TMP_FILE_NAME);
        let stable_file_path = data_dir.join(CHECKPOINT_FILE_NAME);

        Checkpointer {
            directory,
            glob_string,
            tmp_file_path,
            stable_file_path,
            checkpoints: Arc::new(CheckpointsView::default()),
            last: Mutex::new(None),
        }
    }

    pub fn view(&self) -> Arc<CheckpointsView> {
        Arc::clone(&self.checkpoints)
    }

    /// Encode a fingerprint to a file name, including legacy Unknown values
    ///
    /// For each of the non-legacy variants, prepend an identifier byte that
    /// falls outside of the hex range used by the legacy implementation. This
    /// allows them to be differentiated by simply peeking at the first byte.
    #[cfg(test)]
    fn encode(&self, fng: FileFingerprint, pos: u64) -> PathBuf {
        use FileFingerprint::*;

        let path = match fng {
            BytesChecksum(c) => format!("g{c:x}.{pos}"),
            FirstLinesChecksum(c) => format!("h{c:x}.{pos}"),
            DevInode(dev, ino) => format!("i{dev:x}.{ino:x}.{pos}"),
            Unknown(x) => format!("{x:x}.{pos}"),
        };
        self.directory.join(path)
    }

    /// Decode a fingerprint from a file name, accounting for unknowns due to the legacy
    /// implementation.
    ///
    /// The trick here is to rely on the hex encoding of the legacy
    /// format. Because hex encoding only allows [0-9a-f], we can use any
    /// character outside of that range as a magic byte identifier for the newer
    /// formats.
    fn decode(&self, path: &Path) -> (FileFingerprint, u64) {
        use FileFingerprint::*;

        let file_name = &path.file_name().unwrap().to_string_lossy();
        match file_name.chars().next().expect("empty file name") {
            'g' => {
                let (c, pos) = scan_fmt!(file_name, "g{x}.{}", [hex u64], u64).unwrap();
                (BytesChecksum(c), pos)
            }
            'h' => {
                let (c, pos) = scan_fmt!(file_name, "h{x}.{}", [hex u64], u64).unwrap();
                (FirstLinesChecksum(c), pos)
            }
            'i' => {
                let (dev, ino, pos) =
                    scan_fmt!(file_name, "i{x}.{x}.{}", [hex u64], [hex u64], u64).unwrap();
                (DevInode(dev, ino), pos)
            }
            _ => {
                let (c, pos) = scan_fmt!(file_name, "{x}.{}", [hex u64], u64).unwrap();
                (Unknown(c), pos)
            }
        }
    }

    #[cfg(test)]
    pub fn update_checkpoint(&mut self, fng: FileFingerprint, pos: u64) {
        self.checkpoints.update(fng, pos);
    }

    #[cfg(test)]
    pub fn get_checkpoint(&self, fng: FileFingerprint) -> Option<u64> {
        self.checkpoints.get(fng)
    }

    /// Scan through a given list of fresh fingerprints to see if any match an existing legacy
    /// fingerprint. If so, upgrade the existing fingerprint.
    pub fn maybe_upgrade(
        &mut self,
        path: &Path,
        fresh: FileFingerprint,
        fingerprinter: &Fingerprinter,
        fingerprint_buffer: &mut Vec<u8>,
    ) {
        self.checkpoints
            .maybe_upgrade(path, fresh, fingerprinter, fingerprint_buffer)
    }

    /// Persist the current checkpoints state to disk, making our best effort to
    /// do so in an atomic way that allow for recovering the previous state in
    /// the event of a crash.
    pub fn write_checkpoints(&self) -> Result<usize, io::Error> {
        // First drop any checkpoints for files that were removed more than 60
        // seconds ago. This keeps our working set as small as possible and
        // makes sure we don't spend time and IO writing checkpoints that don't
        // matter anymore.
        self.checkpoints.remove_expired();

        let current = self.checkpoints.get_state();

        // Fetch last written state.
        let mut last = self.last.lock().expect("Data poisoned.");
        if last.as_ref() != Some(&current) {
            // Write the new checkpoints to a tmp file and flush it fully to
            // disk. If vector dies anywhere during this section, the existing
            // stable file will still be in its current valid state and we'll be
            // able to recover.
            let mut f = io::BufWriter::new(fs::File::create(&self.tmp_file_path)?);
            serde_json::to_writer(&mut f, &current)?;
            f.into_inner()?.sync_all()?;

            // Once the temp file is fully flushed, rename the tmp file to replace
            // the previous stable file. This is an atomic operation on POSIX
            // systems (and the stdlib claims to provide equivalent behavior on
            // Windows), which should prevent scenarios where we don't have at least
            // one full valid file to recover from.
            fs::rename(&self.tmp_file_path, &self.stable_file_path)?;

            *last = Some(current);
        }

        Ok(self.checkpoints.checkpoints.len())
    }

    /// Write checkpoints to disk in the legacy format. Used for compatibility
    /// testing only.
    #[cfg(test)]
    pub fn write_legacy_checkpoints(&mut self) -> Result<usize, io::Error> {
        fs::remove_dir_all(&self.directory).ok();
        fs::create_dir_all(&self.directory)?;
        for c in self.checkpoints.checkpoints.iter() {
            fs::File::create(self.encode(*c.key(), *c.value()))?;
        }
        Ok(self.checkpoints.checkpoints.len())
    }

    /// Read persisted checkpoints from disk, preferring the new JSON file
    /// format but falling back to the legacy system when those files are found
    /// instead.
    pub fn read_checkpoints(&mut self, ignore_before: Option<DateTime<Utc>>) {
        // First try reading from the tmp file location. If this works, it means
        // that the previous process was interrupted in the process of
        // checkpointing and the tmp file should contain more recent data that
        // should be preferred.
        match self.read_checkpoints_file(&self.tmp_file_path) {
            Ok(state) => {
                warn!(message = "Recovered checkpoint data from interrupted process.");
                self.checkpoints.set_state(state, ignore_before);

                // Try to move this tmp file to the stable location so we don't
                // immediately overwrite it when we next persist checkpoints.
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

        // Next, attempt to read checkpoints from the stable file location. This
        // is the expected location, so warn more aggressively if something goes
        // wrong.
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

        // If we haven't returned yet, go ahead and look for the legacy files
        // and try to read them.
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
    use chrono::{Duration, Utc};
    use similar_asserts::assert_eq;
    use std::io::Write;
    use tempfile::tempdir;

    use super::{Checkpoint, Checkpointer, CHECKPOINT_FILE_NAME, TMP_FILE_NAME};
    use crate::fingerprinter::FINGERPRINT_CRC;
    use crate::{FileFingerprint, FilePosition, FingerprintStrategy, Fingerprinter};

    #[test]
    fn test_checkpointer_basics() {
        let fingerprints = vec![
            FileFingerprint::DevInode(1, 2),
            FileFingerprint::BytesChecksum(3456),
            FileFingerprint::FirstLinesChecksum(78910),
            FileFingerprint::Unknown(1337),
        ];
        for fingerprint in fingerprints {
            let position: FilePosition = 1234;
            let data_dir = tempdir().unwrap();
            let mut chkptr = Checkpointer::new(data_dir.path());
            assert_eq!(
                chkptr.decode(&chkptr.encode(fingerprint, position)),
                (fingerprint, position)
            );
            chkptr.update_checkpoint(fingerprint, position);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
        }
    }

    #[test]
    fn test_checkpointer_ignore_before() {
        let newer = (
            FileFingerprint::DevInode(1, 2),
            Utc::now() - Duration::seconds(5),
        );
        let newish = (
            FileFingerprint::BytesChecksum(3456),
            Utc::now() - Duration::seconds(10),
        );
        let oldish = (
            FileFingerprint::FirstLinesChecksum(78910),
            Utc::now() - Duration::seconds(15),
        );
        let older = (
            FileFingerprint::Unknown(1337),
            Utc::now() - Duration::seconds(20),
        );
        let ignore_before = Some(Utc::now() - Duration::seconds(12));

        let position: u64 = 1234;
        let data_dir = tempdir().unwrap();

        // load and persist the checkpoints
        {
            let chkptr = Checkpointer::new(data_dir.path());

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
            let mut chkptr = Checkpointer::new(data_dir.path());
            chkptr.read_checkpoints(ignore_before);

            assert_eq!(chkptr.get_checkpoint(newish.0), Some(position));
            assert_eq!(chkptr.get_checkpoint(newer.0), Some(position));
            assert_eq!(chkptr.get_checkpoint(oldish.0), None);
            assert_eq!(chkptr.get_checkpoint(older.0), None);
        }
    }

    #[test]
    fn test_checkpointer_expiration() {
        let fingerprint = FileFingerprint::DevInode(1, 2);
        let position: u64 = 1234;
        let data_dir = tempdir().unwrap();
        let mut chkptr = Checkpointer::new(data_dir.path());

        chkptr.update_checkpoint(fingerprint, position);
        assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));

        chkptr.checkpoints.set_dead(fingerprint);
        chkptr.checkpoints.remove_expired();
        assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));

        // Hack the timestamp to be in the past
        let now = Utc::now();
        let past = now - Duration::seconds(61);
        chkptr.checkpoints.removed_times.insert(fingerprint, past);

        chkptr.checkpoints.remove_expired();
        assert_eq!(chkptr.get_checkpoint(fingerprint), None);
    }

    #[test]
    fn test_checkpointer_serialization() {
        let fingerprint = FileFingerprint::DevInode(1, 2);
        let position: u64 = 1234;
        let data_dir = tempdir().unwrap();
        let mut chkptr = Checkpointer::new(data_dir.path());

        chkptr.update_checkpoint(fingerprint, position);
        assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));

        chkptr.write_checkpoints().unwrap();
        assert!(data_dir.path().join(CHECKPOINT_FILE_NAME).exists());
        assert!(!data_dir.path().join(TMP_FILE_NAME).exists());
    }

    #[test]
    fn test_checkpointer_deserialization() {
        let fingerprint = FileFingerprint::DevInode(1, 2);
        let position: u64 = 1234;
        let data_dir = tempdir().unwrap();

        // load and persist the checkpoints
        {
            let mut chkptr = Checkpointer::new(data_dir.path());
            chkptr.update_checkpoint(fingerprint, position);
            chkptr.write_checkpoints().unwrap();
        }

        // read them back
        {
            let mut chkptr = Checkpointer::new(data_dir.path());
            chkptr.read_checkpoints(None);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
        }
    }

    #[test]
    fn test_checkpointer_restart() {
        let fingerprint = FileFingerprint::DevInode(1, 2);
        let position: u64 = 1234;
        let data_dir = tempdir().unwrap();

        // load and persist the checkpoints
        {
            let mut chkptr = Checkpointer::new(data_dir.path());
            chkptr.update_checkpoint(fingerprint, position);
            chkptr.write_checkpoints().unwrap();
        }

        // simulate a crash by writing to the tmp file
        {
            let mut chkptr = Checkpointer::new(data_dir.path());
            chkptr.update_checkpoint(fingerprint, position + 1);
            let current = chkptr.checkpoints.get_state();
            let f = std::io::BufWriter::new(std::fs::File::create(&chkptr.tmp_file_path).unwrap());
            serde_json::to_writer(f, &current).unwrap();
        }

        // read them back and assert we get the tmp file
        {
            let mut chkptr = Checkpointer::new(data_dir.path());
            chkptr.read_checkpoints(None);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position + 1));
        }
    }

    #[test]
    fn test_checkpointer_file_upgrades() {
        let fingerprint = FileFingerprint::DevInode(1, 2);
        let position: u64 = 1234;
        let data_dir = tempdir().unwrap();
        let mut chkptr = Checkpointer::new(data_dir.path());

        chkptr.update_checkpoint(fingerprint, position);
        assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));

        let fingerprint2 = FileFingerprint::DevInode(3, 4);
        chkptr.checkpoints.update_key(fingerprint, fingerprint2);
        assert_eq!(chkptr.get_checkpoint(fingerprint), None);
        assert_eq!(chkptr.get_checkpoint(fingerprint2), Some(position));
    }

    #[test]
    fn test_checkpointer_fingerprint_upgrades_unknown() {
        let fingerprint = FileFingerprint::Unknown(1337);
        let position: u64 = 1234;
        let data_dir = tempdir().unwrap();
        let mut chkptr = Checkpointer::new(data_dir.path());

        chkptr.update_checkpoint(fingerprint, position);
        assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));

        // Create a DevInode fingerprint that has the same legacy value as the Unknown fingerprint
        // This is what we're testing - that an Unknown fingerprint can be upgraded to a DevInode
        // fingerprint if they have the same legacy value
        let dev: u64 = 1;
        let ino: u64 = 2;
        let mut buf = Vec::with_capacity(std::mem::size_of_val(&dev) * 2);
        buf.write_all(&dev.to_be_bytes()).expect("writing to array");
        buf.write_all(&ino.to_be_bytes()).expect("writing to array");
        let legacy_value = FINGERPRINT_CRC.checksum(&buf[..]);

        // Create a new Unknown fingerprint with the same legacy value
        let fingerprint = FileFingerprint::Unknown(legacy_value);
        chkptr.update_checkpoint(fingerprint, position);

        // Now create the DevInode fingerprint with the same values
        let fingerprint2 = FileFingerprint::DevInode(dev, ino);
        let mut buffer = Vec::new();
        let fingerprinter = Fingerprinter::new(FingerprintStrategy::DevInode);
        let path = data_dir.path().join("test");
        std::fs::write(&path, "test").unwrap();

        // This should upgrade the Unknown fingerprint to the DevInode fingerprint
        chkptr.maybe_upgrade(&path, fingerprint2, &fingerprinter, &mut buffer);

        // The Unknown fingerprint should be gone, and the DevInode fingerprint should have the position
        assert_eq!(chkptr.get_checkpoint(fingerprint), None);
        assert_eq!(chkptr.get_checkpoint(fingerprint2), Some(position));
    }

    #[test]
    fn test_checkpointer_fingerprint_upgrades_legacy_checksum() {
        let fingerprint = FileFingerprint::BytesChecksum(1337);
        let position: u64 = 1234;
        let data_dir = tempdir().unwrap();
        let mut chkptr = Checkpointer::new(data_dir.path());

        chkptr.update_checkpoint(fingerprint, position);
        assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));

        let fingerprint2 = FileFingerprint::DevInode(1, 2);
        let mut buffer = Vec::new();
        let fingerprinter = Fingerprinter::new(FingerprintStrategy::DevInode);
        let path = data_dir.path().join("test");
        std::fs::write(&path, "test").unwrap();
        chkptr.maybe_upgrade(&path, fingerprint2, &fingerprinter, &mut buffer);
        assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
        assert_eq!(chkptr.get_checkpoint(fingerprint2), None);
    }

    #[test]
    fn test_checkpointer_fingerprint_upgrades_legacy_first_lines_checksum() {
        let fingerprint = FileFingerprint::FirstLinesChecksum(1337);
        let position: u64 = 1234;
        let data_dir = tempdir().unwrap();
        let mut chkptr = Checkpointer::new(data_dir.path());

        chkptr.update_checkpoint(fingerprint, position);
        assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));

        let fingerprint2 = FileFingerprint::DevInode(1, 2);
        let mut buffer = Vec::new();
        let fingerprinter = Fingerprinter::new(FingerprintStrategy::DevInode);
        let path = data_dir.path().join("test");
        std::fs::write(&path, "test").unwrap();
        chkptr.maybe_upgrade(&path, fingerprint2, &fingerprinter, &mut buffer);
        assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
        assert_eq!(chkptr.get_checkpoint(fingerprint2), None);
    }
}
