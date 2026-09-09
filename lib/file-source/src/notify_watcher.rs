//! An OS-level, event-driven alternative/augmentation to the periodic glob-rescan discovery
//! mechanism in [`crate::file_server::FileServer`].
//!
//! # Design
//!
//! [`FileServer`] traditionally re-globs its `include` patterns on a fixed interval
//! (`glob_minimum_cooldown_ms`, historically defaulting to tens of milliseconds) in order to:
//!   1. discover new files,
//!   2. detect renames (a known fingerprint appearing at a new path),
//!   3. wake up reads for files that have new data.
//!
//! On systems with a large number of matched files (see
//! <https://github.com/vectordotdev/vector/issues/3567>), this is expensive: every rescan
//! opens/fingerprints every matched file, and every matched file keeps an open handle for its
//! entire lifetime on disk, even files excluded from reading by `ignore_older`.
//!
//! This module instead watches the *parent directories* of the configured `include` globs using
//! the cross-platform [`notify`] crate (inotify on Linux, FSEvents on macOS,
//! `ReadDirectoryChangesW` on Windows) and turns OS-level create/modify/rename/remove
//! notifications into a stream of [`NotifyMessage`]s that [`FileServer::run`] selects on,
//! alongside a much-less-frequent periodic reconciliation pass (a full glob+fingerprint pass,
//! functionally identical to the old fixed-interval rescan) that exists purely as a correctness
//! backstop: OS-level notification queues can silently overflow under heavy event bursts, and
//! there is an inherent TOCTOU gap between an initial directory scan and when the watch on that
//! directory is actually established.
//!
//! # Directory selection
//!
//! `notify` watches directories (optionally recursively), not glob patterns. For each `include`
//! pattern we compute the longest literal (non-glob) path prefix and watch that directory. If any
//! glob metacharacter appears after that prefix in a path component *below* another path
//! component (i.e. the pattern can match files nested arbitrarily deep, such as with `**`), we
//! watch recursively; otherwise (e.g. a single trailing `*.log` segment) we watch
//! non-recursively. This mirrors, approximately, how far the glob can "reach" beneath the
//! literal prefix.
//!
//! # Bridging into async/tokio
//!
//! `notify`'s watcher delivers events via a synchronous callback, invoked on a thread owned by
//! the OS backend (this is the same shape used elsewhere in this workspace for config file
//! watching, see `src/config/watcher.rs`). We bridge this into the async world with a
//! `tokio::sync::mpsc::UnboundedSender`, doing a blocking (but very cheap, non-blocking-in-practice)
//! send from the notify callback.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use file_source_common::internal_events::FileSourceInternalEvents;
use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcherTrait,
    event::{CreateKind, ModifyKind, RemoveKind, RenameMode},
};
use tokio::sync::mpsc;
use tracing::{debug, trace, warn};

/// A message delivered from the OS-level file watcher to [`FileServer`](crate::file_server::FileServer).
#[derive(Debug)]
pub enum NotifyMessage {
    /// One or more paths were created, modified, or renamed. This is intentionally coarse: we
    /// don't try to fully interpret notify's (platform-dependent, sometimes ambiguous) event
    /// semantics. Instead we treat any of these as "something changed near this path; go check
    /// it," and let the existing fingerprinting/read logic in `FileServer` figure out the rest.
    /// This is deliberately conservative -- it trades a few spurious wakeups (cheap: a stat +
    /// maybe a fingerprint read) for never having to trust notify's event *kind* classification,
    /// which varies across inotify/FSEvents/ReadDirectoryChangesW.
    PathsChanged(Vec<PathBuf>),
    /// One or more paths were removed. Handled the same way as `PathsChanged` today (the
    /// reconciliation logic in `FileServer` determines liveness by whether the path still globs,
    /// not by trusting the removal event alone), but kept distinct so `FileServer` and telemetry
    /// can reason about it explicitly in the future.
    PathsRemoved(Vec<PathBuf>),
    /// The OS-level event queue overflowed: some events were dropped. The caller MUST trigger a
    /// full reconciliation pass in response; this is not optional.
    Overflow,
    /// The watcher backend itself hit an unrecoverable-for-this-event error (e.g. it lost a
    /// watch because a directory was removed out from under it). The caller should keep relying
    /// on periodic reconciliation; recovery (re-establishing the watch) happens on the next
    /// reconciliation-triggered call to [`NotifyDiscovery::resync_watches`].
    BackendError(String),
}

