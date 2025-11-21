use std::{
    collections::HashMap,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, OpenOptions},
    io::AsyncWriteExt,
    sync::Mutex,
};
use tracing::{debug, error, info, warn};

use super::error::WindowsEventLogError;

const CHECKPOINT_FILENAME: &str = "windows_eventlog_checkpoints.json";

/// Checkpoint data for a single Windows Event Log channel
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelCheckpoint {
    /// The channel name (e.g., "System", "Application", "Security")
    pub channel: String,
    /// The last successfully processed record ID
    pub record_id: u64,
    /// Timestamp when this checkpoint was last updated (for debugging)
    #[serde(default)]
    pub updated_at: String,
}

/// Container for all channel checkpoints
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CheckpointState {
    /// Version for future compatibility
    version: u32,
    /// Map of channel name to checkpoint
    channels: HashMap<String, ChannelCheckpoint>,
}

impl Default for CheckpointState {
    fn default() -> Self {
        Self {
            version: 1,
            channels: HashMap::new(),
        }
    }
}

/// Manages checkpoint persistence for Windows Event Log subscriptions
///
/// Similar to Vector's journald checkpointer, this stores the last processed
/// record ID for each channel to allow resumption after restarts.
pub struct Checkpointer {
    checkpoint_path: PathBuf,
    state: Mutex<CheckpointState>,
}

impl Checkpointer {
    /// Create a new checkpointer for the given data directory
    pub async fn new(data_dir: &Path) -> Result<Self, WindowsEventLogError> {
        let checkpoint_path = data_dir.join(CHECKPOINT_FILENAME);

        // Ensure the data directory exists
        if let Err(e) = fs::create_dir_all(data_dir).await {
            if e.kind() != ErrorKind::AlreadyExists {
                return Err(WindowsEventLogError::IoError { source: e });
            }
        }

        // Load existing checkpoint state or create new
        let state = Self::load_from_disk(&checkpoint_path).await?;

        info!(
            message = "Windows Event Log checkpointer initialized",
            checkpoint_path = %checkpoint_path.display(),
            channels = state.channels.len()
        );

        Ok(Self {
            checkpoint_path,
            state: Mutex::new(state),
        })
    }

    /// Get the last checkpoint for a specific channel
    pub async fn get(&self, channel: &str) -> Option<ChannelCheckpoint> {
        let state = self.state.lock().await;
        state.channels.get(channel).cloned()
    }

    /// Update the checkpoint for a specific channel
    pub async fn set(&self, channel: String, record_id: u64) -> Result<(), WindowsEventLogError> {
        let mut state = self.state.lock().await;

        let checkpoint = ChannelCheckpoint {
            channel: channel.clone(),
            record_id,
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        state.channels.insert(channel.clone(), checkpoint);

        // Persist to disk immediately for reliability
        self.save_to_disk(&state).await?;

        debug!(
            message = "Updated checkpoint for channel",
            channel = %channel,
            record_id = record_id
        );

        Ok(())
    }

    /// Load checkpoint state from disk
    async fn load_from_disk(path: &Path) -> Result<CheckpointState, WindowsEventLogError> {
        match fs::read(path).await {
            Ok(contents) => match serde_json::from_slice::<CheckpointState>(&contents) {
                Ok(state) => {
                    info!(
                        message = "Loaded existing checkpoints",
                        channels = state.channels.len(),
                        path = %path.display()
                    );
                    Ok(state)
                }
                Err(e) => {
                    warn!(
                        message = "Failed to parse checkpoint file, starting fresh",
                        error = %e,
                        path = %path.display()
                    );
                    Ok(CheckpointState::default())
                }
            },
            Err(e) if e.kind() == ErrorKind::NotFound => {
                debug!(
                    message = "No existing checkpoint file, starting fresh",
                    path = %path.display()
                );
                Ok(CheckpointState::default())
            }
            Err(e) => {
                error!(
                    message = "Failed to read checkpoint file",
                    error = %e,
                    path = %path.display()
                );
                Err(WindowsEventLogError::IoError { source: e })
            }
        }
    }

    /// Save checkpoint state to disk atomically
    async fn save_to_disk(&self, state: &CheckpointState) -> Result<(), WindowsEventLogError> {
        // Use atomic write: write to temp file, then rename
        let temp_path = self.checkpoint_path.with_extension("tmp");

        // Serialize state
        let contents = match serde_json::to_vec_pretty(state) {
            Ok(c) => c,
            Err(e) => {
                error!(
                    message = "Failed to serialize checkpoint state",
                    error = %e
                );
                return Err(WindowsEventLogError::IoError {
                    source: io::Error::new(ErrorKind::InvalidData, e),
                });
            }
        };

        // Write to temp file
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)
            .await
            .map_err(|e| WindowsEventLogError::IoError { source: e })?;

        file.write_all(&contents)
            .await
            .map_err(|e| WindowsEventLogError::IoError { source: e })?;

        file.sync_all()
            .await
            .map_err(|e| WindowsEventLogError::IoError { source: e })?;

        drop(file);

        // Atomic rename
        fs::rename(&temp_path, &self.checkpoint_path)
            .await
            .map_err(|e| WindowsEventLogError::IoError { source: e })?;

        Ok(())
    }

