use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, BufReader},
    sync::Mutex,
};
use tracing::{error, info, warn};

use super::{FilePosition, fingerprinter::FileFingerprint};

const TMP_FILE_NAME: &str = "checkpoints.new.json";
pub const CHECKPOINT_FILE_NAME: &str = "checkpoints.json";

/// This enum represents the file format of checkpoints persisted to disk. Right
/// now there is only one variant, but any incompatible changes will require and
/// additional variant to be added here and handled anywhere that we transit
/// this format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "version", rename_all = "snake_case")]
enum State {
    #[serde(rename = "1")]
    V1 { checkpoints: BTreeSet<Checkpoint> },
}

/// A simple JSON-friendly struct of the fingerprint/position pair, since
/// fingerprints as objects cannot be keys in a plain JSON map.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
struct Checkpoint {
    fingerprint: FileFingerprint,
    position: FilePosition,
    modified: DateTime<Utc>,
}

pub struct Checkpointer {
    tmp_file_path: PathBuf,
    stable_file_path: PathBuf,
    checkpoints: Arc<CheckpointsView>,
    last: Mutex<Option<State>>,
}

/// A thread-safe handle for reading and writing checkpoints in-memory across
/// multiple threads.
#[derive(Debug, Default)]
pub struct CheckpointsView {
    checkpoints: DashMap<FileFingerprint, FilePosition>,
    modified_times: DashMap<FileFingerprint, DateTime<Utc>>,
    removed_times: DashMap<FileFingerprint, DateTime<Utc>>,
}

impl CheckpointsView {
    pub fn update(&self, fng: FileFingerprint, pos: FilePosition) {
        self.checkpoints.insert(fng, pos);
        self.modified_times.insert(fng, Utc::now());
        self.removed_times.remove(&fng);
    }