/// Owns the live `notify` watcher and the receiving end of the bridge channel.
///
/// Dropping this stops the watcher thread (via `notify`'s own `Drop` impl on the underlying
/// watcher) and closes the channel.
pub struct NotifyDiscovery {
    watcher: RecommendedWatcher,
    watched_dirs: WantedDirs,
    receiver: mpsc::UnboundedReceiver<NotifyMessage>,
}

impl NotifyDiscovery {
    /// Create a new [`NotifyDiscovery`], watching the directories implied by `include_patterns`.
    ///
    /// Returns `Err` if the underlying OS watcher could not be constructed at all (e.g. platform
    /// resource exhaustion, like hitting the inotify instance limit). Callers should treat this
    /// as "notify-based discovery is unavailable" and fall back to relying solely on the
    /// periodic reconciliation pass -- they should NOT treat it as fatal to the file source as a
    /// whole.
    pub fn new<E: FileSourceInternalEvents>(
        include_patterns: &[PathBuf],
        emitter: &E,
    ) -> notify::Result<Self> {
        let (tx, receiver) = mpsc::unbounded_channel();

        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                // This closure runs on a thread owned by the OS notification backend (e.g. the
                // inotify reader thread). It must not block meaningfully; an unbounded channel
                // send is effectively non-blocking (it only allocates).
                let msg = match res {
                    Ok(event) => classify_event(event),
                    Err(error) => {
                        if is_overflow(&error) {
                            Some(NotifyMessage::Overflow)
                        } else {
                            Some(NotifyMessage::BackendError(error.to_string()))
                        }
                    }
                };
                if let Some(msg) = msg {
                    // The only way this fails is if every receiver has been dropped, i.e.
                    // FileServer has shut down or was never polling; either way, there's
                    // nothing useful to do with the error.
                    drop(tx.send(msg));
                }
            },
            Config::default(),
        )?;

        let mut discovery = Self {
            watcher,
            watched_dirs: WantedDirs::new(),
            receiver,
        };
        discovery.resync_watches(include_patterns, emitter);
        Ok(discovery)
    }

    /// Recompute the set of directories that should be watched from `include_patterns`, and
    /// add/remove watches to match. Cheap to call repeatedly (e.g. from the periodic
    /// reconciliation pass), since it diffs against the currently-watched set rather than
    /// tearing everything down.
    pub fn resync_watches<E: FileSourceInternalEvents>(
        &mut self,
        include_patterns: &[PathBuf],
        emitter: &E,
    ) {
        let wanted = compute_watch_directories(include_patterns);

        // Directories we're not already watching under the mode we now want. This also catches
        // a directory that's currently watched `NonRecursive` but now needs `Recursive` (a
        // second, overlapping `include` pattern started requiring it): re-`watch`-ing with a
        // different mode replaces the previous registration in `notify`, it doesn't stack, so
        // there's no need to `unwatch` first.
        for (path, mode) in &wanted {
            if self.watched_dirs.get(path) == Some(mode) {
                continue;
            }
            match self.watcher.watch(path, mode.mode()) {
                Ok(()) => {
                    trace!(message = "Watching directory for file events.", path = ?path, ?mode);
                    // Only record success: if `watch` failed, leaving this path out of
                    // `watched_dirs` means the next `resync_watches` call (from the backstop
                    // reconciliation pass) will see it as still "wanted but not yet watched" and
                    // retry, rather than wrongly concluding the watch is already in place and
                    // never trying again.
                    self.watched_dirs.insert(path.clone(), *mode);
                }
                Err(error) => {
                    warn!(message = "Failed to watch directory.", path = ?path, %error);
                    emitter
                        .emit_file_watch_backend_error(&std::io::Error::other(error.to_string()));
                }
            }
        }
        self.watched_dirs.retain(|path, _mode| {
            if wanted.contains_key(path) {
                return true;
            }
            // Best-effort: if the directory is already gone, unwatch will simply error, which we
            // can ignore -- there's nothing left to watch.
            drop(self.watcher.unwatch(path));
            false
        });

        emitter.emit_file_watch_directories(self.watched_dirs.len());
    }

    /// Await the next batch of filesystem events.
    pub async fn recv(&mut self) -> Option<NotifyMessage> {
        self.receiver.recv().await
    }

    /// Forget which directories we believe are currently watched, without touching the
    /// underlying OS-level watcher. The next `resync_watches` call will then treat every
    /// directory implied by `include_patterns` as unwatched and re-`watch` it.
    ///
    /// Call this after a [`NotifyMessage::BackendError`], which signals that the watcher backend
    /// itself hit a problem (e.g. it silently dropped a watch because the directory it was
    /// watching was removed and recreated, or some other backend-specific hiccup). Without this,
    /// `resync_watches`'s "only `watch()` a path if we don't already believe it's watched" check
    /// (necessary so it doesn't uselessly re-`watch` paths on every call) means a watch lost this
    /// way is never re-established: `include_patterns` hasn't changed, so the set of "wanted"
    /// directories is identical to what's already recorded in `watched_dirs`, and the loop skips
    /// every one of them. Re-`watch`-ing a path notify still has registered correctly is a
    /// harmless no-op, so clearing all bookkeeping on any backend error, rather than trying to
    /// determine which specific watch was affected (which notify's error doesn't tell us), is the
    /// simple, safe choice here.
    pub fn forget_watches(&mut self) {
        self.watched_dirs.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum WatchMode {
    Recursive,
    NonRecursive,
}

impl WatchMode {
    fn mode(self) -> RecursiveMode {
        match self {
            WatchMode::Recursive => RecursiveMode::Recursive,
            WatchMode::NonRecursive => RecursiveMode::NonRecursive,
        }
    }

    /// Combine the modes wanted for the same directory by two different (overlapping) `include`
    /// patterns. `Recursive` always wins: it observes a strict superset of what `NonRecursive`
    /// would, so it's the only mode that satisfies both patterns' requirements at once.
    fn merge(self, other: WatchMode) -> WatchMode {
        if self == WatchMode::Recursive || other == WatchMode::Recursive {
            WatchMode::Recursive
        } else {
            WatchMode::NonRecursive
        }
    }
}

/// A directory to `WatchMode` mapping. A `HashMap` keyed on the path alone -- not a set of
/// `(PathBuf, WatchMode)` pairs -- is deliberate: two different `include` patterns can imply the
/// same directory under two different modes (e.g. `/var/log/*.log` wants it `NonRecursive` while
/// `/var/log/**/*.log` wants it `Recursive`), and a directory can only actually be watched one
/// way at a time. Keying on the pair would let both "versions" of the same directory coexist as
/// distinct set members, which doesn't correspond to any real state `notify` can be in.
type WantedDirs = HashMap<PathBuf, WatchMode>;

/// Compute, for a set of glob include patterns, the directories that should be watched (and
/// whether each should be watched recursively) in order to observe every path that could
/// possibly match one of the patterns.
///
/// For each pattern, this walks its path components, stopping at the first component containing
/// a glob metacharacter (`*`, `?`, `[`, `{`). The path made up of the components before that
/// point is the directory to watch. If the pattern contains a `**` component, or has any
/// directory separator after the first glob metacharacter (meaning matches can be nested
/// arbitrarily deep below the watched directory), the watch is recursive; otherwise (a single
/// glob component with no further nesting, e.g. `/var/log/*.log`) a non-recursive watch
/// suffices and is preferred, since it's cheaper (particularly on inotify, where recursive
/// watching means watching every subdirectory individually) and matches the "flat glob"
/// intent.
fn compute_watch_directories(include_patterns: &[PathBuf]) -> WantedDirs {
    let mut result = WantedDirs::new();

    for pattern in include_patterns {
        let mut literal_prefix = PathBuf::new();
        let mut remainder_has_glob = false;
        let mut remainder_has_separator_after_glob = false;
        let mut seen_glob = false;

        for component in pattern.components() {
            let comp_str = component.as_os_str().to_string_lossy();
            let is_glob_component = contains_glob_metachar(&comp_str);

            if !seen_glob && !is_glob_component {
                literal_prefix.push(component.as_os_str());
                continue;
            }

            // Any component from here on -- glob or literal -- appearing after the *first* glob
            // component means matches can be nested at least one level below the watched
            // directory (e.g. `*/*.log` or `*/sub/*.log`), which recursion is needed to observe.
            // Checking only for a literal-after-glob (an earlier version of this) missed the
            // equally common two-glob-components case (`*/*.log`): its second component is itself
            // a glob, not a literal, so it never hit the literal-only branch, silently leaving
            // such patterns NonRecursive.
            if seen_glob {
                remainder_has_separator_after_glob = true;
            }

            seen_glob = true;
            if is_glob_component {
                remainder_has_glob = true;
                // `**` means "any depth of nesting" on its own, regardless of what (if anything)
                // follows it in the pattern -- a standalone trailing `**` (e.g. `/var/log/**`,
                // with nothing after it) needs recursion just as much as `**/*.log` does, but the
                // "something follows the first glob component" check above doesn't fire for it
                // since there's nothing after it to be "something." Check for it explicitly too.
                if comp_str.contains("**") {
                    remainder_has_separator_after_glob = true;
                }
            }
        }

        // If the whole pattern was literal (no glob at all), watch its parent directory
        // non-recursively so we notice the file itself being created/modified/removed.
        if !seen_glob {
            let dir = literal_prefix
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            insert_merging_mode(&mut result, dir, WatchMode::NonRecursive);
            continue;
        }

        let dir = if literal_prefix.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            literal_prefix
        };

        let mode = if remainder_has_glob && remainder_has_separator_after_glob {
            WatchMode::Recursive
        } else {
            WatchMode::NonRecursive
        };

        insert_merging_mode(&mut result, dir, mode);
    }

    result
}

