#![deny(warnings)]
#![deny(clippy::all)]

pub mod buffer;
pub mod checkpointer;
mod fingerprinter;
pub mod internal_events;
mod metadata_ext;

use vector_config::configurable_component;

pub use self::{
    checkpointer::{
        CHECKPOINT_FILE_NAME, Checkpointer, CheckpointsView, OwnerGeneration, next_owner_generation,
    },
    fingerprinter::{
        FileFingerprint, FingerprintOutcome, FingerprintStrategy, Fingerprinter, PartialPrefix,
        PrefixWanted,
    },
    internal_events::FileSourceInternalEvents,
    metadata_ext::{AsyncFileInfo, PortableFileExt},
};

pub type FilePosition = u64;

/// Make `path` absolute the same way the `notify` crate does internally before using a path
/// passed to `watch()` -- joined onto `cwd` if relative, unchanged if already absolute. Needed
/// wherever a path from a glob-based `PathsProvider`/`include` pattern (which can be relative) is
/// compared against a path `notify` reports in an event (always absolute). `cwd` is `None` only
/// if `std::env::current_dir()` itself failed, in which case `path` is returned unchanged.
pub fn absolutize(path: &std::path::Path, cwd: Option<&std::path::Path>) -> std::path::PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    match cwd {
        Some(cwd) => cwd.join(path),
        None => path.to_path_buf(),
    }
}

/// What `known_small_files` records for a file whose fingerprint is not yet available.
///
/// Keyed by canonical identity, so every spelling of one file -- relative, dotted, or reached
/// through a symlink -- is one entry. `remove_after` must unlink a path the configuration actually
/// names, never the canonical target: deleting the target of a symlinked include would remove a file
/// outside the watched path, possibly shared with something else. So every spelling the file has been
/// seen under is kept, and removal uses one of them.
#[derive(Clone, Debug)]
pub struct SmallFile {
    /// When the file was first seen too short to fingerprint.
    pub first_seen: std::time::Instant,
    /// Every path the caller has named this file by. Non-empty.
    removal_paths: std::collections::BTreeSet<std::path::PathBuf>,
    /// `(dev, ino)` of the handle the fingerprint was last read from, when it could be taken.
    ///
    /// The entry is keyed by canonical path, which a replacement inherits: without this, a file
    /// replaced while still too short would take over the old one's age and `remove_after` could
    /// unlink it long before its own grace period elapsed.
    read_identity: Option<(u64, u64)>,
    /// Whether the identity was the *first* spelling this file was recorded under.
    ///
    /// That distinguishes the two ways the identity can appear among the spellings, which the path
    /// alone cannot: an ordinary file is discovered by the glob pass under its real path, so identity
    /// and configured path coincide and unlinking it is exactly right. A symlinked include is instead
    /// recorded under its alias first, and only later does a notify backend report the canonical
    /// target -- which may live outside the include and be shared, so it must never be unlinked.
    identity_recorded_first: bool,
}

impl SmallFile {
    /// A path `remove_after` may unlink for the file at `identity`, if any is safe.
    ///
    /// A spelling other than the identity is always safe -- the configuration named it. The identity
    /// itself only when it was recorded first (see `identity_recorded_first`); otherwise `None`, since
    /// an alias that was later taken over can leave the canonical target as the only spelling, and
    /// unlinking that deletes a file the include never named.
    ///
    /// Among several the lowest is taken, purely so the choice is stable and reproducible.
    fn removal_path(&self, identity: &std::path::Path) -> Option<&std::path::Path> {
        self.removal_paths
            .iter()
            .map(std::path::PathBuf::as_path)
            .find(|path| *path != identity)
            .or_else(|| {
                self.identity_recorded_first
                    .then(|| self.removal_paths.first().map(std::path::PathBuf::as_path))
                    .flatten()
            })
    }
}