    /// Remove checkpoint for a channel (useful for testing or reset)
    #[allow(dead_code)]
    pub async fn remove(&self, channel: &str) -> Result<(), WindowsEventLogError> {
        let mut state = self.state.lock().await;
        state.channels.remove(channel);
        self.save_to_disk(&state).await?;

        info!(
            message = "Removed checkpoint for channel",
            channel = %channel
        );

        Ok(())
    }

    /// Get all channel checkpoints (useful for debugging)
    #[allow(dead_code)]
    pub async fn list(&self) -> Vec<ChannelCheckpoint> {
        let state = self.state.lock().await;
        state.channels.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_checkpointer() -> (Checkpointer, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let checkpointer = Checkpointer::new(temp_dir.path()).await.unwrap();
        (checkpointer, temp_dir)
    }

    #[tokio::test]
    async fn test_checkpoint_basic_operations() {
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        // Initially empty
        assert!(checkpointer.get("System").await.is_none());

        // Set checkpoint
        checkpointer.set("System".to_string(), 12345).await.unwrap();

        // Retrieve checkpoint
        let checkpoint = checkpointer.get("System").await.unwrap();
        assert_eq!(checkpoint.channel, "System");
        assert_eq!(checkpoint.record_id, 12345);
    }

    #[tokio::test]
    async fn test_checkpoint_persistence() {
        let temp_dir = TempDir::new().unwrap();

        // Create first checkpointer and set values
        {
            let checkpointer = Checkpointer::new(temp_dir.path()).await.unwrap();
            checkpointer.set("System".to_string(), 100).await.unwrap();
            checkpointer
                .set("Application".to_string(), 200)
                .await
                .unwrap();
        }

        // Create new checkpointer (simulating restart) and verify persistence
        {
            let checkpointer = Checkpointer::new(temp_dir.path()).await.unwrap();
            let system_checkpoint = checkpointer.get("System").await.unwrap();
            assert_eq!(system_checkpoint.record_id, 100);

            let app_checkpoint = checkpointer.get("Application").await.unwrap();
            assert_eq!(app_checkpoint.record_id, 200);
        }
    }

    #[tokio::test]
    async fn test_checkpoint_update() {
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        // Set initial value
        checkpointer.set("System".to_string(), 100).await.unwrap();

        // Update value
        checkpointer.set("System".to_string(), 200).await.unwrap();

        // Verify updated value
        let checkpoint = checkpointer.get("System").await.unwrap();
        assert_eq!(checkpoint.record_id, 200);
    }

    #[tokio::test]
    async fn test_checkpoint_multiple_channels() {
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        checkpointer.set("System".to_string(), 100).await.unwrap();
        checkpointer
            .set("Application".to_string(), 200)
            .await
            .unwrap();
        checkpointer.set("Security".to_string(), 300).await.unwrap();

        assert_eq!(checkpointer.get("System").await.unwrap().record_id, 100);
        assert_eq!(
            checkpointer.get("Application").await.unwrap().record_id,
            200
        );
        assert_eq!(checkpointer.get("Security").await.unwrap().record_id, 300);
    }

    #[tokio::test]
    async fn test_checkpoint_remove() {
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        checkpointer.set("System".to_string(), 100).await.unwrap();
        assert!(checkpointer.get("System").await.is_some());

        checkpointer.remove("System").await.unwrap();
        assert!(checkpointer.get("System").await.is_none());
    }

    #[tokio::test]
    async fn test_checkpoint_list() {
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        checkpointer.set("System".to_string(), 100).await.unwrap();
        checkpointer
            .set("Application".to_string(), 200)
            .await
            .unwrap();

        let checkpoints = checkpointer.list().await;
        assert_eq!(checkpoints.len(), 2);
    }

    #[tokio::test]
    async fn test_corrupted_checkpoint_file() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_path = temp_dir.path().join(CHECKPOINT_FILENAME);

        // Write corrupted data
        fs::write(&checkpoint_path, b"invalid json {{{")
            .await
            .unwrap();

        // Should handle gracefully and start fresh
        let checkpointer = Checkpointer::new(temp_dir.path()).await.unwrap();
        assert!(checkpointer.get("System").await.is_none());

        // Should be able to write new checkpoints
        checkpointer.set("System".to_string(), 100).await.unwrap();
        assert_eq!(checkpointer.get("System").await.unwrap().record_id, 100);
    }
}
