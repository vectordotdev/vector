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

/// Hands out the generation numbers. Process-wide, so two watchers never share one.
static NEXT_OWNER_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Claim a generation for a watcher about to be installed on a fingerprint.
pub fn next_owner_generation() -> OwnerGeneration {
    NEXT_OWNER_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Who, if anyone, may move a checkpoint.
///
/// Liveness lives here rather than being inferred from the removal mark: that mark means what it
/// meant upstream -- this file is gone, expire its entry -- and a reaped watcher's queued
/// acknowledgement legitimately refutes it, while leaving the fingerprint just as claimable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owner {
    /// A watcher holds this fingerprint. Only it may write, and a rekey may not take it over.
    Live(OwnerGeneration),
    /// Its watcher has been reaped. Lines it read are still in flight, so its acknowledgements are
    /// accepted, but the fingerprint is free for a rekey to claim.
    Reaped(OwnerGeneration),
    /// Loaded from disk and never claimed: a starting position with no reader behind it.
    Unowned,
}

impl Owner {
    /// The generation entitled to write, whether or not its watcher still exists.
    fn writer(self) -> Option<OwnerGeneration> {
        match self {
            Self::Live(generation) | Self::Reaped(generation) => Some(generation),
            Self::Unowned => None,
        }
    }

    /// Whether a rekey may take this fingerprint over.
    fn claimable(self) -> bool {
        !matches!(self, Self::Live(_))
    }
}

/// A checkpoint together with the watcher entitled to move it.
#[derive(Debug, Clone, Copy)]
struct Owned {
    position: FilePosition,
    owner: Owner,
}

/// A thread-safe handle for reading and writing checkpoints in-memory across
/// multiple threads.
#[derive(Debug, Default)]
pub struct CheckpointsView {
    /// Position and owner in one entry, so a `DashMap` entry lock covers both: checking the owner
    /// and writing the position must not be separate operations, or a rekey landing between them
    /// recreates an entry under a fingerprint nothing owns any more.
    checkpoints: DashMap<FileFingerprint, Owned>,
    modified_times: DashMap<FileFingerprint, DateTime<Utc>>,
    removed_times: DashMap<FileFingerprint, DateTime<Utc>>,
}

impl CheckpointsView {
    /// Record a reader's progress, as reported by an acknowledgement or a direct read.
    ///
    /// Written only if `generation` still owns the fingerprint. An acknowledgement carries the
    /// generation captured when its line was read, so one arriving after its watcher was rekeyed
    /// away -- or after a different file took the fingerprint over -- is refused rather than
    /// resuming that reader past content it never emitted.
    pub fn update(&self, fng: FileFingerprint, pos: FilePosition, generation: OwnerGeneration) {
        // One entry lock covers reading the owner and writing the position. Separate operations
        // would let a rekey land in between and recreate an entry under a dead fingerprint.
        let dashmap::mapref::entry::Entry::Occupied(mut entry) = self.checkpoints.entry(fng) else {
            // Vacant: rekeyed away, reaped, or never registered. Nothing owns it.
            return;
        };
        if entry.get().owner.writer() != Some(generation) {
            return;
        }
        let owner = entry.get().owner;
        entry.get_mut().position = pos;
        self.modified_times.insert(fng, Utc::now());
        if matches!(owner, Owner::Live(_)) {
            // Progress from a live reader means the file is not gone after all. From a reaped one it
            // means only that data it had already read got through, which says nothing about the
            // file -- and clearing the mark there would both keep the entry from ever expiring and
            // make it look live enough to refuse a rekey.
            self.removed_times.remove(&fng);
        }
    }

