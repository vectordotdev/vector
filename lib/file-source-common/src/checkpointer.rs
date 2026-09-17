use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
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

/// Identifies *which* watcher a checkpoint belongs to.
///
/// A fingerprint names content, not a reader: the same value can be held by one watcher, retired by
/// a rewrite, then held by a different file whose first lines happen to hash the same. An
/// acknowledgement carries the fingerprint captured when its line was *read*, so without a second
/// term there is no way to tell a live reader's progress from a previous owner's late arrival --
/// and accepting the latter resumes a reader past content it never emitted.
///
/// Runtime-only, and deliberately not persisted: after a restart there are no acknowledgements in
/// flight, so a loaded checkpoint has no owner until a watcher registers for it.
pub type OwnerGeneration = u64;

/// The checkpoint owned by one reader. A fingerprint identifies persisted content; the
/// generation identifies the reader, and its initial position must belong to that reader.
#[derive(Debug, Clone, Copy)]
pub struct ReaderCheckpoint {
    pub fingerprint: FileFingerprint,
    pub generation: OwnerGeneration,
    pub position: FilePosition,
}

/// Progress of an independently owned reader. An incomplete fingerprint is not a key borrowed
/// from another file: it stays absent until discovery can identify this reader's content.
#[derive(Debug)]
struct GenerationCheckpoint {
    fingerprint: Option<FileFingerprint>,
    provisional_fingerprint: Option<FileFingerprint>,
    acknowledged: FilePosition,
    read_position: FilePosition,
    acknowledgement_target: FilePosition,
    completed_at: Option<DateTime<Utc>>,
    modified: DateTime<Utc>,
}