    pub fn get(&self, fng: FileFingerprint) -> Option<FilePosition> {
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
                duration >= chrono::Duration::seconds(60)
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
                    if let Some(ignore_before) = ignore_before
                        && checkpoint.modified < ignore_before
                    {
                        continue;
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
}

impl Checkpointer {
    pub fn new(data_dir: &Path) -> Checkpointer {
        let tmp_file_path = data_dir.join(TMP_FILE_NAME);
        let stable_file_path = data_dir.join(CHECKPOINT_FILE_NAME);

        Checkpointer {
            tmp_file_path,
            stable_file_path,
            checkpoints: Arc::new(CheckpointsView::default()),
            last: Mutex::new(None),
        }
    }

    pub fn view(&self) -> Arc<CheckpointsView> {
        Arc::clone(&self.checkpoints)
    }

    #[cfg(test)]
    pub fn update_checkpoint(&mut self, fng: FileFingerprint, pos: FilePosition) {
        self.checkpoints.update(fng, pos);
    }

    #[cfg(test)]
    pub fn get_checkpoint(&self, fng: FileFingerprint) -> Option<FilePosition> {
        self.checkpoints.get(fng)
    }

    /// Persist the current checkpoints state to disk, making our best effort to
    /// do so in an atomic way that allow for recovering the previous state in
    /// the event of a crash.
    pub async fn write_checkpoints(&self) -> Result<usize, io::Error> {
        // First drop any checkpoints for files that were removed more than 60
        // seconds ago. This keeps our working set as small as possible and
        // makes sure we don't spend time and IO writing checkpoints that don't
        // matter anymore.
        self.checkpoints.remove_expired();

        let current = self.checkpoints.get_state();

        // Fetch last written state.
        let mut last = self.last.lock().await;
        if last.as_ref() != Some(&current) {
            // Write the new checkpoints to a tmp file and flush it fully to
            // disk. If vector dies anywhere during this section, the existing
            // stable file will still be in its current valid state and we'll be
            // able to recover.
            let tmp_file_path = self.tmp_file_path.clone();

            // spawn_blocking shouldn't be needed: https://github.com/vectordotdev/vector/issues/23743
            let current = tokio::task::spawn_blocking(move || -> Result<State, io::Error> {
                let mut f = std::io::BufWriter::new(std::fs::File::create(tmp_file_path)?);
                serde_json::to_writer(&mut f, &current)?;
                f.into_inner()?.sync_all()?;
                Ok(current)
            })
            .await
            .map_err(io::Error::other)??;

            // Once the temp file is fully flushed, rename the tmp file to replace
            // the previous stable file. This is an atomic operation on POSIX
            // systems (and the stdlib claims to provide equivalent behavior on
            // Windows), which should prevent scenarios where we don't have at least
            // one full valid file to recover from.
            fs::rename(&self.tmp_file_path, &self.stable_file_path).await?;

            *last = Some(current);
        }

        Ok(self.checkpoints.checkpoints.len())
    }

    /// Read persisted checkpoints from disk, preferring the new JSON file
    /// format but falling back to the legacy system when those files are found
    /// instead.
    pub async fn read_checkpoints(&mut self, ignore_before: Option<DateTime<Utc>>) {
        // First try reading from the tmp file location. If this works, it means
        // that the previous process was interrupted in the process of
        // checkpointing and the tmp file should contain more recent data that
        // should be preferred.
        match self.read_checkpoints_file(&self.tmp_file_path).await {
            Ok(state) => {
                warn!(message = "Recovered checkpoint data from interrupted process.");
                self.checkpoints.set_state(state, ignore_before);

                // Try to move this tmp file to the stable location so we don't
                // immediately overwrite it when we next persist checkpoints.
                if let Err(error) = fs::rename(&self.tmp_file_path, &self.stable_file_path).await {
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
        match self.read_checkpoints_file(&self.stable_file_path).await {
            Ok(state) => {
                info!(message = "Loaded checkpoint data.");
                self.checkpoints.set_state(state, ignore_before);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // This is expected, so no warning needed
            }
            Err(error) => {
                warn!(message = "Unable to load checkpoint data.", %error);
            }
        }
    }

    async fn read_checkpoints_file(&self, path: &Path) -> Result<State, io::Error> {
        // Possible optimization: mmap the file into a slice and pass it into serde_json instead of
        // calling read_to_end. Need to investigate if this would work with tokio::fs::File

        let mut reader = BufReader::new(File::open(path).await?);
        let mut output = Vec::new();
        reader.read_to_end(&mut output).await?;

        serde_json::from_slice(&output[..])
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod test {
    use chrono::{Duration, Utc};
    use similar_asserts::assert_eq;
    use tempfile::tempdir;
    use tokio::fs;

    use super::{
        CHECKPOINT_FILE_NAME, Checkpoint, Checkpointer, FileFingerprint, FilePosition,
        TMP_FILE_NAME,
    };

    #[test]
    fn test_checkpointer_basics() {
        let fingerprints = vec![
            FileFingerprint::DevInode(1, 2),
            FileFingerprint::FirstLinesChecksum(78910),
        ];
        for fingerprint in fingerprints {
            let position: FilePosition = 1234;
            let data_dir = tempdir().unwrap();
            let mut chkptr = Checkpointer::new(data_dir.path());
            chkptr.update_checkpoint(fingerprint, position);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
        }
    }

    #[tokio::test]
    async fn test_checkpointer_ignore_before() {
        let now = Utc::now();
        let newer = (FileFingerprint::DevInode(1, 2), now - Duration::seconds(5));
        let oldish = (
            FileFingerprint::FirstLinesChecksum(78910),
            now - Duration::seconds(15),
        );
        let older = (FileFingerprint::DevInode(3, 4), now - Duration::seconds(20));
        let ignore_before = Some(now - Duration::seconds(12));

        let position: FilePosition = 1234;
        let data_dir = tempdir().unwrap();

        // load and persist the checkpoints
        {
            let chkptr = Checkpointer::new(data_dir.path());

            for (fingerprint, modified) in &[&newer, &oldish, &older] {
                chkptr.checkpoints.load(Checkpoint {
                    fingerprint: *fingerprint,
                    position,
                    modified: *modified,
                });
                assert_eq!(chkptr.get_checkpoint(*fingerprint), Some(position));
                chkptr.write_checkpoints().await.unwrap();
            }
        }

        // read them back and assert old are removed
        {
            let mut chkptr = Checkpointer::new(data_dir.path());
            chkptr.read_checkpoints(ignore_before).await;

            assert_eq!(chkptr.get_checkpoint(newer.0), Some(position));
            assert_eq!(chkptr.get_checkpoint(oldish.0), None);
            assert_eq!(chkptr.get_checkpoint(older.0), None);
        }
    }

    #[tokio::test]
    async fn test_checkpointer_restart() {
        let fingerprints = vec![
            FileFingerprint::DevInode(1, 2),
            FileFingerprint::FirstLinesChecksum(78910),
        ];
        for fingerprint in fingerprints {
            let position: FilePosition = 1234;
            let data_dir = tempdir().unwrap();
            {
                let mut chkptr = Checkpointer::new(data_dir.path());
                chkptr.update_checkpoint(fingerprint, position);
                assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
                chkptr.write_checkpoints().await.unwrap();
            }
            {
                let mut chkptr = Checkpointer::new(data_dir.path());
                assert_eq!(chkptr.get_checkpoint(fingerprint), None);
                chkptr.read_checkpoints(None).await;
                assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
            }
        }
    }

    #[tokio::test]
    async fn test_checkpointer_file_upgrades() {
        let fingerprint = FileFingerprint::DevInode(1, 2);
        let position: FilePosition = 1234;

        let data_dir = tempdir().unwrap();

        {
            let mut chkptr = Checkpointer::new(data_dir.path());
            chkptr.update_checkpoint(fingerprint, position);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));

            // Ensure that the new files were not written but the old style of files were
            assert!(!data_dir.path().join(TMP_FILE_NAME).exists());
            assert!(!data_dir.path().join(CHECKPOINT_FILE_NAME).exists());
            assert!(!data_dir.path().join("checkpoints").is_dir());

            chkptr.write_checkpoints().await.unwrap();

            assert!(!data_dir.path().join(TMP_FILE_NAME).exists());
            assert!(data_dir.path().join(CHECKPOINT_FILE_NAME).exists());
            assert!(!data_dir.path().join("checkpoints").is_dir());
        }

        // Read from those old files, ensure the checkpoints were loaded properly, and then write
        // them normally (i.e. in the new format)
        {
            let mut chkptr = Checkpointer::new(data_dir.path());
            chkptr.read_checkpoints(None).await;
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
            chkptr.write_checkpoints().await.unwrap();
        }

        // Ensure that the stable file is present, the tmp file is not, and the legacy files have
        // been cleaned up
        assert!(!data_dir.path().join(TMP_FILE_NAME).exists());
        assert!(data_dir.path().join(CHECKPOINT_FILE_NAME).exists());
        assert!(!data_dir.path().join("checkpoints").is_dir());

        // Ensure one last time that we can reread from the new files and get the same result
        {
            let mut chkptr = Checkpointer::new(data_dir.path());
            chkptr.read_checkpoints(None).await;
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
        }
    }

    #[tokio::test]
    async fn test_checkpointer_expiration() {
        let cases = vec![
            // (checkpoint, position, seconds since removed)
            (FileFingerprint::FirstLinesChecksum(123), 0, 30),
            (FileFingerprint::FirstLinesChecksum(456), 1, 60),
            (FileFingerprint::FirstLinesChecksum(789), 2, 90),
            (FileFingerprint::FirstLinesChecksum(101112), 3, 120),
        ];

        let data_dir = tempdir().unwrap();
        let mut chkptr = Checkpointer::new(data_dir.path());

        for (fingerprint, position, removed) in cases.clone() {
            chkptr.update_checkpoint(fingerprint, position);

            // slide these in manually so we don't have to sleep for a long time
            chkptr
                .checkpoints
                .removed_times
                .insert(fingerprint, Utc::now() - chrono::Duration::seconds(removed));

            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
        }

        // Update one that would otherwise be expired to ensure it sticks around
        chkptr.update_checkpoint(cases[2].0, 42);

        // Expiration is piggybacked on the persistence interval, so do a write to trigger it
        chkptr.write_checkpoints().await.unwrap();

        assert_eq!(chkptr.get_checkpoint(cases[0].0), Some(0));
        assert_eq!(chkptr.get_checkpoint(cases[1].0), None);
        assert_eq!(chkptr.get_checkpoint(cases[2].0), Some(42));
        assert_eq!(chkptr.get_checkpoint(cases[3].0), None);
    }

    #[tokio::test]
    async fn test_checkpointer_strategy_checksum_happy_path() {
        let data_dir = tempdir().unwrap();

        let mut fingerprinter = crate::Fingerprinter::new(
            crate::FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 1,
            },
            1024,
            false,
        );

        let log_path = data_dir.path().join("test.log");
        let contents = "hello i am a test log line that is just long enough but not super long\n";
        fs::write(&log_path, contents)
            .await
            .expect("writing test data");

        let new = fingerprinter
            .fingerprint(&log_path)
            .await
            .expect("getting new checksum");

        assert!(matches!(new, FileFingerprint::FirstLinesChecksum(_)));

        let mut chkptr = Checkpointer::new(data_dir.path());
        chkptr.update_checkpoint(new, 1234);
        assert_eq!(Some(1234), chkptr.get_checkpoint(new));
    }

    // guards against accidental changes to the checkpoint serialization
    #[tokio::test]
    async fn test_checkpointer_serialization() {
        let fingerprints = vec![
            (
                FileFingerprint::DevInode(1, 2),
                r#"{"version":"1","checkpoints":[{"fingerprint":{"dev_inode":[1,2]},"position":1234}]}"#,
            ),
            (
                FileFingerprint::FirstLinesChecksum(78910),
                r#"{"version":"1","checkpoints":[{"fingerprint":{"first_lines_checksum":78910},"position":1234}]}"#,
            ),
        ];
        for (fingerprint, expected) in fingerprints {
            let expected: serde_json::Value = serde_json::from_str(expected).unwrap();

            let position: FilePosition = 1234;
            let data_dir = tempdir().unwrap();
            let mut chkptr = Checkpointer::new(data_dir.path());

            chkptr.update_checkpoint(fingerprint, position);
            chkptr.write_checkpoints().await.unwrap();

            let got: serde_json::Value = {
                let s = fs::read_to_string(data_dir.path().join(CHECKPOINT_FILE_NAME))
                    .await
                    .unwrap();
                let mut checkpoints: serde_json::Value = serde_json::from_str(&s).unwrap();
                for checkpoint in checkpoints["checkpoints"].as_array_mut().unwrap() {
                    checkpoint.as_object_mut().unwrap().remove("modified");
                }
                checkpoints
            };

            assert_eq!(expected, got);
        }
    }

    // guards against accidental changes to the checkpoint deserialization and tests deserializing
    // old checkpoint versions
    #[tokio::test]
    async fn test_checkpointer_deserialization() {
        let serialized_checkpoints = r#"
{
  "version": "1",
  "checkpoints": [
    {
      "fingerprint": { "dev_inode": [ 1, 2 ] },
      "position": 1234,
      "modified": "2021-07-12T18:19:11.769003Z"
    },
    {
      "fingerprint": { "first_line_checksum": 1234 },
      "position": 1234,
      "modified": "2021-07-12T18:19:11.769003Z"
    },
    {
      "fingerprint": { "first_lines_checksum": 78910 },
      "position": 1234,
      "modified": "2021-07-12T18:19:11.769003Z"
    }
  ]
}
        "#;
        let fingerprints = vec![
            FileFingerprint::DevInode(1, 2),
            FileFingerprint::FirstLinesChecksum(1234),
            FileFingerprint::FirstLinesChecksum(78910),
        ];

        let data_dir = tempdir().unwrap();

        let mut chkptr = Checkpointer::new(data_dir.path());

        fs::write(
            data_dir.path().join(CHECKPOINT_FILE_NAME),
            serialized_checkpoints,
        )
        .await
        .unwrap();

        chkptr.read_checkpoints(None).await;

        for fingerprint in fingerprints {
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(1234))
        }
    }
}