/// The set of files seen too short to fingerprint, awaiting either a complete record or
/// `remove_after`.
///
/// Keyed by canonical identity, with each entry holding *every* configured spelling of its file, so
/// there is no single "correct" path for two structures to disagree about. `owner_by_path` is a
/// derived lookup for speed only -- no decision is read out of it.
#[derive(Debug, Default)]
pub struct KnownSmallFiles {
    by_identity: std::collections::HashMap<std::path::PathBuf, SmallFile>,
    /// Which identity currently holds each recorded spelling.
    ///
    /// Without it, taking a spelling over from another file scans every entry, and during initial
    /// discovery every spelling is new -- measured at 24s for 20k short files, quadratic.
    owner_by_path: std::collections::HashMap<std::path::PathBuf, std::path::PathBuf>,
}

impl KnownSmallFiles {
    /// Record `removal_path` as a spelling of the file at `identity`, too short to fingerprint.
    ///
    /// Returns `true` if this is a newly recorded file, so the caller emits its event only once.
    ///
    /// An identity already recorded keeps its original `first_seen`: this runs on every discovery
    /// pass for a file that is still too short, so taking the new timestamp would restart the
    /// `remove_after` clock each pass and a permanently short file would never expire.
    ///
    /// The exception is `read_identity` changing, which means a *different* file now answers to this
    /// path and is entitled to its own grace period. Size and mtime cannot stand in for it: a
    /// replacement can be longer and newer than what it replaced, exactly like an ordinary append.
    pub fn insert(
        &mut self,
        identity: std::path::PathBuf,
        removal_path: &std::path::Path,
        first_seen: std::time::Instant,
        read_identity: Option<(u64, u64)>,
    ) -> bool {
        let (is_new_file, is_new_spelling) = match self.by_identity.entry(identity.clone()) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                let entry = entry.get_mut();
                // Only when both are known: an unreadable identity is absence of evidence, and
                // treating it as a change would restart the clock on every pass that lost the race.
                if let (Some(recorded), Some(observed)) = (entry.read_identity, read_identity)
                    && recorded != observed
                {
                    entry.first_seen = first_seen;
                }
                if read_identity.is_some() {
                    entry.read_identity = read_identity;
                }
                (false, entry.removal_paths.insert(removal_path.to_owned()))
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                let identity_recorded_first = removal_path == entry.key().as_path();
                entry.insert(SmallFile {
                    first_seen,
                    removal_paths: std::collections::BTreeSet::from([removal_path.to_owned()]),
                    identity_recorded_first,
                    read_identity,
                });
                (true, true)
            }
        };

        // A path names one file, so a spelling this entry did not already hold has to be taken away
        // from whichever other identity held it -- that file was replaced, and leaving the spelling
        // behind would let `remove_after` unlink the replacement.
        //
        // Guarded on the spelling being new, which is what keeps discovery linear: the repeat
        // sighting of an already-recorded short file is the common case and costs one hash lookup.
        // Doing this scan unconditionally measured 24s for 20k short files (quadratic), the very
        // cost the index this type replaced was introduced to remove.
        if is_new_spelling {
            self.release_path_from_others(&identity, removal_path);
            self.owner_by_path.insert(removal_path.to_owned(), identity);
        }
        is_new_file
    }

    /// Drop `removal_path` from whichever other entry holds it, discarding that entry if the path was
    /// its last spelling.
    fn release_path_from_others(
        &mut self,
        identity: &std::path::Path,
        removal_path: &std::path::Path,
    ) {
        let Some(previous) = self.owner_by_path.get(removal_path) else {
            return;
        };
        if previous == identity {
            return;
        }
        let previous = previous.clone();
        if let Some(entry) = self.by_identity.get_mut(&previous) {
            entry.removal_paths.remove(removal_path);
            if entry.removal_paths.is_empty() {
                self.by_identity.remove(&previous);
            }
        }
    }

    /// Forget the file at `identity`, and any file still recorded under `removal_path`. Called once
    /// a file fingerprints, or when it turns out not to exist.
    pub fn remove(&mut self, identity: &std::path::Path, removal_path: &std::path::Path) {
        // Every spelling of the removed entry leaves the index with it. Clearing only the path
        // passed in was a review finding: a file named by three or more spellings left the rest
        // pointing at an identity that no longer existed.
        if let Some(entry) = self.by_identity.remove(identity) {
            for path in &entry.removal_paths {
                self.owner_by_path.remove(path);
            }
        }
        // `removal_path` may belong to a *different* file -- the caller saw it vanish or fingerprint
        // under one identity while the map recorded it under another. Drop it from that entry too.
        if let Some(previous) = self.owner_by_path.remove(removal_path)
            && let Some(entry) = self.by_identity.get_mut(&previous)
        {
            entry.removal_paths.remove(removal_path);
            // Dropped when nothing *removable* is left, not merely when the set empties: an entry left
            // with only its own identity has no safe removal path, so `expired` never returns it.
            if entry.removal_path(&previous).is_none() {
                let stranded: Vec<_> = entry.removal_paths.iter().cloned().collect();
                self.by_identity.remove(&previous);
                for path in stranded {
                    self.owner_by_path.remove(&path);
                }
            }
        }
    }

    /// Entries older than `grace_period`, as `(identity, removal path)` pairs.
    pub fn expired(
        &self,
        grace_period: std::time::Duration,
    ) -> Vec<(std::path::PathBuf, std::path::PathBuf)> {
        self.by_identity
            .iter()
            .filter(|(_, entry)| entry.first_seen.elapsed() >= grace_period)
            // An entry with no safe spelling left is skipped, not unlinked by its canonical path.
            .filter_map(|(identity, entry)| {
                Some((identity.clone(), entry.removal_path(identity)?.to_owned()))
            })
            .collect()
    }

    /// A path `remove_after` may unlink for `identity`, if it is still recorded.
    pub fn removal_path(&self, identity: &std::path::Path) -> Option<&std::path::Path> {
        self.by_identity
            .get(identity)
            .and_then(|entry| entry.removal_path(identity))
    }

    /// Drop one spelling of `identity` that turned out not to exist, dropping the entry when nothing
    /// removable is left.
    pub fn forget_missing_spelling(
        &mut self,
        identity: &std::path::Path,
        missing: &std::path::Path,
    ) {
        let Some(entry) = self.by_identity.get_mut(identity) else {
            return;
        };
        entry.removal_paths.remove(missing);
        self.owner_by_path.remove(missing);
        if entry.removal_path(identity).is_none() {
            let stranded: Vec<_> = entry.removal_paths.iter().cloned().collect();
            self.by_identity.remove(identity);
            for path in stranded {
                self.owner_by_path.remove(&path);
            }
        }
    }

    /// Whether a file is recorded under `identity`.
    pub fn contains_identity(&self, identity: &std::path::Path) -> bool {
        self.by_identity.contains_key(identity)
    }

    pub fn is_empty(&self) -> bool {
        self.by_identity.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_identity.len()
    }
}