/// Insert `(dir, mode)` into `result`, merging with any existing entry for the same directory
/// (via [`WatchMode::merge`]) rather than overwriting it -- so that two different `include`
/// patterns implying the same directory under different modes correctly end up with the one
/// mode (`Recursive`) that satisfies both, instead of one silently clobbering the other
/// depending on iteration order.
fn insert_merging_mode(result: &mut WantedDirs, dir: PathBuf, mode: WatchMode) {
    result
        .entry(dir)
        .and_modify(|existing| *existing = existing.merge(mode))
        .or_insert(mode);
}

fn contains_glob_metachar(component: &str) -> bool {
    component.contains(['*', '?', '[', '{'])
}

/// Collapse the wide variety of notify [`EventKind`]s we care about into the coarse
/// [`NotifyMessage`] variants `FileServer` acts on. Returns `None` for event kinds we
/// deliberately ignore (e.g. bare `Access` events, which fire far too often to be useful and
/// carry no information our fingerprint-based reconciliation needs).
fn classify_event(event: Event) -> Option<NotifyMessage> {
    match event.kind {
        EventKind::Create(CreateKind::Any | CreateKind::File | CreateKind::Folder) => {
            Some(NotifyMessage::PathsChanged(event.paths))
        }
        EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_) | ModifyKind::Metadata(_)) => {
            Some(NotifyMessage::PathsChanged(event.paths))
        }
        // Rename events: notify (when it can correlate From/To pairs, which is
        // platform-dependent) still gives us the paths involved; treat both ends as "changed"
        // since the safest thing to do is let FileServer's reconciliation logic figure out
        // what's alive at each path via a fingerprint/stat check.
        EventKind::Modify(ModifyKind::Name(
            RenameMode::Any | RenameMode::Both | RenameMode::From | RenameMode::To,
        )) => Some(NotifyMessage::PathsChanged(event.paths)),
        EventKind::Remove(RemoveKind::Any | RemoveKind::File | RemoveKind::Folder) => {
            Some(NotifyMessage::PathsRemoved(event.paths))
        }
        EventKind::Create(CreateKind::Other)
        | EventKind::Modify(ModifyKind::Other | ModifyKind::Name(RenameMode::Other))
        | EventKind::Remove(RemoveKind::Other) => Some(NotifyMessage::PathsChanged(event.paths)),
        EventKind::Any | EventKind::Other => {
            // Deliberately conservative: an event we can't classify might still be relevant
            // (some backends report generic "Any" for things we care about), so treat it as a
            // change rather than silently dropping it. Access events are the only kind we
            // intentionally ignore below.
            if event.paths.is_empty() {
                None
            } else {
                Some(NotifyMessage::PathsChanged(event.paths))
            }
        }
        EventKind::Access(_) => {
            debug!(message = "Ignoring filesystem access event.", paths = ?event.paths);
            None
        }
    }
}