/// Hands out the generation numbers. Process-wide, so two watchers never share one.
static NEXT_OWNER_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Claim a generation for a watcher about to be installed on a fingerprint.
pub fn next_owner_generation() -> OwnerGeneration {
    NEXT_OWNER_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A thread-safe handle for reading and writing checkpoints in-memory across
/// multiple threads.
#[derive(Debug, Default)]
pub struct CheckpointsView {
    readers: RwLock<HashMap<OwnerGeneration, GenerationCheckpoint>>,
    // Always lock readers first. The index is updated under its write lock, so readers and
    // snapshots cannot observe a partially published ownership transition.
    reader_fingerprints: RwLock<HashMap<FileFingerprint, HashSet<OwnerGeneration>>>,
    /// Unclaimed positions loaded from disk; never writable by acknowledgements.
    loaded: DashMap<FileFingerprint, Checkpoint>,
}

impl CheckpointsView {
    /// Acknowledge only the reader captured when the record was emitted. Unknown generations
    /// are expired or invalidated; they must never fall back to fingerprint ownership.
    pub fn acknowledge_reader(&self, generation: OwnerGeneration, position: FilePosition) {
        let mut readers = self.readers.write().expect("reader checkpoints poisoned");
        if let Some(reader) = readers.get_mut(&generation) {
            reader.acknowledged = reader.acknowledged.max(position);
            reader.modified = Utc::now();
        }
    }

    pub fn get(&self, fng: FileFingerprint) -> Option<FilePosition> {
        let readers = self.readers.read().expect("reader checkpoints poisoned");
        let index = self
            .reader_fingerprints
            .read()
            .expect("reader index poisoned");
        let loaded = self.loaded.get(&fng).map(|entry| entry.position);
        index
            .get(&fng)
            .into_iter()
            .flatten()
            .filter_map(|generation| readers.get(generation))
            .map(|reader| reader.read_position.max(reader.acknowledged))
            .chain(loaded)
            .min()
    }

    /// Return the last position that is safe to persist, excluding a rotation drain that is still
    /// waiting for acknowledgements. This is used before repointing the old watcher.
    pub fn get_acknowledged(&self, fng: FileFingerprint) -> Option<FilePosition> {
        let readers = self.readers.read().expect("reader checkpoints poisoned");
        let index = self
            .reader_fingerprints
            .read()
            .expect("reader index poisoned");
        index
            .get(&fng)
            .into_iter()
            .flatten()
            .filter_map(|generation| readers.get(generation))
            .map(|reader| reader.acknowledged)
            .chain(self.loaded.get(&fng).map(|entry| entry.position))
            .min()
    }

    // Call only while holding the readers write lock; all index readers acquire readers first.
    fn index_reader(
        &self,
        generation: OwnerGeneration,
        old: Option<FileFingerprint>,
        new: Option<FileFingerprint>,
    ) {
        if old == new {
            return;
        }
        let mut index = self
            .reader_fingerprints
            .write()
            .expect("reader index poisoned");
        if let Some(old) = old
            && let Some(generations) = index.get_mut(&old)
        {
            generations.remove(&generation);
            if generations.is_empty() {
                index.remove(&old);
            }
        }
        if let Some(new) = new {
            index.entry(new).or_default().insert(generation);
        }
    }

    pub fn register_reader(
        &self,
        fingerprint: Option<FileFingerprint>,
        generation: OwnerGeneration,
        position: FilePosition,
    ) {
        let mut readers = self.readers.write().expect("reader checkpoints poisoned");
        // Loaded checkpoints supply the starting position, but are not another live reader.
        // Consume that bootstrap entry in the same transaction as publishing the generation;
        // otherwise snapshots keep taking the minimum with the stale on-disk position forever.
        if let Some(fingerprint) = fingerprint {
            self.loaded.remove(&fingerprint);
        }
        let previous = readers.insert(
            generation,
            GenerationCheckpoint {
                fingerprint,
                provisional_fingerprint: None,
                acknowledged: position,
                read_position: position,
                acknowledgement_target: position,
                completed_at: None,
                modified: Utc::now(),
            },
        );
        self.index_reader(
            generation,
            previous.and_then(|reader| reader.fingerprint),
            fingerprint,
        );
    }

    /// Publish both sides of a rotation in one snapshot transaction. An incomplete replacement
    /// reserves the former key at zero until its actual fingerprint is known.
    pub fn replace_reader(
        &self,
        previous: ReaderCheckpoint,
        fingerprint: Option<FileFingerprint>,
        generation: OwnerGeneration,
        emitted_position: FilePosition,
    ) {
        let mut readers = self.readers.write().expect("reader checkpoints poisoned");
        if let Some(reader) = readers.get_mut(&previous.generation) {
            reader.read_position = previous.position;
            reader.acknowledgement_target = emitted_position;
        }
        if let Some(fingerprint) = fingerprint {
            self.loaded.remove(&fingerprint);
        }
        let replaced = readers.insert(
            generation,
            GenerationCheckpoint {
                fingerprint,
                provisional_fingerprint: fingerprint.is_none().then_some(previous.fingerprint),
                acknowledged: 0,
                read_position: 0,
                acknowledgement_target: 0,
                completed_at: None,
                modified: Utc::now(),
            },
        );
        self.index_reader(
            generation,
            replaced.and_then(|reader| reader.fingerprint),
            fingerprint,
        );
    }

    /// Bind a completed fingerprint without changing the reader's identity or invalidating lines
    /// already travelling downstream with that generation and its former provisional key.
    pub fn bind_reader(&self, generation: OwnerGeneration, fingerprint: FileFingerprint) -> bool {
        let mut readers = self.readers.write().expect("reader checkpoints poisoned");
        let Some(reader) = readers.get_mut(&generation) else {
            return false;
        };
        let old = reader.fingerprint;
        self.loaded.remove(&fingerprint);
        reader.fingerprint = Some(fingerprint);
        reader.provisional_fingerprint = None;
        reader.modified = Utc::now();
        self.index_reader(generation, old, Some(fingerprint));
        true
    }

    /// A short replacement may finish its fingerprint only after becoming an archive.
    pub fn reader_needs_fingerprint(&self, generation: OwnerGeneration) -> bool {
        self.readers
            .read()
            .expect("reader checkpoints poisoned")
            .get(&generation)
            .is_some_and(|reader| reader.fingerprint.is_none())
    }

    pub fn finish_reader(&self, generation: OwnerGeneration, read_position: FilePosition) {
        if let Some(reader) = self
            .readers
            .write()
            .expect("reader checkpoints poisoned")
            .get_mut(&generation)
        {
            reader.read_position = read_position;
            reader.completed_at = Some(Utc::now());
        }
    }

    /// Track consumed bytes and the last emitted record separately: discarded oversized lines
    /// advance the reader but can never produce a downstream acknowledgement.
    pub fn record_read(
        &self,
        generation: OwnerGeneration,
        position: FilePosition,
        emitted_position: Option<FilePosition>,
    ) {
        if let Some(reader) = self
            .readers
            .write()
            .expect("reader checkpoints poisoned")
            .get_mut(&generation)
        {
            reader.read_position = position;
            if let Some(emitted_position) = emitted_position {
                reader.acknowledgement_target = reader.acknowledgement_target.max(emitted_position);
            }
        }
    }

    /// Include final fragments emitted outside the regular read loop, for example during removal.
    pub fn record_emitted(
        &self,
        positions: impl IntoIterator<Item = (OwnerGeneration, FilePosition)>,
    ) {
        let mut readers = self.readers.write().expect("reader checkpoints poisoned");
        for (generation, position) in positions {
            if let Some(reader) = readers.get_mut(&generation) {
                reader.acknowledgement_target = reader.acknowledgement_target.max(position);
            }
        }
    }

    /// Rewrites invalidate the old content, unlike rotation, which retains an opened inode.
    pub fn restart_reader(
        &self,
        fingerprint: FileFingerprint,
        old: OwnerGeneration,
        new: OwnerGeneration,
    ) -> bool {
        self.restart_reader_as(Some(fingerprint), old, new)
    }

    /// Restart a reader whose rewritten content does not yet have a confirmed fingerprint.
    ///
    /// The previous generation must still be completed and unindexed: leaving it live under its
    /// old fingerprint would make the stale checkpoint look like it belongs to the new content.
    pub fn restart_reader_without_fingerprint(
        &self,
        old: OwnerGeneration,
        new: OwnerGeneration,
    ) -> bool {
        self.restart_reader_as(None, old, new)
    }

    fn restart_reader_as(
        &self,
        fingerprint: Option<FileFingerprint>,
        old: OwnerGeneration,
        new: OwnerGeneration,
    ) -> bool {
        let mut readers = self.readers.write().expect("reader checkpoints poisoned");
        let Some(previous) = readers.get_mut(&old) else {
            return false;
        };
        self.index_reader(old, previous.fingerprint, None);
        previous.fingerprint = None;
        previous.provisional_fingerprint = None;
        previous.completed_at = Some(Utc::now());
        if let Some(fingerprint) = fingerprint {
            self.loaded.remove(&fingerprint);
        }
        let replaced = readers.insert(
            new,
            GenerationCheckpoint {
                fingerprint,
                provisional_fingerprint: None,
                acknowledged: 0,
                read_position: 0,
                acknowledgement_target: 0,
                completed_at: None,
                modified: Utc::now(),
            },
        );
        self.index_reader(
            new,
            replaced.and_then(|reader| reader.fingerprint),
            fingerprint,
        );
        true
    }

    pub fn remove_expired(&self) {
        self.remove_expired_before(Utc::now());
    }

    /// [`Self::remove_expired`] against a given instant, so a test need not wait out the window.
    fn remove_expired_before(&self, now: DateTime<Utc>) {
        self.readers
            .write()
            .expect("reader checkpoints poisoned")
            .retain(|generation, reader| {
                let keep = reader.acknowledged < reader.acknowledgement_target
                    || reader
                        .completed_at
                        .is_none_or(|completed| now - completed < chrono::Duration::seconds(60));
                if !keep {
                    self.index_reader(*generation, reader.fingerprint, None);
                }
                keep
            });
    }

    fn load(&self, checkpoint: Checkpoint) {
        let _readers = self.readers.write().expect("reader checkpoints poisoned");
        self.loaded.insert(checkpoint.fingerprint, checkpoint);
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
        let readers = self.readers.read().expect("reader checkpoints poisoned");
        let mut merged = BTreeMap::<FileFingerprint, Checkpoint>::new();
        for reader in readers.values() {
            if let Some(fingerprint) = reader.fingerprint.or(reader.provisional_fingerprint) {
                let acknowledged = if reader.fingerprint.is_some() {
                    reader.acknowledged
                } else {
                    0
                };
                let checkpoint = merged.entry(fingerprint).or_insert(Checkpoint {
                    fingerprint,
                    position: acknowledged,
                    modified: reader.modified,
                });
                checkpoint.position = checkpoint.position.min(acknowledged);
                checkpoint.modified = checkpoint.modified.max(reader.modified);
            }
        }
        for entry in &self.loaded {
            let loaded = entry.value();
            let checkpoint = merged
                .entry(loaded.fingerprint)
                .or_insert_with(|| loaded.clone());
            checkpoint.position = checkpoint.position.min(loaded.position);
        }
        State::V1 {
            checkpoints: merged.into_values().collect(),
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
        self.checkpoints.load(Checkpoint {
            fingerprint: fng,
            position: pos,
            modified: Utc::now(),
        });
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
        let State::V1 { checkpoints } = &current;
        let checkpoint_count = checkpoints.len();

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

        Ok(checkpoint_count)
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
        CHECKPOINT_FILE_NAME, Checkpoint, Checkpointer, CheckpointsView, FileFingerprint,
        FilePosition, ReaderCheckpoint, State, TMP_FILE_NAME, next_owner_generation,
    };

    #[test]
    fn an_unread_displaced_generation_does_not_pin_checkpoints_forever() {
        let view = CheckpointsView::default();
        let key = FileFingerprint::FirstLinesChecksum(1);
        let displaced = next_owner_generation();
        let selected = next_owner_generation();
        view.register_reader(Some(key), displaced, 0);
        view.finish_reader(displaced, 0);
        view.register_reader(Some(key), selected, 0);
        view.record_read(selected, 100, Some(100));
        view.acknowledge_reader(selected, 100);
        view.remove_expired_before(Utc::now() + Duration::seconds(61));
        let State::V1 { checkpoints } = view.get_state();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints.first().unwrap().position, 100);
        view.acknowledge_reader(displaced, 200);
        assert_eq!(view.get_acknowledged(key), Some(100));
    }

    #[test]
    fn fingerprint_transitions_consume_loaded_positions() {
        for transition in 0..3 {
            let view = CheckpointsView::default();
            let old = FileFingerprint::FirstLinesChecksum(1);
            let new = FileFingerprint::FirstLinesChecksum(2);
            let first = next_owner_generation();
            let second = next_owner_generation();
            view.load(Checkpoint {
                fingerprint: new,
                position: 5,
                modified: Utc::now(),
            });
            view.register_reader(Some(old), first, 0);
            let generation = match transition {
                0 => {
                    assert!(view.bind_reader(first, new));
                    first
                }
                1 => {
                    assert!(view.restart_reader(new, first, second));
                    second
                }
                _ => {
                    view.replace_reader(
                        ReaderCheckpoint {
                            fingerprint: old,
                            generation: first,
                            position: 0,
                        },
                        Some(new),
                        second,
                        0,
                    );
                    second
                }
            };
            view.acknowledge_reader(generation, 20);
            assert_eq!(view.get_acknowledged(new), Some(20));
            let State::V1 { checkpoints } = view.get_state();
            assert_eq!(
                checkpoints
                    .iter()
                    .find(|entry| entry.fingerprint == new)
                    .unwrap()
                    .position,
                20
            );
        }
    }

    #[test]
    fn stale_acknowledgements_and_death_do_not_affect_a_rewrite() {
        let view = CheckpointsView::default();
        let key = FileFingerprint::FirstLinesChecksum(1);
        let first = next_owner_generation();
        let second = next_owner_generation();
        view.register_reader(Some(key), first, 100);
        assert!(view.restart_reader(key, first, second));
        view.acknowledge_reader(first, 200);
        view.finish_reader(first, 200);
        view.remove_expired_before(Utc::now() + Duration::hours(1));
        assert_eq!(view.get_acknowledged(key), Some(0));
        view.acknowledge_reader(first, 300);
        view.acknowledge_reader(second, 10);
        assert_eq!(view.get_acknowledged(key), Some(10));
    }

    #[test]
    fn restarting_without_a_fingerprint_completes_the_previous_generation() {
        let view = CheckpointsView::default();
        let stale = FileFingerprint::FirstLinesChecksum(1);
        let old = next_owner_generation();
        let new = next_owner_generation();
        view.register_reader(Some(stale), old, 100);

        assert!(view.restart_reader_without_fingerprint(old, new));

        let readers = view.readers.read().unwrap();
        let previous = readers.get(&old).unwrap();
        assert!(previous.completed_at.is_some());
        assert_eq!(previous.fingerprint, None);
        assert_eq!(previous.provisional_fingerprint, None);
        drop(readers);
        assert_eq!(view.get(stale), None, "the stale fingerprint is unindexed");
        assert!(view.reader_needs_fingerprint(new));

        view.remove_expired_before(Utc::now() + Duration::hours(1));
        assert!(
            !view.readers.read().unwrap().contains_key(&old),
            "the completed generation must be eligible for normal checkpoint expiry"
        );
        assert!(view.reader_needs_fingerprint(new));
    }

    #[test]
    fn retained_generations_keep_independent_acknowledgements() {
        let view = CheckpointsView::default();
        let key = FileFingerprint::FirstLinesChecksum(1);
        let replacement_key = FileFingerprint::FirstLinesChecksum(2);
        let first = next_owner_generation();
        let second = next_owner_generation();
        let third = next_owner_generation();
        view.register_reader(Some(key), first, 100);
        view.record_read(first, 200, Some(200));
        view.register_reader(None, second, 0);
        view.record_read(second, 40, Some(40));
        view.register_reader(Some(replacement_key), third, 0);
        view.finish_reader(first, 200);
        view.finish_reader(second, 40);

        // The middle inode rotated before its fingerprint was available. Its acknowledgements
        // still name the provisional key, but must never advance either neighbouring reader.
        view.acknowledge_reader(second, 40);
        view.acknowledge_reader(third, 20);
        view.acknowledge_reader(first, 150);
        assert_eq!(view.get_acknowledged(key), Some(150));
        assert_eq!(view.get_acknowledged(replacement_key), Some(20));
        assert!(view.bind_reader(second, replacement_key));
        assert_eq!(view.get_acknowledged(replacement_key), Some(20));

        let after_expiry = Utc::now() + Duration::hours(1);
        view.remove_expired_before(after_expiry);
        assert_eq!(view.get_acknowledged(key), Some(150));
        view.acknowledge_reader(first, 200);
        view.remove_expired_before(after_expiry);
        assert_eq!(view.get_acknowledged(key), None);
        assert_eq!(view.get_acknowledged(replacement_key), Some(20));
    }

    #[test]
    fn three_pending_generations_keep_independent_durable_positions() {
        let view = CheckpointsView::default();
        let key = FileFingerprint::FirstLinesChecksum(1);
        let generations = [
            next_owner_generation(),
            next_owner_generation(),
            next_owner_generation(),
        ];
        view.register_reader(Some(key), generations[0], 0);
        for (index, generation) in generations.into_iter().enumerate() {
            if index > 0 {
                view.replace_reader(
                    ReaderCheckpoint {
                        fingerprint: key,
                        generation: generations[index - 1],
                        position: 100,
                    },
                    Some(key),
                    generation,
                    100,
                );
            }
            view.record_read(generation, 100, Some(100));
            view.finish_reader(generation, 100);
        }
        let persisted_position = || {
            let State::V1 { checkpoints } = view.get_state();
            assert_eq!(checkpoints.len(), 1);
            checkpoints.first().unwrap().position
        };
        view.acknowledge_reader(generations[2], 100);
        assert_eq!(persisted_position(), 0);
        view.acknowledge_reader(generations[0], 100);
        assert_eq!(persisted_position(), 0);
        view.remove_expired_before(Utc::now() + Duration::hours(1));
        assert_eq!(persisted_position(), 0);
        view.acknowledge_reader(generations[1], 40);
        assert_eq!(persisted_position(), 40);
        view.acknowledge_reader(generations[1], 100);
        assert_eq!(persisted_position(), 100);
        view.remove_expired_before(Utc::now() + Duration::hours(1));
        view.acknowledge_reader(generations[0], 200);
        assert_eq!(view.get(key), None);
    }

    #[test]
    fn reader_index_tracks_rebinding_restart_and_expiry() {
        let view = CheckpointsView::default();
        let first_key = FileFingerprint::FirstLinesChecksum(1);
        let second_key = FileFingerprint::FirstLinesChecksum(2);
        let first = next_owner_generation();
        let second = next_owner_generation();
        let restarted = next_owner_generation();
        let assert_index = || {
            let readers = view.readers.read().unwrap();
            let index = view.reader_fingerprints.read().unwrap();
            let mut expected = std::collections::HashMap::<_, std::collections::HashSet<_>>::new();
            for (generation, reader) in readers.iter() {
                if let Some(fingerprint) = reader.fingerprint {
                    expected.entry(fingerprint).or_default().insert(*generation);
                }
            }
            assert_eq!(*index, expected);
        };
        view.register_reader(Some(first_key), first, 10);
        view.register_reader(None, second, 0);
        assert_index();
        assert!(view.bind_reader(second, first_key));
        assert_eq!(view.get(first_key), Some(0));
        assert_index();
        assert!(view.bind_reader(second, second_key));
        assert_eq!(view.get(first_key), Some(10));
        assert_index();
        assert!(view.restart_reader(second_key, second, restarted));
        assert_index();
        view.finish_reader(first, 10);
        view.remove_expired_before(Utc::now() + Duration::hours(1));
        assert_eq!(view.get(first_key), None);
        assert_eq!(view.get(second_key), Some(0));
        assert_index();
    }

    #[test]
    fn colliding_reader_snapshots_use_the_safe_position() {
        let view = CheckpointsView::default();
        let key = FileFingerprint::FirstLinesChecksum(1);
        let first = next_owner_generation();
        let second = next_owner_generation();
        view.register_reader(Some(key), first, 100);
        view.register_reader(Some(key), second, 0);
        view.acknowledge_reader(first, 200);
        let State::V1 { checkpoints } = view.get_state();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints.first().unwrap().position, 0);
        view.acknowledge_reader(second, 30);
        assert_eq!(view.get_acknowledged(key), Some(30));
    }

    #[test]
    fn a_registered_reader_consumes_its_loaded_checkpoint() {
        let view = CheckpointsView::default();
        let key = FileFingerprint::FirstLinesChecksum(1);
        view.load(Checkpoint {
            fingerprint: key,
            position: 40,
            modified: Utc::now(),
        });
        let generation = next_owner_generation();
        let resumed = view.get(key).unwrap();
        view.register_reader(Some(key), generation, resumed);
        view.record_read(generation, 80, Some(80));
        view.acknowledge_reader(generation, 80);
        assert_eq!(view.get_acknowledged(key), Some(80));
        let State::V1 { checkpoints } = view.get_state();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints.first().unwrap().position, 80);
        view.finish_reader(generation, 80);
        view.remove_expired_before(Utc::now() + Duration::hours(1));
        assert_eq!(
            view.get(key),
            None,
            "the bootstrap entry must not outlive its reader"
        );
    }

    #[test]
    fn discarded_bytes_do_not_require_an_acknowledgement() {
        let view = CheckpointsView::default();
        let key = FileFingerprint::FirstLinesChecksum(1);
        let generation = next_owner_generation();
        view.register_reader(Some(key), generation, 0);
        view.record_read(generation, 10, Some(10));
        // A later batch contains only oversized records discarded by the reader.
        view.record_read(generation, 200, None);
        view.finish_reader(generation, 200);
        let after_expiry = Utc::now() + Duration::hours(1);
        view.remove_expired_before(after_expiry);
        assert_eq!(
            view.get(key),
            Some(200),
            "the emitted record is still pending"
        );
        view.acknowledge_reader(generation, 10);
        view.remove_expired_before(after_expiry);
        assert_eq!(
            view.get(key),
            None,
            "discarded bytes must not pin the reader forever"
        );
    }

    #[test]
    fn final_fragments_keep_a_reader_until_acknowledged() {
        let view = CheckpointsView::default();
        let key = FileFingerprint::FirstLinesChecksum(1);
        let generation = next_owner_generation();
        view.register_reader(Some(key), generation, 0);
        view.record_read(generation, 10, None);
        view.record_emitted([(generation, 10)]);
        view.finish_reader(generation, 10);
        let after_expiry = Utc::now() + Duration::hours(1);
        view.remove_expired_before(after_expiry);
        assert_eq!(view.get_acknowledged(key), Some(0));
        view.acknowledge_reader(generation, 10);
        view.remove_expired_before(after_expiry);
        assert_eq!(view.get(key), None);
    }

    #[test]
    fn replacing_a_reader_does_not_wait_for_its_discarded_tail() {
        let view = CheckpointsView::default();
        let old = FileFingerprint::FirstLinesChecksum(1);
        let new = FileFingerprint::FirstLinesChecksum(2);
        let generation = next_owner_generation();
        view.register_reader(Some(old), generation, 0);
        view.replace_reader(
            ReaderCheckpoint {
                fingerprint: old,
                generation,
                position: 200,
            },
            Some(new),
            next_owner_generation(),
            10,
        );
        view.finish_reader(generation, 200);
        let after_expiry = Utc::now() + Duration::hours(1);
        view.remove_expired_before(after_expiry);
        assert_eq!(view.get_acknowledged(old), Some(0));
        view.acknowledge_reader(generation, 10);
        view.remove_expired_before(after_expiry);
        assert_eq!(view.get(old), None);
        assert_eq!(view.get_acknowledged(new), Some(0));
    }

    #[test]
    fn incomplete_replacement_reserves_its_provisional_checkpoint() {
        let view = CheckpointsView::default();
        let old = FileFingerprint::FirstLinesChecksum(1);
        let new = FileFingerprint::FirstLinesChecksum(2);
        let first = next_owner_generation();
        let second = next_owner_generation();
        view.register_reader(Some(old), first, 100);
        view.replace_reader(
            ReaderCheckpoint {
                fingerprint: old,
                generation: first,
                position: 200,
            },
            None,
            second,
            200,
        );
        view.acknowledge_reader(first, 150);
        view.acknowledge_reader(second, 10);
        let State::V1 { checkpoints } = view.get_state();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints.first().unwrap().position, 0);
        assert!(view.bind_reader(second, new));
        let State::V1 { checkpoints } = view.get_state();
        let positions: std::collections::BTreeMap<_, _> = checkpoints
            .into_iter()
            .map(|checkpoint| (checkpoint.fingerprint, checkpoint.position))
            .collect();
        assert_eq!(positions.get(&old), Some(&150));
        assert_eq!(positions.get(&new), Some(&10));
    }

    #[test]
    fn a_loaded_checkpoint_accepts_no_update_until_a_watcher_registers() {
        let data_dir = tempdir().unwrap();
        let chkptr = Checkpointer::new(data_dir.path());
        let view = chkptr.view();
        let fng = FileFingerprint::FirstLinesChecksum(7);

        view.load(Checkpoint {
            fingerprint: fng,
            position: 500,
            modified: Utc::now(),
        });
        assert_eq!(view.get(fng), Some(500), "the loaded position is readable");

        view.acknowledge_reader(next_owner_generation(), 900);
        assert_eq!(
            view.get(fng),
            Some(500),
            "an unowned checkpoint must not be moved"
        );

        let generation = next_owner_generation();
        view.register_reader(Some(fng), generation, 500);
        view.acknowledge_reader(generation, 900);
        assert_eq!(
            view.get(fng),
            Some(900),
            "the registered owner moves it normally"
        );
    }

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
            let generation = next_owner_generation();
            chkptr
                .checkpoints
                .register_reader(Some(fingerprint), generation, position);
            chkptr.checkpoints.finish_reader(generation, position);

            // slide these in manually so we don't have to sleep for a long time
            chkptr
                .checkpoints
                .readers
                .write()
                .unwrap()
                .get_mut(&generation)
                .unwrap()
                .completed_at = Some(Utc::now() - chrono::Duration::seconds(removed));

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