/// Remove `.` components, leaving `..` exactly where it is.
///
/// This is what `glob` does to the candidates it emits, measured: `./logs/*.log` yields
/// `logs/app.log` with the `.` gone, while `logs/../logs/*.log` yields `logs/../logs/app.log` with
/// the `..` intact. Reconstructing the spelling `paths()` would have produced -- which is what the
/// raw exclusion patterns are compared against -- therefore has to drop one and keep the other.
pub fn strip_current_dir_components(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;

    let stripped: std::path::PathBuf = path
        .components()
        .filter(|component| !matches!(component, Component::CurDir))
        .collect();
    // An all-`.` path keeps its meaning only as `.`.
    if stripped.as_os_str().is_empty() && path.as_os_str() != "" {
        return std::path::PathBuf::from(".");
    }
    stripped
}

/// Remove `.` components and resolve `..` lexically, without touching the filesystem.
///
/// Needed when a glob *pattern* is compared against a path a notify backend reported: `Pattern`
/// matches component text literally, so `<cwd>/./logs/*.log` does not match `<cwd>/logs/app.log`
/// (verified), and a legitimate `./logs/*.log` include would silently stop matching its own files.
/// Resolving `..` lexically rather than via `canonicalize` keeps this usable for paths that do not
/// exist yet, and keeps it free of syscalls on a per-event path.
pub fn lexically_normalize(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;

    let mut out = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Only pop a real directory name; `..` above the root, or after another `..` in a
                // relative path, has to be preserved to keep the path's meaning.
                let pops_a_name = out
                    .components()
                    .next_back()
                    .is_some_and(|last| matches!(last, Component::Normal(_)));
                if pops_a_name {
                    out.pop();
                } else {
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// Normalize a path used as a key in maps shared between glob-driven and notify-driven discovery
/// (for example `known_small_files`). The glob may yield a relative path while notify always
/// reports an absolute one, so the same file must not produce two distinct keys.
pub fn normalize_path_key(path: &std::path::Path) -> std::path::PathBuf {
    // `.` and `..` are resolved too, not just made absolute: a glob pass under an include such as
    // `./logs/*.log` inserts `<cwd>/./logs/file.log` while notify reports `<cwd>/logs/file.log`, and
    // two spellings of one file mean a successful fingerprint fails to remove the earlier
    // `known_small_files` entry -- leaving `remove_after` free to delete a file that is now valid.
    //
    // `current_dir()` is still skipped for an already-absolute path (the common case, and every path
    // on the notify side): normalizing is pure string work, so only the syscall is worth avoiding.
    if path.is_absolute() {
        return lexically_normalize(path);
    }
    lexically_normalize(&absolutize(path, std::env::current_dir().ok().as_deref()))
}

#[cfg(test)]
mod path_tests {
    use std::path::PathBuf;

    use super::{absolutize, normalize_path_key};

    #[test]
    fn leaves_absolute_paths_unchanged() {
        let cwd = PathBuf::from("/home/user/project");
        let absolute = PathBuf::from("/var/log/app.log");
        assert_eq!(absolutize(&absolute, Some(&cwd)), absolute);
    }

    #[test]
    fn joins_relative_paths_onto_cwd() {
        let cwd = PathBuf::from("/home/user/project");
        let relative = PathBuf::from("logs/app.log");
        assert_eq!(
            absolutize(&relative, Some(&cwd)),
            PathBuf::from("/home/user/project/logs/app.log")
        );
    }

    #[test]
    fn falls_back_to_the_relative_path_when_cwd_is_unknown() {
        let relative = PathBuf::from("logs/app.log");
        assert_eq!(absolutize(&relative, None), relative);
    }

    #[test]
    fn normalizes_relative_and_absolute_spellings_to_one_key() {
        let relative = PathBuf::from("logs/app.log");
        let absolute = std::env::current_dir().unwrap().join(&relative);
        assert_eq!(
            normalize_path_key(&relative),
            normalize_path_key(&absolute),
            "a relative and an absolute spelling of one file must share a map key"
        );
    }
}

#[cfg(test)]
mod known_small_files_tests {
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };

    use super::KnownSmallFiles;

    /// Regression test for a bug found in review: `insert` runs on every discovery pass for a file
    /// that is still too short, and overwriting the entry reset `first_seen` each time. With
    /// `remove_after` longer than the discovery interval, the age never reached the grace period and
    /// a permanently short file was never removed -- defeating `remove_after` for its main case.
    #[test]
    fn reinserting_a_short_file_keeps_its_original_timestamp() {
        let mut known = KnownSmallFiles::default();
        let identity = PathBuf::from("/logs/app.log");
        let path = PathBuf::from("/logs/app.log");

        let first_seen = Instant::now() - Duration::from_secs(60);
        assert!(
            known.insert(identity.clone(), &path, first_seen, None),
            "the first sighting is newly recorded"
        );
        assert!(
            !known.insert(identity.clone(), &path, Instant::now(), None),
            "a repeat sighting is not newly recorded"
        );

        let expired = known.expired(Duration::from_secs(30));
        assert_eq!(
            expired,
            vec![(identity, path)],
            "a file first seen 60s ago must expire against a 30s grace period, however often it \
             has been seen since"
        );
    }

    /// Regression test for a bug found in review: entries are keyed by canonical path, so a file
    /// replaced at that path while still too short inherited its predecessor's age and
    /// `remove_after` could unlink it almost immediately.
    #[test]
    fn replacing_a_short_file_restarts_its_grace_period() {
        let mut known = KnownSmallFiles::default();
        let identity = PathBuf::from("/logs/app.log");

        let first_seen = Instant::now() - Duration::from_secs(60);
        known.insert(identity.clone(), &identity, first_seen, Some((1, 10)));
        assert_eq!(
            known.expired(Duration::from_secs(30)).len(),
            1,
            "the original file is old enough to expire"
        );

        // Same path, different inode: a new file, entitled to its own grace period.
        known.insert(identity.clone(), &identity, Instant::now(), Some((1, 11)));
        assert!(
            known.expired(Duration::from_secs(30)).is_empty(),
            "a replacement must not inherit the age of the file it replaced"
        );

        // An identity that could not be read is not evidence of a replacement, or a pass that lost
        // the race would restart the clock and a permanently short file would never expire.
        let mut known = KnownSmallFiles::default();
        known.insert(identity.clone(), &identity, first_seen, Some((1, 10)));
        known.insert(identity.clone(), &identity, Instant::now(), None);
        assert_eq!(
            known.expired(Duration::from_secs(30)).len(),
            1,
            "an unreadable identity must leave the recorded timestamp alone"
        );
    }

    /// Regression test for a bug found in review: one canonical file reachable through two
    /// configured aliases kept a reverse-index entry for the alias it was no longer stored under.
    /// Replacing the file at that stale alias then removed the *original* identity from
    /// `by_identity`, so `remove_after` lost track of a file that was still short.
    #[test]
    fn replacing_a_file_at_a_stale_alias_keeps_the_original_tracked() {
        let mut known = KnownSmallFiles::default();
        let identity = PathBuf::from("/logs/canonical.log");
        let first_alias = PathBuf::from("/logs/a.log");
        let second_alias = PathBuf::from("/logs/b.log");
        let first_seen = Instant::now() - Duration::from_secs(60);

        // The same canonical file, observed through two configured spellings.
        known.insert(identity.clone(), &first_alias, first_seen, None);
        known.insert(identity.clone(), &second_alias, first_seen, None);
        assert_eq!(known.len(), 1, "two aliases of one file are one entry");

        // A different short file now occupies the alias the entry is no longer stored under.
        let other = PathBuf::from("/logs/other.log");
        known.insert(other.clone(), &first_alias, Instant::now(), None);

        assert!(
            known.contains_identity(&identity),
            "the original file is still short and must stay tracked for remove_after"
        );
        assert!(
            known.contains_identity(&other),
            "the replacement at the reused alias must be tracked too"
        );
    }

    /// Regression test for a bug found in review, introduced by the round 26 fix itself: refreshing
    /// `removal_path` on every sighting let a notify backend reporting the *canonical target* of a
    /// symlinked include overwrite the configured link. `remove_after` would then unlink the target
    /// -- a file outside the include, possibly shared -- which is the exact hazard `removal_path`
    /// exists to prevent. The first spelling recorded is the configured one, so it is kept.
    #[test]
    fn an_existing_identity_keeps_its_configured_removal_path() {
        let mut known = KnownSmallFiles::default();
        // The canonical target sorts *before* the configured link, so "lowest path wins" alone would
        // pick the target -- which is the unsafe answer.
        let identity = PathBuf::from("/a-shared/target.log");
        let configured_link = PathBuf::from("/logs/app.log");
        let first_seen = Instant::now() - Duration::from_secs(60);

        // Discovered through the configured symlink. The link sorts *after* the target, so a
        // "lowest path wins" tie-break would pick the target and this test would pass for the wrong
        // reason -- the point is that the target is never chosen while a configured spelling exists.
        assert!(
            configured_link > identity,
            "test setup requires the link to sort after the target"
        );
        known.insert(identity.clone(), &configured_link, first_seen, None);
        // A notify event names the canonical target of the same file.
        known.insert(identity.clone(), &identity, Instant::now(), None);

        assert_eq!(
            known.removal_path(&identity),
            Some(configured_link.as_path()),
            "remove_after must unlink the configured path, never the symlink's target"
        );
        assert_eq!(
            known.expired(Duration::from_secs(30)),
            vec![(identity, configured_link)],
            "the expiry entry must also carry the configured path"
        );
    }

    /// Regression test for a bug found in review: when an identity survives because another alias
    /// still reaches it, its `removal_path` may be the very alias that was just taken over. Two
    /// entries then carry the same removal path, `expired` reports it twice, and `remove_after`
    /// unlinks the replacement on the retained file's behalf. The survivor must be repointed at an
    /// alias that still resolves to it.
    #[test]
    fn a_retained_identity_does_not_share_a_removal_path_with_its_replacement() {
        let mut known = KnownSmallFiles::default();
        let retained = PathBuf::from("/logs/canonical.log");
        let first_alias = PathBuf::from("/logs/a.log");
        let second_alias = PathBuf::from("/logs/b.log");
        let first_seen = Instant::now() - Duration::from_secs(60);

        // One short file reachable through two configured spellings; `a.log` recorded it.
        known.insert(retained.clone(), &first_alias, first_seen, None);
        known.insert(retained.clone(), &second_alias, first_seen, None);

        // A different short file now occupies `a.log`. The original survives through `b.log`.
        let replacement = PathBuf::from("/logs/replacement.log");
        known.insert(replacement.clone(), &first_alias, first_seen, None);

        assert!(known.contains_identity(&retained));
        assert_eq!(
            known.removal_path(&retained),
            Some(second_alias.as_path()),
            "the retained file must be removable by an alias that still reaches it, not by the \
             path its replacement now owns"
        );

        let mut expired = known.expired(Duration::from_secs(30));
        expired.sort();
        assert_eq!(
            expired,
            vec![(retained, second_alias.clone()), (replacement, first_alias),],
            "each entry must expire under its own path; a shared one unlinks the replacement twice"
        );
    }

    /// Cleanup that hits `NotFound` must stop choosing that spelling, or it retries the missing path
    /// on every pass and never tries the live one.
    #[test]
    fn a_missing_spelling_is_dropped_so_a_live_one_is_chosen() {
        let mut known = KnownSmallFiles::default();
        let identity = PathBuf::from("/logs/canonical.log");
        let renamed_away = PathBuf::from("/logs/a.log");
        let live = PathBuf::from("/logs/b.log");
        let first_seen = Instant::now() - Duration::from_secs(60);

        known.insert(identity.clone(), &renamed_away, first_seen, None);
        known.insert(identity.clone(), &live, first_seen, None);
        assert_eq!(
            known.removal_path(&identity),
            Some(renamed_away.as_path()),
            "setup requires the lexically first spelling to be the one that is gone"
        );

        known.forget_missing_spelling(&identity, &renamed_away);

        assert_eq!(
            known.removal_path(&identity),
            Some(live.as_path()),
            "the live spelling must be chosen once the missing one is dropped"
        );
        assert_eq!(
            known.expired(Duration::from_secs(30)),
            vec![(identity, live)],
            "and expiry must offer it under that spelling"
        );
    }

    #[test]
    fn removing_by_a_lexical_key_still_drops_the_canonical_entry() {
        // When `canonicalize` fails (a dangling symlink) the caller's key is the lexical spelling, not
        // the canonical identity the entry is filed under.
        let mut known = KnownSmallFiles::default();
        let canonical = PathBuf::from("/var/target.log");
        let alias = PathBuf::from("/logs/link.log");
        known.insert(canonical.clone(), &alias, Instant::now(), None);

        // The target is gone, so the caller arrives with the alias as both key and path.
        known.remove(&alias, &alias);

        assert!(known.is_empty(), "no entry may survive: {known:?}");
        assert!(
            !known.contains_identity(&canonical),
            "the canonical entry must go, or it is tracked forever"
        );

        // The same file seen under *both* spellings, then removed by the lexical key alone.
        let mut known = KnownSmallFiles::default();
        known.insert(canonical.clone(), &alias, Instant::now(), None);
        known.insert(canonical.clone(), &canonical, Instant::now(), None);
        assert_eq!(known.len(), 1, "both spellings are one entry");

        known.remove(&alias, &alias);
        assert!(
            known.is_empty(),
            "removing by the alias must not leave a canonical-only entry, which `expired` would \
             never return: {known:?}"
        );
    }

    #[test]
    fn a_file_left_with_only_its_canonical_spelling_is_not_removable() {
        let mut known = KnownSmallFiles::default();
        let target = PathBuf::from("/var/shared/target.log");
        let alias = PathBuf::from("/logs/app.log");
        let first_seen = Instant::now() - Duration::from_secs(60);

        // Seen through the configured alias, and also named directly by a notify event.
        known.insert(target.clone(), &alias, first_seen, None);
        known.insert(target.clone(), &target, first_seen, None);

        // The alias is retargeted at a different short file, so only the canonical spelling remains.
        known.insert(
            PathBuf::from("/var/other.log"),
            &alias,
            Instant::now(),
            None,
        );

        assert_eq!(
            known.removal_path(&target),
            None,
            "with no configured spelling left, remove_after must not fall back to the canonical \
             target: it can live outside the include and be shared"
        );
        assert!(
            !known
                .expired(Duration::from_secs(30))
                .iter()
                .any(|(identity, _)| identity == &target),
            "such a file must not be offered for expiry at all"
        );
    }

    /// Removing an identity must clear every spelling it was recorded under, not just the one passed
    /// in: a later file appearing at a leftover alias looked like it displaced a tracked entry.
    #[test]
    fn removing_an_identity_clears_every_alias() {
        let mut known = KnownSmallFiles::default();
        let identity = PathBuf::from("/logs/canonical.log");
        let first_alias = PathBuf::from("/logs/a.log");
        let second_alias = PathBuf::from("/logs/b.log");
        let first_seen = Instant::now();

        known.insert(identity.clone(), &first_alias, first_seen, None);
        known.insert(identity.clone(), &second_alias, first_seen, None);

        // A third spelling: the old two-structure version cleared only the alias passed in and the
        // one the entry stored, so a third was left dangling.
        let third_alias = PathBuf::from("/logs/c.log");
        known.insert(identity.clone(), &third_alias, first_seen, None);

        // Removed through a spelling that is not the one `removal_path()` would pick.
        known.remove(&identity, &second_alias);
        assert!(known.is_empty(), "no entry may survive: {known:?}");

        // No alias may still resolve to the removed identity.
        let other = PathBuf::from("/logs/other.log");
        assert!(
            known.insert(other.clone(), &first_alias, Instant::now(), None),
            "a new file at a cleared alias must be recorded as new"
        );
        assert_eq!(
            known.removal_path(&other),
            Some(first_alias.as_path()),
            "the new file keeps its own removal path"
        );
        assert_eq!(
            known.len(),
            1,
            "a dangling alias must not resurrect anything"
        );
    }

    /// `owner_by_path` is a derived lookup, so it must never outlive what `by_identity` holds. This
    /// walks a sequence of takeovers and removals and checks the two agree afterwards -- a leak here
    /// is what made the previous design produce four separate review findings.
    #[test]
    fn the_path_index_stays_consistent_with_the_entries() {
        let mut known = KnownSmallFiles::default();
        let first = PathBuf::from("/logs/one.log");
        let second = PathBuf::from("/logs/two.log");
        let third = PathBuf::from("/logs/three.log");
        let now = Instant::now();

        // One file under three spellings, removed through just one of them: the other two must not
        // be left in the index. Removing an entry that still holds several spellings is what the
        // previous design got wrong.
        known.insert(first.clone(), &first, now, None);
        known.insert(first.clone(), &second, now, None);
        known.insert(first.clone(), &third, now, None);
        known.remove(&first, &second);
        assert!(known.is_empty(), "the entry itself must be gone");

        // Then a fresh takeover sequence, so the checks below see both code paths.
        known.insert(second.clone(), &second, now, None);
        known.insert(second.clone(), &third, now, None);
        known.insert(third.clone(), &third, now, None);

        for (identity, entry) in &known.by_identity {
            for path in &entry.removal_paths {
                assert_eq!(
                    known.owner_by_path.get(path),
                    Some(identity),
                    "every spelling an entry holds must be indexed to that entry"
                );
            }
        }
        for (path, identity) in &known.owner_by_path {
            let entry = known
                .by_identity
                .get(identity)
                .unwrap_or_else(|| panic!("{path:?} is indexed to absent identity {identity:?}"));
            assert!(
                entry.removal_paths.contains(path),
                "{path:?} is indexed to {identity:?}, which does not hold it"
            );
        }
    }

    /// `remove` is called when a file fingerprints or turns out to be gone. Removing it by one alias
    /// must not strand the reverse index: the other alias for the same identity has to go too, or a
    /// later file appearing at that alias would look like it displaced a still-tracked entry.
    #[test]
    fn removing_by_one_alias_clears_the_other() {
        let mut known = KnownSmallFiles::default();
        let identity = PathBuf::from("/logs/canonical.log");
        let first_alias = PathBuf::from("/logs/a.log");
        let second_alias = PathBuf::from("/logs/b.log");
        let first_seen = Instant::now();

        known.insert(identity.clone(), &first_alias, first_seen, None);
        known.insert(identity.clone(), &second_alias, first_seen, None);

        // The file completed a line, so it is no longer short, observed via the first alias.
        known.remove(&identity, &first_alias);
        assert!(
            !known.contains_identity(&identity),
            "a fingerprinted file must not stay tracked"
        );
        assert!(known.is_empty(), "no entry may survive: {known:?}");

        // A brand-new short file at the other alias must be recorded as new, not treated as a
        // repeat sighting of the entry that was just removed.
        let other = PathBuf::from("/logs/other.log");
        assert!(
            known.insert(other.clone(), &second_alias, Instant::now(), None),
            "a new file at a cleared alias must be newly recorded"
        );
        assert!(known.contains_identity(&other));
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ReadFrom {
    #[default]
    Beginning,
    End,
    Checkpoint(FilePosition),
}

/// File position to use when reading a new file.
#[configurable_component]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadFromConfig {
    /// Read from the beginning of the file.
    Beginning,

    /// Start reading from the current end of the file.
    End,
}

impl From<ReadFromConfig> for ReadFrom {
    fn from(rfc: ReadFromConfig) -> Self {
        match rfc {
            ReadFromConfig::Beginning => ReadFrom::Beginning,
            ReadFromConfig::End => ReadFrom::End,
        }
    }
}