fn is_overflow(error: &notify::Error) -> bool {
    matches!(error.kind, notify::ErrorKind::MaxFilesWatch)
        || error.to_string().to_lowercase().contains("overflow")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_pattern_watches_parent_non_recursive() {
        let dirs = compute_watch_directories(&[PathBuf::from("/var/log/vector.log")]);
        assert_eq!(dirs.len(), 1);
        let (path, mode) = dirs.into_iter().next().unwrap();
        assert_eq!(path, PathBuf::from("/var/log"));
        assert!(matches!(mode, WatchMode::NonRecursive));
    }

    #[test]
    fn single_star_watches_dir_non_recursive() {
        let dirs = compute_watch_directories(&[PathBuf::from("/var/log/*.log")]);
        assert_eq!(dirs.len(), 1);
        let (path, mode) = dirs.into_iter().next().unwrap();
        assert_eq!(path, PathBuf::from("/var/log"));
        assert!(matches!(mode, WatchMode::NonRecursive));
    }

    #[test]
    fn double_star_watches_recursively() {
        let dirs = compute_watch_directories(&[PathBuf::from("/var/log/**/*.log")]);
        assert_eq!(dirs.len(), 1);
        let (path, mode) = dirs.into_iter().next().unwrap();
        assert_eq!(path, PathBuf::from("/var/log"));
        assert!(matches!(mode, WatchMode::Recursive));
    }

    #[test]
    fn standalone_trailing_double_star_watches_recursively() {
        // Regression test for a bug found in review: a `**` with nothing after it in the pattern
        // (as opposed to `**/*.log`, covered above) means "any depth of nesting" on its own, so
        // it needs a recursive watch just as much as the with-a-suffix case does. The
        // "something follows the first glob component" check that handles `*/*.log` and
        // `*/sub/*.log` doesn't fire here, since there's nothing after the lone `**` component to
        // be "something" -- this pattern needs the explicit "this component itself contains '**'"
        // check to be recognized as recursive.
        let dirs = compute_watch_directories(&[PathBuf::from("/var/log/**")]);
        assert_eq!(dirs.len(), 1);
        let (path, mode) = dirs.into_iter().next().unwrap();
        assert_eq!(path, PathBuf::from("/var/log"));
        assert!(matches!(mode, WatchMode::Recursive));
    }

    #[test]
    fn nested_literal_after_glob_watches_recursively() {
        let dirs = compute_watch_directories(&[PathBuf::from("/var/log/*/app.log")]);
        assert_eq!(dirs.len(), 1);
        let (path, mode) = dirs.into_iter().next().unwrap();
        assert_eq!(path, PathBuf::from("/var/log"));
        assert!(matches!(mode, WatchMode::Recursive));
    }

    #[test]
    fn nested_glob_after_glob_watches_recursively() {
        // Two glob components in a row (`*/*.log`), as opposed to a literal component after a
        // glob (`*/app.log`, covered above) or a `**` component. Both the directory-matching `*`
        // and the file-matching `*.log` are themselves globs, so neither the old "does this
        // component contain '**'" check nor the old "is this a literal component after a glob"
        // check caught this pattern -- it was silently left NonRecursive, meaning writes to files
        // in subdirectories that already existed when the watch was established would never be
        // noticed by the notify event path (only by the much-less-frequent reconcile-interval
        // backstop).
        let dirs = compute_watch_directories(&[PathBuf::from("/var/log/*/*.log")]);
        assert_eq!(dirs.len(), 1);
        let (path, mode) = dirs.into_iter().next().unwrap();
        assert_eq!(path, PathBuf::from("/var/log"));
        assert!(matches!(mode, WatchMode::Recursive));
    }

    #[test]
    fn multiple_patterns_produce_multiple_dirs() {
        let dirs = compute_watch_directories(&[
            PathBuf::from("/var/log/*.log"),
            PathBuf::from("/opt/app/logs/*.log"),
        ]);
        assert_eq!(dirs.len(), 2);
    }

    #[test]
    fn overlapping_patterns_for_same_dir_merge_to_one_recursive_entry() {
        // `*.log` alone would only need `/var/log` watched non-recursively, but the second,
        // overlapping pattern needs it recursive (nested subdirectories can match too). The two
        // patterns must resolve to exactly one entry for `/var/log`, watched `Recursive` (which
        // covers what `NonRecursive` would have caught too) -- not two separate entries for the
        // same directory under different modes, which isn't a state `notify` can actually be in
        // (a directory is watched one way or the other, never both at once).
        let dirs = compute_watch_directories(&[
            PathBuf::from("/var/log/*.log"),
            PathBuf::from("/var/log/**/*.log"),
        ]);
        assert_eq!(
            dirs.len(),
            1,
            "overlapping patterns for the same directory must merge into a single entry, \
             not coexist as separate (path, mode) pairs"
        );
        assert_eq!(
            dirs.get(&PathBuf::from("/var/log")),
            Some(&WatchMode::Recursive)
        );
    }

    #[test]
    fn watch_mode_merge_prefers_recursive() {
        assert_eq!(
            WatchMode::NonRecursive.merge(WatchMode::Recursive),
            WatchMode::Recursive
        );
        assert_eq!(
            WatchMode::Recursive.merge(WatchMode::NonRecursive),
            WatchMode::Recursive
        );
        assert_eq!(
            WatchMode::NonRecursive.merge(WatchMode::NonRecursive),
            WatchMode::NonRecursive
        );
        assert_eq!(
            WatchMode::Recursive.merge(WatchMode::Recursive),
            WatchMode::Recursive
        );
    }

    #[derive(Clone)]
    struct NoopEmitter;

    impl FileSourceInternalEvents for NoopEmitter {
        fn emit_file_added(&self, _path: &std::path::Path) {}
        fn emit_file_resumed(&self, _path: &std::path::Path, _file_position: u64) {}
        fn emit_file_watch_error(&self, _path: &std::path::Path, _error: std::io::Error) {}
        fn emit_file_unwatched(&self, _path: &std::path::Path, _reached_eof: bool) {}
        fn emit_file_deleted(&self, _path: &std::path::Path) {}
        fn emit_file_delete_error(&self, _path: &std::path::Path, _error: std::io::Error) {}
        fn emit_file_fingerprint_read_error(
            &self,
            _path: &std::path::Path,
            _error: std::io::Error,
        ) {
        }
        fn emit_file_checkpointed(&self, _count: usize, _duration: std::time::Duration) {}
        fn emit_file_checksum_failed(&self, _path: &std::path::Path) {}
        fn emit_file_checkpoint_write_error(&self, _error: std::io::Error) {}
        fn emit_files_open(&self, _count: usize) {}
        fn emit_files_idle(&self, _count: usize) {}
        fn emit_path_globbing_failed(&self, _path: &std::path::Path, _error: &std::io::Error) {}
        fn emit_file_line_too_long(&self, _buf: &bytes::BytesMut, _max_size: usize, _size: usize) {}
    }

    #[test]
    fn forget_watches_makes_resync_re_watch_everything() {
        // Regression test for a bug found in review: on `NotifyMessage::BackendError` (the
        // watcher backend silently dropping a watch, e.g. because a watched directory was
        // removed and recreated out from under it), `FileServer` used to just trigger a
        // reconciliation pass without clearing `NotifyDiscovery`'s own bookkeeping first.
        // `resync_watches` only calls `watch()` on a directory it doesn't already believe is
        // watched -- necessary so it doesn't uselessly re-`watch` every directory on every call
        // -- so with `include_patterns` unchanged, the "wanted" set is identical to what's
        // already recorded, and the loop would skip re-`watch`-ing the directory that actually
        // lost its watch. The watch would then never be re-established until Vector restarted.
        //
        // This can't easily be tested by actually breaking notify's underlying watch (that's
        // backend- and OS-specific, and not something the `notify` crate exposes a way to
        // simulate), but the property that matters is `NotifyDiscovery`-internal and doesn't
        // require one: `forget_watches` must leave `resync_watches` believing every directory is
        // unwatched, so it re-`watch`es all of them. Re-`watch`-ing a path the backend actually
        // still has registered correctly is a harmless no-op, so this is the right (and only
        // practical) recovery strategy regardless of which specific watch was actually lost.
        let dir = tempfile::tempdir().unwrap();
        let pattern = dir.path().join("*.log");
        let mut discovery =
            NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter).unwrap();

        assert_eq!(
            discovery.watched_dirs.len(),
            1,
            "resync_watches (called from `new`) should have recorded the one watched directory"
        );

        discovery.forget_watches();
        assert!(
            discovery.watched_dirs.is_empty(),
            "forget_watches must clear the watched-directories bookkeeping"
        );

        // With bookkeeping cleared but `include_patterns` unchanged, resync_watches must
        // re-`watch` (not skip) the directory, ending up back where it started.
        discovery.resync_watches(&[pattern], &NoopEmitter);
        assert_eq!(
            discovery.watched_dirs.len(),
            1,
            "resync_watches must re-establish the watch after bookkeeping was forgotten"
        );
        assert_eq!(
            discovery.watched_dirs.get(dir.path()),
            Some(&WatchMode::NonRecursive)
        );
    }
}
