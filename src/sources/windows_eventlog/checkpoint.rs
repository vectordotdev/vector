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
///
/// Uses Windows Event Log bookmarks for robust position tracking that survives
/// channel clears, log rotations, and provides O(1) seeking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelCheckpoint {
    /// The channel name (e.g., "System", "Application", "Security")
    pub channel: String,
    /// Windows Event Log bookmark XML for position tracking
    pub bookmark_xml: String,
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
            version: 1, // Version 1: bookmark-based checkpointing
            channels: HashMap::new(),
        }
    }
}

/// Manages checkpoint persistence for Windows Event Log subscriptions
///
/// Uses Windows Event Log bookmarks (opaque XML handles) to track position in
/// each channel. Bookmarks are more robust than record IDs as they survive
/// channel clears, log rotations, and provide O(1) seeking on restart.
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

    /// Update the checkpoint for a specific channel using bookmark XML
    ///
    /// Bookmarks provide robust position tracking that survives channel clears,
    /// log rotations, and provides O(1) seeking on restart.
    ///
    /// Note: For better performance with multiple channels, prefer `set_batch()`
    /// which writes all checkpoints in a single disk operation.
    #[cfg(test)]
    pub async fn set(
        &self,
        channel: String,
        bookmark_xml: String,
    ) -> Result<(), WindowsEventLogError> {
        let mut state = self.state.lock().await;

        let checkpoint = ChannelCheckpoint {
            channel: channel.clone(),
            bookmark_xml,
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        state.channels.insert(channel.clone(), checkpoint);

        // Persist to disk immediately for reliability
        self.save_to_disk(&state).await?;

        debug!(
            message = "Updated checkpoint for channel",
            channel = %channel
        );

        Ok(())
    }

    /// Update multiple channel checkpoints in a single atomic disk write
    ///
    /// This is much more efficient than calling `set()` multiple times because:
    /// - Single file write instead of N writes
    /// - Single fsync instead of N fsyncs
    /// - Atomic - either all channels update or none do
    ///
    /// This matches the behavior of enterprise tools like Winlogbeat and Splunk UF
    /// which batch checkpoint updates for performance.
    pub async fn set_batch(
        &self,
        updates: Vec<(String, String)>,
    ) -> Result<(), WindowsEventLogError> {
        if updates.is_empty() {
            return Ok(());
        }

        let mut state = self.state.lock().await;
        let timestamp = chrono::Utc::now().to_rfc3339();

        for (channel, bookmark_xml) in &updates {
            let checkpoint = ChannelCheckpoint {
                channel: channel.clone(),
                bookmark_xml: bookmark_xml.clone(),
                updated_at: timestamp.clone(),
            };
            state.channels.insert(channel.clone(), checkpoint);
        }

        // Single disk write for all channels
        self.save_to_disk(&state).await?;

        debug!(
            message = "Batch updated checkpoints",
            channels_updated = updates.len()
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
    #[cfg(test)]
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
    #[cfg(test)]
    pub async fn list(&self) -> Vec<ChannelCheckpoint> {
        let state = self.state.lock().await;
        state.channels.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper to create test bookmark XML
    fn test_bookmark_xml(channel: &str, record_id: u64) -> String {
        format!(
            r#"<BookmarkList><Bookmark Channel="{}" RecordId="{}" IsCurrent="True"/></BookmarkList>"#,
            channel, record_id
        )
    }

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
        let bookmark = test_bookmark_xml("System", 12345);
        checkpointer
            .set("System".to_string(), bookmark.clone())
            .await
            .unwrap();

        // Retrieve checkpoint
        let checkpoint = checkpointer.get("System").await.unwrap();
        assert_eq!(checkpoint.channel, "System");
        assert_eq!(checkpoint.bookmark_xml, bookmark);
    }

    #[tokio::test]
    async fn test_checkpoint_persistence() {
        let temp_dir = TempDir::new().unwrap();

        let system_bookmark = test_bookmark_xml("System", 100);
        let app_bookmark = test_bookmark_xml("Application", 200);

        // Create first checkpointer and set values
        {
            let checkpointer = Checkpointer::new(temp_dir.path()).await.unwrap();
            checkpointer
                .set("System".to_string(), system_bookmark.clone())
                .await
                .unwrap();
            checkpointer
                .set("Application".to_string(), app_bookmark.clone())
                .await
                .unwrap();
        }

        // Create new checkpointer (simulating restart) and verify persistence
        {
            let checkpointer = Checkpointer::new(temp_dir.path()).await.unwrap();
            let system_checkpoint = checkpointer.get("System").await.unwrap();
            assert_eq!(system_checkpoint.bookmark_xml, system_bookmark);

            let app_checkpoint = checkpointer.get("Application").await.unwrap();
            assert_eq!(app_checkpoint.bookmark_xml, app_bookmark);
        }
    }

    #[tokio::test]
    async fn test_checkpoint_update() {
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        // Set initial value
        let bookmark1 = test_bookmark_xml("System", 100);
        checkpointer
            .set("System".to_string(), bookmark1)
            .await
            .unwrap();

        // Update value
        let bookmark2 = test_bookmark_xml("System", 200);
        checkpointer
            .set("System".to_string(), bookmark2.clone())
            .await
            .unwrap();

        // Verify updated value
        let checkpoint = checkpointer.get("System").await.unwrap();
        assert_eq!(checkpoint.bookmark_xml, bookmark2);
    }

    #[tokio::test]
    async fn test_checkpoint_multiple_channels() {
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let system_bookmark = test_bookmark_xml("System", 100);
        let app_bookmark = test_bookmark_xml("Application", 200);
        let security_bookmark = test_bookmark_xml("Security", 300);

        checkpointer
            .set("System".to_string(), system_bookmark.clone())
            .await
            .unwrap();
        checkpointer
            .set("Application".to_string(), app_bookmark.clone())
            .await
            .unwrap();
        checkpointer
            .set("Security".to_string(), security_bookmark.clone())
            .await
            .unwrap();

        assert_eq!(
            checkpointer.get("System").await.unwrap().bookmark_xml,
            system_bookmark
        );
        assert_eq!(
            checkpointer.get("Application").await.unwrap().bookmark_xml,
            app_bookmark
        );
        assert_eq!(
            checkpointer.get("Security").await.unwrap().bookmark_xml,
            security_bookmark
        );
    }

    #[tokio::test]
    async fn test_checkpoint_remove() {
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let bookmark = test_bookmark_xml("System", 100);
        checkpointer
            .set("System".to_string(), bookmark)
            .await
            .unwrap();
        assert!(checkpointer.get("System").await.is_some());

        checkpointer.remove("System").await.unwrap();
        assert!(checkpointer.get("System").await.is_none());
    }

    #[tokio::test]
    async fn test_checkpoint_list() {
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let system_bookmark = test_bookmark_xml("System", 100);
        let app_bookmark = test_bookmark_xml("Application", 200);

        checkpointer
            .set("System".to_string(), system_bookmark)
            .await
            .unwrap();
        checkpointer
            .set("Application".to_string(), app_bookmark)
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
        let bookmark = test_bookmark_xml("System", 100);
        checkpointer
            .set("System".to_string(), bookmark.clone())
            .await
            .unwrap();
        assert_eq!(
            checkpointer.get("System").await.unwrap().bookmark_xml,
            bookmark
        );
    }

    #[tokio::test]
    async fn test_checkpoint_batch_update() {
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let system_bookmark = test_bookmark_xml("System", 100);
        let app_bookmark = test_bookmark_xml("Application", 200);
        let security_bookmark = test_bookmark_xml("Security", 300);

        // Batch update all channels at once
        checkpointer
            .set_batch(vec![
                ("System".to_string(), system_bookmark.clone()),
                ("Application".to_string(), app_bookmark.clone()),
                ("Security".to_string(), security_bookmark.clone()),
            ])
            .await
            .unwrap();

        // Verify all channels were updated
        assert_eq!(
            checkpointer.get("System").await.unwrap().bookmark_xml,
            system_bookmark
        );
        assert_eq!(
            checkpointer.get("Application").await.unwrap().bookmark_xml,
            app_bookmark
        );
        assert_eq!(
            checkpointer.get("Security").await.unwrap().bookmark_xml,
            security_bookmark
        );
    }

    #[tokio::test]
    async fn test_checkpoint_batch_empty() {
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        // Empty batch should succeed without writing
        checkpointer.set_batch(vec![]).await.unwrap();

        // No checkpoints should exist
        assert!(checkpointer.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_checkpoint_batch_persistence() {
        let temp_dir = TempDir::new().unwrap();

        let system_bookmark = test_bookmark_xml("System", 100);
        let app_bookmark = test_bookmark_xml("Application", 200);

        // Create first checkpointer and batch update
        {
            let checkpointer = Checkpointer::new(temp_dir.path()).await.unwrap();
            checkpointer
                .set_batch(vec![
                    ("System".to_string(), system_bookmark.clone()),
                    ("Application".to_string(), app_bookmark.clone()),
                ])
                .await
                .unwrap();
        }

        // Create new checkpointer (simulating restart) and verify persistence
        {
            let checkpointer = Checkpointer::new(temp_dir.path()).await.unwrap();
            assert_eq!(
                checkpointer.get("System").await.unwrap().bookmark_xml,
                system_bookmark
            );
            assert_eq!(
                checkpointer.get("Application").await.unwrap().bookmark_xml,
                app_bookmark
            );
        }
    }
}
