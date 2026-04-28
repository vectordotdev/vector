use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, FixedOffset, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::{fs, sync::Mutex};
use tracing::{error, info, warn};

const TMP_FILE_NAME: &str = "checkpoints.new.json";
const CHECKPOINT_FILE_NAME: &str = "checkpoints.json";
const CHECKPOINT_EXPIRY: chrono::Duration = chrono::Duration::days(7);

/// This enum represents the file format of checkpoints persisted to disk. Right
/// now there is only one variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "version", rename_all = "snake_case")]
enum State {
    #[serde(rename = "1")]
    V1 {
        checkpoints: BTreeSet<ContainerCheckpoint>,
    },
}

/// A container checkpoint mapping container ID to last log timestamp.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
struct ContainerCheckpoint {
    container_id: String,
    last_log_timestamp: DateTime<FixedOffset>,
    modified: DateTime<Utc>,
}

pub(super) struct Checkpointer {
    tmp_file_path: PathBuf,
    stable_file_path: PathBuf,
    checkpoints: Arc<CheckpointsView>,
    last: Mutex<Option<State>>,
}

/// A thread-safe handle for reading and writing checkpoints in-memory across
/// multiple threads.
#[derive(Debug, Default)]
pub(super) struct CheckpointsView {
    checkpoints: DashMap<String, DateTime<FixedOffset>>,
    modified_times: DashMap<String, DateTime<Utc>>,
}

impl CheckpointsView {
    pub(super) fn update(&self, container_id: &str, timestamp: DateTime<FixedOffset>) {
        self.checkpoints.insert(container_id.to_string(), timestamp);
        self.modified_times
            .insert(container_id.to_string(), Utc::now());
    }

    pub(super) fn get(&self, container_id: &str) -> Option<DateTime<FixedOffset>> {
        self.checkpoints.get(container_id).map(|r| *r.value())
    }

    pub(super) fn remove_expired(&self) {
        let now = Utc::now();

        // Collect all of the expired keys. Removing them while iterating can
        // lead to deadlocks, the set should be small, and this is not a
        // performance-sensitive path.
        let to_remove = self
            .modified_times
            .iter()
            .filter(|entry| {
                let ts = entry.value();
                let duration = now - *ts;
                duration >= CHECKPOINT_EXPIRY
            })
            .map(|entry| entry.key().clone())
            .collect::<Vec<String>>();

        for key in to_remove {
            self.checkpoints.remove(&key);
            self.modified_times.remove(&key);
        }
    }

    fn load(&self, checkpoint: ContainerCheckpoint) {
        self.checkpoints.insert(
            checkpoint.container_id.clone(),
            checkpoint.last_log_timestamp,
        );
        self.modified_times
            .insert(checkpoint.container_id, checkpoint.modified);
    }

    fn set_state(&self, state: State) {
        match state {
            State::V1 { checkpoints } => {
                for checkpoint in checkpoints {
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
                    let container_id = entry.key();
                    let last_log_timestamp = entry.value();
                    ContainerCheckpoint {
                        container_id: container_id.clone(),
                        last_log_timestamp: *last_log_timestamp,
                        modified: self
                            .modified_times
                            .get(container_id)
                            .map(|r| *r.value())
                            .unwrap_or_else(Utc::now),
                    }
                })
                .collect(),
        }
    }
}

impl Checkpointer {
    pub(super) fn new(data_dir: &Path) -> Checkpointer {
        let tmp_file_path = data_dir.join(TMP_FILE_NAME);
        let stable_file_path = data_dir.join(CHECKPOINT_FILE_NAME);

        Checkpointer {
            tmp_file_path,
            stable_file_path,
            checkpoints: Arc::new(CheckpointsView::default()),
            last: Mutex::new(None),
        }
    }

    pub(super) fn view(&self) -> Arc<CheckpointsView> {
        Arc::clone(&self.checkpoints)
    }

    /// Persist the current checkpoints state to disk, making our best effort to
    /// do so in an atomic way that allows for recovering the previous state in
    /// the event of a crash.
    pub(super) async fn write_checkpoints(&self) -> Result<usize, io::Error> {
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

    /// Read persisted checkpoints from disk, preferring the tmp file (which
    /// indicates an interrupted checkpoint write) over the stable file.
    pub(super) fn read_checkpoints(&self) {
        // First try reading from the tmp file location. If this works, it means
        // that the previous process was interrupted in the process of
        // checkpointing and the tmp file should contain more recent data that
        // should be preferred.
        match self.read_checkpoints_file(&self.tmp_file_path) {
            Ok(state) => {
                warn!(message = "Recovered checkpoint data from interrupted process.");
                self.checkpoints.set_state(state);
                self.checkpoints.remove_expired();

                // Try to move this tmp file to the stable location so we don't
                // immediately overwrite it when we next persist checkpoints.
                if let Err(error) = std::fs::rename(&self.tmp_file_path, &self.stable_file_path) {
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
                self.checkpoints.set_state(state);
                self.checkpoints.remove_expired();
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // This is expected, so no warning needed
            }
            Err(error) => {
                warn!(message = "Unable to load checkpoint data.", %error);
            }
        }
    }

    fn read_checkpoints_file(&self, path: &Path) -> io::Result<State> {
        let data = std::fs::read_to_string(path)?;
        serde_json::from_str(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}
