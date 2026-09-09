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
//! If that literal prefix doesn't exist on disk yet (e.g. `/var/log/newapp/*.log` before
//! `newapp` has been created), it can't be `watch()`-ed directly; [`NotifyDiscovery`] instead
//! watches the nearest existing ancestor recursively as a stand-in, so the prefix directory's
//! eventual creation is still observed promptly. Once it exists, the next `resync_watches` call
//! upgrades to watching it directly and drops the broader ancestor watch.
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
    /// For a wanted directory that doesn't exist yet (so it can't be `watch()`-ed directly),
    /// tracks the nearest existing ancestor we're watching recursively instead, keyed by the
    /// *wanted* directory. `resync_watches` uses this to notice once the wanted directory has
    /// been created and upgrade to watching it directly (dropping the broader, more expensive
    /// ancestor watch) rather than watching the ancestor forever. See `resync_watches` for
    /// details.
    fallback_watches: HashMap<PathBuf, PathBuf>,
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
            fallback_watches: HashMap::new(),
            receiver,
        };
        discovery.resync_watches(include_patterns, emitter);
        Ok(discovery)
    }

    /// Recompute the set of directories that should be watched from `include_patterns`, and
    /// add/remove watches to match. Cheap to call repeatedly (e.g. from the periodic
    /// reconciliation pass), since it diffs against the currently-watched set rather than
    /// tearing everything down.
    ///
    /// If a wanted directory doesn't exist yet (e.g. an `include` pattern like
    /// `/var/log/newapp/*.log` where `newapp` hasn't been created yet), `watch()`-ing it directly
    /// fails; this falls back to recursively watching the nearest existing ancestor instead, so
    /// that creating the wanted directory (and anything under it) is still noticed promptly
    /// rather than only on the next `reconcile_interval` backstop. Once the wanted directory
    /// exists, a later call upgrades to watching it directly and drops the broader ancestor watch
    /// (unless some other wanted directory still needs that same ancestor as its own fallback).
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
            // If some other not-yet-existing wanted directory already depends on `path` as its
            // fallback ancestor (necessarily `Recursive`: see `watch_fallback_ancestor`), that
            // requirement must be merged in here too. Without this, a directory that is *both* a
            // directly-wanted `NonRecursive` directory *and* someone else's fallback ancestor
            // could have its watch silently downgraded to `NonRecursive` below -- depending on
            // this `HashMap`'s unspecified iteration order, `path` may be processed only after
            // `watch_fallback_ancestor` already installed the `Recursive` watch it needs, and
            // `self.watched_dirs.get(path) == Some(mode)` (comparing directly against the plain
            // `NonRecursive` `wanted` for this path) would then be `false`, causing a re-`watch`
            // that replaces the existing `Recursive` registration with a weaker `NonRecursive`
            // one. That leaves creation of files nested under `path` unnoticed until the next
            // `reconcile_interval` backstop, defeating the very purpose of the fallback watch.
            let mode = if self
                .fallback_watches
                .values()
                .any(|fallback_ancestor| fallback_ancestor == path)
            {
                mode.merge(WatchMode::Recursive)
            } else {
                *mode
            };
            if self.watched_dirs.get(path) == Some(&mode) {
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
                    self.watched_dirs.insert(path.clone(), mode);
                    // The real directory is now watched directly; drop any record of it having
                    // depended on a fallback ancestor. The ancestor's own watch, if now unused,
                    // is cleaned up below.
                    self.fallback_watches.remove(path);
                }
                Err(error) => {
                    // Always try the ancestor fallback on any `watch` failure, rather than first
                    // branching on `path.is_dir()` to decide whether the directory "doesn't exist
                    // yet" (fallback) versus "exists but couldn't be watched" (no fallback, e.g. a
                    // permissions problem or platform resource limit): checking `is_dir()` only
                    // *after* `watch` has already failed is a TOCTOU race -- if the directory is
                    // created in the gap between the two calls, `is_dir()` now reports `true` for
                    // what was, at `watch`-time, a missing directory, wrongly skipping the fallback
                    // that would otherwise have watched its (now-populated) parent. Attempting the
                    // fallback unconditionally is safe either way: if the directory does exist and
                    // the failure is permanent (permissions, resource limits), watching its parent
                    // recursively still lets us notice changes to it (a recursive watch on a
                    // directory observes events inside its children too, so this isn't a no-op),
                    // it's simply broader/more expensive than directly watching `path`. That's a
                    // strictly better outcome than reporting the error and doing nothing further
                    // until the next `reconcile_interval` backstop.
                    match find_existing_ancestor(path) {
                        Some(ancestor) => self.watch_fallback_ancestor(path, ancestor, emitter),
                        None => {
                            warn!(message = "Failed to watch directory.", path = ?path, %error);
                            emitter.emit_file_watch_backend_error(&std::io::Error::other(
                                error.to_string(),
                            ));
                        }
                    }
                }
            }
        }

        self.fallback_watches
            .retain(|path, _ancestor| wanted.contains_key(path));
        // A directory stays watched if it's directly wanted, or if some still-wanted directory
        // depends on it as its fallback ancestor; anything else (no longer wanted, or a fallback
        // ancestor whose dependent either got its own direct watch or was dropped above) is
        // unwatched and forgotten.
        let ancestors_in_use: std::collections::HashSet<&PathBuf> =
            self.fallback_watches.values().collect();
        self.watched_dirs.retain(|path, _mode| {
            if wanted.contains_key(path) || ancestors_in_use.contains(path) {
                return true;
            }
            // Best-effort: if the directory is already gone, unwatch will simply error, which we
            // can ignore -- there's nothing left to watch.
            drop(self.watcher.unwatch(path));
            false
        });

        emitter.emit_file_watch_directories(self.watched_dirs.len());
    }

    /// Watch `ancestor` (recursively, so creation of `wanted` underneath it is observed) as a
    /// stand-in for the not-yet-existing `wanted` directory, recording the substitution in
    /// `fallback_watches` so a later `resync_watches` call can detect once `wanted` exists and
    /// upgrade to watching it directly.
    fn watch_fallback_ancestor<E: FileSourceInternalEvents>(
        &mut self,
        wanted: &Path,
        ancestor: PathBuf,
        emitter: &E,
    ) {
        if self.watched_dirs.get(&ancestor) == Some(&WatchMode::Recursive) {
            // Some other wanted directory already caused us to watch this same ancestor
            // recursively; nothing more to do beyond recording that this wanted directory now
            // also depends on it.
            self.fallback_watches.insert(wanted.to_path_buf(), ancestor);
            return;
        }
        match self.watcher.watch(&ancestor, RecursiveMode::Recursive) {
            Ok(()) => {
                debug!(
                    message = "Configured directory does not exist yet; watching nearest existing ancestor instead.",
                    wanted = ?wanted,
                    ancestor = ?ancestor,
                );
                self.watched_dirs
                    .insert(ancestor.clone(), WatchMode::Recursive);
                self.fallback_watches.insert(wanted.to_path_buf(), ancestor);
            }
            Err(error) => {
                warn!(message = "Failed to watch directory.", path = ?ancestor, %error);
                emitter.emit_file_watch_backend_error(&std::io::Error::other(error.to_string()));
            }
        }
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

    /// Forget bookkeeping for a single watched directory, without touching the underlying OS-level
    /// watcher, so the next `resync_watches` call re-`watch`es it if it's still (or again) wanted.
    ///
    /// Call this when a [`NotifyMessage::PathsRemoved`] reports the removal of a path that is
    /// itself one of our watched directories (as opposed to a file inside one). On Linux/inotify,
    /// removing a watched directory invalidates the kernel-side watch on that inode; if the
    /// directory is later recreated (e.g. an application that removes and recreates its log
    /// directory, or `logrotate`-style directory rotation), `notify` has no watch left to fire
    /// events from, but `resync_watches`'s "only `watch()` a path we don't already believe is
    /// watched" check still sees this directory in `watched_dirs` (removal doesn't change
    /// `include_patterns`, so the "wanted" set is unchanged) and skips re-`watch`-ing it forever.
    /// Without this, such a directory falls back to being noticed only by the much-less-frequent
    /// backstop reconciliation, same as a lost `BackendError`-reported watch.
    pub fn forget_watch(&mut self, path: &Path) {
        self.watched_dirs.remove(path);
    }

    /// Whether `path` is currently believed to be a watched directory (as opposed to, say, a file
    /// inside one). Used to decide whether a [`NotifyMessage::PathsRemoved`] path warrants
    /// `forget_watch`.
    pub fn is_watched_dir(&self, path: &Path) -> bool {
        self.watched_dirs.contains_key(path)
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

/// Walk up from `path` to find the nearest ancestor directory that currently exists on disk.
/// Returns `None` only if no ancestor exists at all (e.g. even the filesystem root couldn't be
/// stat-ed, which in practice shouldn't happen).
///
/// For a relative `path` with only one component (e.g. `logs` from an `include` pattern like
/// `logs/*.log`), `Path::ancestors()` yields that component and then an empty path (`""`) --
/// there is no further parent to walk up to for a relative path. `Path::is_dir()` on `""` is
/// always `false` regardless of the actual current directory (unlike `"."`, which `is_dir()`
/// correctly reports as the current directory), so without special-casing it, a relative
/// top-level root that doesn't exist yet would find no existing ancestor at all -- silently
/// forgoing the fallback-ancestor watch and leaving that `include` pattern's eventual root
/// creation unnoticed until the next `reconcile_interval` backstop. Treat the empty ancestor as
/// `.` (the current directory), which is what it actually denotes.
fn find_existing_ancestor(path: &Path) -> Option<PathBuf> {
    path.ancestors().skip(1).find_map(|ancestor| {
        let ancestor = if ancestor.as_os_str().is_empty() {
            Path::new(".")
        } else {
            ancestor
        };
        ancestor.is_dir().then(|| ancestor.to_path_buf())
    })
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

    #[test]
    fn forget_watch_makes_resync_re_watch_one_directory() {
        // Regression test for a bug found in review: removing a watched *directory* on
        // Linux/inotify invalidates the watch on that inode, but a `PathsRemoved` notification for
        // it used to be handled the same as any other path event (just triggering a reconciliation
        // pass), leaving the directory recorded in `watched_dirs`. `resync_watches`'s "only
        // `watch()` a directory we don't already believe is watched" check would then skip
        // re-`watch`-ing it even after it was recreated, permanently falling back to the
        // much-less-frequent backstop reconciliation for that directory.
        let dir = tempfile::tempdir().unwrap();
        let pattern = dir.path().join("*.log");
        let mut discovery =
            NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter).unwrap();

        assert!(discovery.is_watched_dir(dir.path()));

        discovery.forget_watch(dir.path());
        assert!(
            !discovery.is_watched_dir(dir.path()),
            "forget_watch must remove just this directory's bookkeeping"
        );

        discovery.resync_watches(&[pattern], &NoopEmitter);
        assert!(
            discovery.is_watched_dir(dir.path()),
            "resync_watches must re-establish the watch after it was forgotten"
        );
    }

    #[test]
    fn missing_root_falls_back_to_watching_existing_ancestor() {
        // Regression test for a bug found in review: if the literal prefix of an `include`
        // pattern doesn't exist yet at startup (e.g. `/var/log/newapp/*.log` before `newapp` has
        // been created), `watch()`-ing it directly fails and, prior to this fix, nothing was
        // watched at all for that pattern -- its creation would only be noticed on the next
        // `reconcile_interval` backstop tick (potentially minutes away), rather than promptly via
        // a notify event.
        let root = tempfile::tempdir().unwrap();
        let missing_dir = root.path().join("newapp");
        let pattern = missing_dir.join("*.log");
        let mut discovery =
            NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter).unwrap();

        assert!(
            !discovery.is_watched_dir(&missing_dir),
            "the not-yet-existing directory itself should not be directly watched"
        );
        assert_eq!(
            discovery.watched_dirs.get(root.path()),
            Some(&WatchMode::Recursive),
            "the nearest existing ancestor should be watched recursively as a stand-in"
        );

        // Once the directory is created, the next resync should upgrade to watching it directly
        // and drop the broader ancestor watch.
        std::fs::create_dir(&missing_dir).unwrap();
        discovery.resync_watches(&[pattern], &NoopEmitter);
        assert_eq!(
            discovery.watched_dirs.get(&missing_dir),
            Some(&WatchMode::NonRecursive),
            "resync_watches must upgrade to watching the now-existing directory directly"
        );
        assert!(
            !discovery.is_watched_dir(root.path()),
            "the fallback ancestor watch should be dropped once no longer needed"
        );
    }

    #[test]
    fn find_existing_ancestor_treats_relative_top_level_root_as_current_dir() {
        // Regression test for a bug found in review: for a relative `include` pattern with a
        // single-component root (e.g. `logs/*.log`, whose literal prefix is just `logs`),
        // `Path::ancestors()` on a not-yet-existing `logs` yields `logs` then an empty path
        // (`""`) -- there's no further parent for a relative path to walk up to. `Path::is_dir()`
        // on `""` is always `false`, even though `""` denotes the current directory (same as
        // `"."`, which `is_dir()` correctly reports as existing). Before this fix,
        // `find_existing_ancestor` would therefore return `None` for a missing relative
        // top-level root, silently skipping the fallback-ancestor watch entirely: creating
        // `logs` could never be noticed via notify, only via the `reconcile_interval` backstop.
        let missing_relative_root = PathBuf::from("logs");
        let ancestor = find_existing_ancestor(&missing_relative_root)
            .expect("the current directory must be found as an existing ancestor");
        assert!(
            ancestor.is_dir(),
            "the returned ancestor must actually exist and be a directory"
        );
    }

    #[test]
    fn fallback_ancestor_that_is_also_directly_wanted_stays_recursive() {
        // Regression test for a bug found in review: `root` is both (a) a fallback ancestor for
        // `missing_dir`, which doesn't exist yet and needs `root` watched `Recursive` so its
        // eventual creation is noticed, and (b) itself a directly-wanted directory from a second,
        // unrelated `include` pattern that on its own would only need `NonRecursive`.
        // `HashMap`'s unspecified iteration order means the main loop in `resync_watches` could
        // process `root` (as the plain `NonRecursive`-wanted directory) either before or after
        // `missing_dir` triggers the `Recursive` fallback watch on it. Before this fix, whichever
        // order put the direct `NonRecursive` `watch()` call *last* would silently downgrade
        // `root`'s registration from `Recursive` to `NonRecursive`, since the "already watched
        // under the wanted mode" skip-check compared only against the plain per-pattern mode, not
        // the merged requirement. That leaves file creation nested under `root` (which is what the
        // `missing_dir` fallback exists to observe) unnoticed until the next `reconcile_interval`
        // backstop.
        let root = tempfile::tempdir().unwrap();
        let missing_dir = root.path().join("newapp");
        let missing_pattern = missing_dir.join("*.log");
        let direct_pattern = root.path().join("*.log");
        let mut discovery = NotifyDiscovery::new(
            &[missing_pattern.clone(), direct_pattern.clone()],
            &NoopEmitter,
        )
        .unwrap();

        assert_eq!(
            discovery.watched_dirs.get(root.path()),
            Some(&WatchMode::Recursive),
            "root must stay Recursive: it's both directly wanted (NonRecursive on its own) and \
             a fallback ancestor (Recursive) for the not-yet-existing missing_dir"
        );

        // Re-running resync_watches (e.g. the periodic backstop, with nothing on disk having
        // changed) must not downgrade it either, regardless of `wanted`'s iteration order on this
        // second pass.
        discovery.resync_watches(&[missing_pattern, direct_pattern], &NoopEmitter);
        assert_eq!(
            discovery.watched_dirs.get(root.path()),
            Some(&WatchMode::Recursive),
            "root must remain Recursive across repeated resync_watches calls"
        );
    }
}