    /// Install `generation` as the owner of `fng`, starting at `pos`.
    ///
    /// Called by the file server, which is the authority on which watcher holds which fingerprint.
    /// Until this runs, a checkpoint loaded from disk supplies a starting position but accepts no
    /// updates, so a stale acknowledgement cannot be mistaken for a live reader's progress.
    pub fn register(&self, fng: FileFingerprint, pos: FilePosition, generation: OwnerGeneration) {
        // Keep the ownership transition and clearing of the removal mark under the same
        // per-fingerprint entry lock that expiry takes. Otherwise expiry can observe the old
        // Reaped entry after this insert, but before this method clears `removed_times`, and then
        // delete the checkpoint belonging to the new live watcher.
        match self.checkpoints.entry(fng) {
            dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                entry.insert(Owned {
                    position: pos,
                    owner: Owner::Live(generation),
                });
                self.removed_times.remove(&fng);
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(Owned {
                    position: pos,
                    owner: Owner::Live(generation),
                });
                self.removed_times.remove(&fng);
            }
        }
        self.modified_times.insert(fng, Utc::now());
    }

    /// Keep the final position of a reader that was drained before its watcher was repointed.
    ///
    /// The old fingerprint may be discovered again shortly afterwards when a rotated archive is
    /// still inside the include patterns. Starting that watcher from zero would replay the tail
    /// that the drain already emitted. The entry is deliberately Reaped: late acknowledgements from
    /// the drained reader remain valid, while a newly registered watcher may take the fingerprint
    /// over immediately.
    pub fn register_reaped(
        &self,
        fng: FileFingerprint,
        pos: FilePosition,
        generation: OwnerGeneration,
    ) {
        let marked_at = Utc::now();
        let mut installed = false;
        match self.checkpoints.entry(fng) {
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(Owned {
                    position: pos,
                    owner: Owner::Reaped(generation),
                });
                // Hold the checkpoint entry lock while installing the mark, so expiry cannot
                // decide based on a half-installed state.
                self.removed_times.insert(fng, marked_at);
                installed = true;
            }
            dashmap::mapref::entry::Entry::Occupied(_) => {}
        }
        if installed {
            self.modified_times.insert(fng, marked_at);
        }
    }

    pub fn get(&self, fng: FileFingerprint) -> Option<FilePosition> {
        self.checkpoints.get(&fng).map(|r| r.value().position)
    }

    /// The watcher `generation` owned `fng` and has been reaped; its checkpoint is kept for the
    /// expiry window so a file that comes back can resume from it.
    ///
    /// Its right to write survives: lines it read are already on their way out when it is reaped,
    /// and their acknowledgements arrive later still. Refusing those would leave the checkpoint
    /// behind data that was genuinely emitted, and a restart would replay the last batch. The
    /// fingerprint becomes claimable all the same, so a dead owner cannot lock out a rekey.
    ///
    /// Qualified by generation: a death notice for a watcher that has already been replaced must not
    /// retire the replacement.
    pub fn set_dead(&self, fng: FileFingerprint, generation: OwnerGeneration) {
        // The mark goes with the transition, not beside it. A notice naming a watcher that has since
        // been replaced retires nothing, so marking regardless would leave a live replacement
        // carrying someone else's death sentence -- and `remove_expired` would delete its checkpoint
        // if it happened to stay idle.
        let marked_at = Utc::now();
        match self.checkpoints.entry(fng) {
            dashmap::mapref::entry::Entry::Occupied(mut entry)
                if entry.get().owner == Owner::Live(generation) =>
            {
                entry.get_mut().owner = Owner::Reaped(generation);
                self.removed_times.insert(fng, marked_at);
            }
            // No entry at all: nothing to contradict the notice, and upstream marked it too.
            dashmap::mapref::entry::Entry::Vacant(_) => {}
            dashmap::mapref::entry::Entry::Occupied(_) => {}
        }
    }

    /// Move a checkpoint onto the fingerprint its file now hashes to, under a new owner.
    ///
    /// `new_generation` is the rekeyed watcher's: the move is an ownership transition, and reusing
    /// the old number would let acknowledgements already in flight for `old` write to `new`.
    ///
    /// The old key is *removed* rather than tombstoned, which is what makes those in-flight
    /// acknowledgements harmless: they find a vacant entry and are dropped.
    pub fn update_key(
        &self,
        old: FileFingerprint,
        new: FileFingerprint,
        new_generation: OwnerGeneration,
    ) {
        if old == new {
            return;
        }
        let Some((_, moved)) = self.checkpoints.remove(&old) else {
            return;
        };
        // An entry a *live* watcher owns is left alone: two files can share a fingerprint, and the
        // file server refuses such a rekey, but the maps must not depend on that check holding
        // across an await.
        //
        // An unowned one is replaced. A checkpoint left behind by a reaped watcher, or loaded from
        // disk and never claimed, describes a reader that no longer exists; skipping it would leave
        // this watcher holding a fresh generation against a stale entry, and every checkpoint it
        // ever writes would be refused.
        // One entry lock covers the test and the write, like [`Self::update`]: reading the owner and
        // replacing it must not be separate operations.
        let claimed = Owned {
            position: moved.position,
            owner: Owner::Live(new_generation),
        };
        let taken_over = match self.checkpoints.entry(new) {
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(claimed);
                true
            }
            dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                // Never claimed, or claimed by a watcher that has since been reaped. The removal
                // mark is read inside the entry lock, so a `set_dead` cannot land between the test
                // and the write and leave this rekey locked out by an owner that just died.
                let claimable = entry.get().owner.claimable();
                if claimable {
                    entry.insert(claimed);
                }
                claimable
            }
        };

        if let Some((_, value)) = self.modified_times.remove(&old) {
            self.modified_times.insert(new, value);
        }

        let old_removal = self.removed_times.remove(&old).map(|(_, value)| value);
        if taken_over {
            // A live watcher now holds this fingerprint, so any death mark it carried is void --
            // whether inherited from the old key or left by whoever held it before. Without this,
            // `remove_expired` reaps a perfectly live checkpoint 60 seconds later.
            self.removed_times.remove(&new);
        } else if let Some(value) = old_removal {
            self.removed_times.insert(new, value);
        }
    }

    pub fn remove_expired(&self) {
        self.remove_expired_before(Utc::now());
    }

    /// [`Self::remove_expired`] against a given instant, so a test need not wait out the window.
    fn remove_expired_before(&self, now: DateTime<Utc>) {
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
            .map(|entry| (*entry.key(), *entry.value()))
            .collect::<Vec<(FileFingerprint, DateTime<Utc>)>>();

        for (fng, marked_at) in to_remove {
            // The list above is a hint, not a decision: a watcher can take this fingerprint over
            // between collecting it and deleting it, and `register` clears the mark. Deleting on the
            // stale verdict would discard every checkpoint that new owner writes, so a restart
            // replays the file from the beginning. The mark is re-read under the entry lock and must
            // still be the one collected -- a fingerprint reaped, revived and reaped again has a
            // newer mark, whose own window has not elapsed yet.
            let dashmap::mapref::entry::Entry::Occupied(entry) = self.checkpoints.entry(fng) else {
                self.modified_times.remove(&fng);
                self.removed_times.remove(&fng);
                continue;
            };
            let still_marked = self
                .removed_times
                .get(&fng)
                .is_some_and(|mark| *mark.value() == marked_at);
            let still_reaped = matches!(entry.get().owner, Owner::Reaped(_));
            if !still_marked || !still_reaped {
                continue;
            }
            entry.remove();
            self.modified_times.remove(&fng);
            self.removed_times.remove(&fng);
        }
    }

    fn load(&self, checkpoint: Checkpoint) {
        // No generation: a loaded checkpoint supplies a starting position, but nothing owns it until
        // a watcher registers, so no acknowledgement can move it.
        self.checkpoints.insert(
            checkpoint.fingerprint,
            Owned {
                position: checkpoint.position,
                owner: Owner::Unowned,
            },
        );
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
                    let position = entry.value().position;
                    Checkpoint {
                        fingerprint: *fingerprint,
                        position,
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
        self.checkpoints
            .register(fng, pos, crate::checkpointer::next_owner_generation());
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
        CHECKPOINT_FILE_NAME, Checkpoint, Checkpointer, CheckpointsView, FileFingerprint,
        FilePosition, TMP_FILE_NAME, next_owner_generation,
    };

    /// Regression test for a bug found in review: with acknowledgements enabled, the acking task
    /// holds the fingerprint captured when the line was read. One arriving after a rewrite rekeyed
    /// the file recorded a checkpoint under the dead fingerprint, which nothing reaped.
    #[test]
    fn a_late_acknowledgement_from_a_previous_owner_is_dropped() {
        let view = CheckpointsView::default();
        let old = FileFingerprint::FirstLinesChecksum(1);
        let new = FileFingerprint::FirstLinesChecksum(2);

        let first = next_owner_generation();
        view.register(old, 100, first);

        // The rewrite rekeys the file and installs a new owner for the new identity.
        let second = next_owner_generation();
        view.update_key(old, new, second);
        assert_eq!(view.get(new), Some(100), "the rekey carries the position");

        // An acknowledgement for a line read before the rewrite, stamped with the old owner.
        view.update(old, 150, first);
        assert_eq!(
            view.get(old),
            None,
            "a late acknowledgement must not recreate the rekeyed-away entry"
        );
        assert_eq!(
            view.get(new),
            Some(100),
            "and must not move the new owner checkpoint"
        );

        // The new owner records its own progress.
        view.update(new, 4200, second);
        assert_eq!(view.get(new), Some(4200), "the live owner is accepted");
    }

    /// Regression test for a bug found in review: a genuinely *different* file whose first lines
    /// hash to a value a rewrite had just retired was silenced for 60 seconds, so a restart in that
    /// window reread it from the beginning.
    #[test]
    fn a_new_owner_of_a_reused_fingerprint_checkpoints_immediately() {
        let view = CheckpointsView::default();
        let old = FileFingerprint::FirstLinesChecksum(1);
        let new = FileFingerprint::FirstLinesChecksum(2);

        let first = next_owner_generation();
        view.register(old, 100, first);
        let second = next_owner_generation();
        view.update_key(old, new, second);

        // An unrelated file now hashes to the value the rewrite left behind.
        let third = next_owner_generation();
        view.register(old, 0, third);
        view.update(old, 64, third);
        assert_eq!(
            view.get(old),
            Some(64),
            "a new owner of a reused fingerprint must record progress at once"
        );

        // The previous owner of that same value is still refused.
        view.update(old, 150, first);
        assert_eq!(
            view.get(old),
            Some(64),
            "a previous owner must not move the new owner checkpoint"
        );
    }

    /// Regression test for a bug found in review: a rekey onto a fingerprint that already had a
    /// checkpoint -- left by a reaped watcher, or loaded from disk and never claimed -- skipped the
    /// move, so the rekeyed watcher held a fresh generation against a stale entry and every
    /// checkpoint it wrote was refused.
    #[test]
    fn a_rekey_onto_an_unowned_checkpoint_takes_it_over() {
        let view = CheckpointsView::default();
        let old = FileFingerprint::FirstLinesChecksum(1);
        let new = FileFingerprint::FirstLinesChecksum(2);

        // A checkpoint for `new` survives from a previous run, owned by nobody.
        view.load(Checkpoint {
            fingerprint: new,
            position: 900,
            modified: Utc::now(),
        });

        let first = next_owner_generation();
        view.register(old, 100, first);
        let second = next_owner_generation();
        view.update_key(old, new, second);

        view.update(new, 128, second);
        assert_eq!(
            view.get(new),
            Some(128),
            "the rekeyed watcher must own the fingerprint it moved onto"
        );
    }

    /// Regression test for a bug found in review: a checkpoint kept after its watcher was reaped
    /// still carried that watcher's generation, so it looked live. A rekey onto it during the
    /// retention window was refused, and the rekeyed watcher could never persist progress.
    #[test]
    fn a_rekey_onto_a_dead_owners_checkpoint_takes_it_over() {
        let view = CheckpointsView::default();
        let old = FileFingerprint::FirstLinesChecksum(1);
        let new = FileFingerprint::FirstLinesChecksum(2);

        // A watcher owned `new`, then went away -- its position is retained for the expiry window.
        let departed = next_owner_generation();
        view.register(new, 900, departed);
        view.set_dead(new, departed);

        let first = next_owner_generation();
        view.register(old, 100, first);
        let second = next_owner_generation();
        view.update_key(old, new, second);

        view.update(new, 128, second);
        assert_eq!(
            view.get(new),
            Some(128),
            "a rekey must take over a fingerprint whose owner is gone"
        );

        // Only once the fingerprint has a new owner is the departed one refused.
        view.update(new, 900, departed);
        assert_eq!(
            view.get(new),
            Some(128),
            "a replaced owner must not move the checkpoint it left behind"
        );
    }

    /// Regression test for a bug found in review: taking over a dead owner's fingerprint left its
    /// removal mark in place, so `remove_expired` reaped the live watcher's checkpoint 60 seconds
    /// later and a restart reread or skipped data.
    #[test]
    fn a_rekey_clears_the_death_mark_it_inherits() {
        let view = CheckpointsView::default();
        let old = FileFingerprint::FirstLinesChecksum(1);
        let new = FileFingerprint::FirstLinesChecksum(2);

        let departed = next_owner_generation();
        view.register(new, 900, departed);
        view.set_dead(new, departed);

        let first = next_owner_generation();
        view.register(old, 100, first);
        let second = next_owner_generation();
        view.update_key(old, new, second);

        // Past the retention window. The rekeyed entry is live and must survive it.
        view.remove_expired_before(Utc::now() + chrono::Duration::seconds(120));
        assert_eq!(
            view.get(new),
            Some(100),
            "a rekey onto a dead owner must clear the death mark it inherits"
        );
    }

    /// Regression test for a bug found in review: accepting a reaped owner's queued acknowledgement
    /// also cleared the removal mark, which at the time was what made the fingerprint claimable. A
    /// replacement could then never take it over, and the dead entry never expired either.
    #[test]
    fn a_late_acknowledgement_does_not_block_a_later_rekey() {
        let view = CheckpointsView::default();
        let old = FileFingerprint::FirstLinesChecksum(1);
        let new = FileFingerprint::FirstLinesChecksum(2);

        let departed = next_owner_generation();
        view.register(new, 900, departed);
        view.set_dead(new, departed);

        // A line the reaped watcher had already read gets through.
        view.update(new, 980, departed);
        assert_eq!(
            view.get(new),
            Some(980),
            "the queued acknowledgement must still commit"
        );

        // A replacement rekeys onto the same fingerprint afterwards.
        let first = next_owner_generation();
        view.register(old, 100, first);
        let second = next_owner_generation();
        view.update_key(old, new, second);
        view.update(new, 128, second);
        assert_eq!(
            view.get(new),
            Some(128),
            "a late acknowledgement must not lock the fingerprint against a rekey"
        );
    }

    /// The other half of the same rule: a reaped watcher's acknowledgement records progress but
    /// says nothing about the file, so it must not cancel the expiry the reaping started. Otherwise
    /// the entry for a file that is gone is kept alive indefinitely by its own trailing traffic.
    #[test]
    fn a_late_acknowledgement_does_not_keep_a_dead_entry_alive() {
        let view = CheckpointsView::default();
        let fng = FileFingerprint::FirstLinesChecksum(1);

        let departed = next_owner_generation();
        view.register(fng, 100, departed);
        view.set_dead(fng, departed);
        view.update(fng, 180, departed);

        view.remove_expired_before(Utc::now() + chrono::Duration::seconds(120));
        assert_eq!(
            view.get(fng),
            None,
            "a dead entry must still expire, however late its own acknowledgements arrive"
        );
    }

    /// A death notice names the watcher it is about, so one for a watcher that has already been
    /// replaced cannot retire the replacement.
    #[test]
    fn a_stale_death_notice_does_not_retire_the_replacement() {
        let view = CheckpointsView::default();
        let fng = FileFingerprint::FirstLinesChecksum(1);

        let departed = next_owner_generation();
        view.register(fng, 100, departed);

        // A different file takes the fingerprint over.
        let replacement = next_owner_generation();
        view.register(fng, 0, replacement);

        // The previous watcher is only now reaped.
        view.set_dead(fng, departed);

        view.update(fng, 64, replacement);
        assert_eq!(
            view.get(fng),
            Some(64),
            "the replacement must still own its fingerprint"
        );
        view.update(fng, 900, departed);
        assert_eq!(
            view.get(fng),
            Some(64),
            "and the departed watcher must not write to it"
        );
    }

    /// The same stale notice must not leave a death mark on the replacement: a watcher that then
    /// stays idle -- writing nothing to clear it -- would have its live checkpoint expired away.
    #[test]
    fn a_stale_death_notice_does_not_expire_an_idle_replacement() {
        let view = CheckpointsView::default();
        let fng = FileFingerprint::FirstLinesChecksum(1);

        let departed = next_owner_generation();
        view.register(fng, 100, departed);
        let replacement = next_owner_generation();
        view.register(fng, 64, replacement);

        // The previous watcher is reaped only now, and the replacement reads nothing afterwards.
        view.set_dead(fng, departed);

        view.remove_expired_before(Utc::now() + chrono::Duration::seconds(120));
        assert_eq!(
            view.get(fng),
            Some(64),
            "a stale death notice must not expire the replacement's checkpoint"
        );
    }

    /// Lines a watcher read are already queued for output when it is reaped, and their
    /// acknowledgements arrive later still. Refusing those would leave the checkpoint behind data
    /// that was genuinely emitted, so a restart would replay the last batch.
    #[test]
    fn a_reaped_watcher_still_commits_its_queued_lines() {
        let view = CheckpointsView::default();
        let fng = FileFingerprint::FirstLinesChecksum(1);

        let generation = next_owner_generation();
        view.register(fng, 100, generation);
        view.set_dead(fng, generation);

        view.update(fng, 180, generation);
        assert_eq!(
            view.get(fng),
            Some(180),
            "an acknowledgement for a line read before the watcher died must still commit"
        );
    }

    /// The other half of the same branch: a fingerprint a *live* watcher owns is never taken over,
    /// since two files can share one and the rekey would evict a legitimate reader.
    #[test]
    fn a_rekey_onto_a_live_owner_leaves_it_alone() {
        let view = CheckpointsView::default();
        let old = FileFingerprint::FirstLinesChecksum(1);
        let new = FileFingerprint::FirstLinesChecksum(2);

        let live = next_owner_generation();
        view.register(new, 900, live);

        let first = next_owner_generation();
        view.register(old, 100, first);
        let second = next_owner_generation();
        view.update_key(old, new, second);

        assert_eq!(view.get(new), Some(900), "a live owner keeps its position");
        view.update(new, 950, live);
        assert_eq!(
            view.get(new),
            Some(950),
            "and keeps writing its own checkpoints"
        );
    }

    /// A checkpoint read from disk supplies a starting position but has no live reader, so nothing
    /// may move it until a watcher registers -- otherwise a stale acknowledgement arriving early
    /// would be mistaken for the new reader progress.
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

        view.update(fng, 900, next_owner_generation());
        assert_eq!(
            view.get(fng),
            Some(500),
            "an unowned checkpoint must not be moved"
        );

        let generation = next_owner_generation();
        view.register(fng, 500, generation);
        view.update(fng, 900, generation);
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
                .register(fingerprint, position, generation);
            chkptr.checkpoints.set_dead(fingerprint, generation);

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
