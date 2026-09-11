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
//! watching, see `src/config/watcher.rs`). We bridge this into the async world with a bounded
//! `tokio::sync::mpsc::Sender`, using `try_send` (non-blocking) from the notify callback.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc as std_mpsc,
    },
};

use file_source_common::internal_events::FileSourceInternalEvents;
use notify::{
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcherTrait,
    event::ModifyKind,
};
use tokio::{fs, sync::mpsc};
use tracing::{debug, trace, warn};

/// A message delivered from the OS-level file watcher to [`FileServer`](crate::file_server::FileServer).
#[derive(Debug)]
pub enum NotifyMessage {
    /// One or more paths were modified or otherwise changed. This is intentionally coarse: we
    /// don't try to fully interpret notify's (platform-dependent, sometimes ambiguous) event
    /// semantics. Instead we treat any of these as "something changed near this path; go check
    /// it," and let the existing fingerprinting/read logic in `FileServer` figure out the rest.
    /// This is deliberately conservative -- it trades a few spurious wakeups (cheap: a stat +
    /// maybe a fingerprint read) for never having to trust notify's event *kind* classification,
    /// which varies across inotify/FSEvents/ReadDirectoryChangesW.
    PathsChanged(Vec<PathBuf>),
    /// One or more paths were created or may have been moved into a watched directory. These are
    /// also retained as rename candidates because some backends report cross-directory moves as
    /// a creation rather than a rename event.
    PathsCreated(Vec<PathBuf>),
    /// One or more paths were removed or renamed away. In addition to triggering reconciliation,
    /// `FileServer` uses these paths to invalidate bookkeeping for watched directory roots whose
    /// OS-level registrations no longer follow the configured path.
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
/// `Drop` hands the underlying watcher off to `spawn_teardown` rather than dropping it inline, so
/// this is always safe to simply drop -- from anywhere, including `= None` on an `Option`, an
/// early `return`, or a panic unwind -- even when the backend is unhealthy, which `notify`'s own
/// `Drop` impl on the watcher is not: it can itself block (joining a backend thread) or panic (an
/// `unwrap()` on a shutdown-channel send) in that case.
pub struct NotifyDiscovery {
    /// `None` only transiently, while `forget_watches` is tearing down the old watcher before
    /// building its replacement, or once `Drop` has taken it; every other method can assume this
    /// is always `Some`.
    watcher: Option<RecommendedWatcher>,
    watched_dirs: WantedDirs,
    /// For a wanted directory that doesn't exist yet (so it can't be `watch()`-ed directly),
    /// tracks the nearest existing ancestor we're watching recursively instead, keyed by the
    /// *wanted* directory. `resync_watches` uses this to notice once the wanted directory has
    /// been created and upgrade to watching it directly (dropping the broader, more expensive
    /// ancestor watch) rather than watching the ancestor forever. See `resync_watches` for
    /// details.
    fallback_watches: HashMap<PathBuf, PathBuf>,
    receiver: mpsc::Receiver<NotifyMessage>,
    /// Set by `watcher`'s notify callback on a `BackendError`, regardless of whether the
    /// corresponding `NotifyMessage::BackendError` made it onto the (bounded) channel. A
    /// `BackendError` can mean the watcher silently dropped a watch, so it must always trigger
    /// `forget_watches` on the next `resync_watches` call -- relying solely on the channel message
    /// would lose that requirement if the channel happened to be full at the time (see
    /// `NOTIFY_CHANNEL_CAPACITY`), since the callback's sticky recovery flags are the durable
    /// signal when both the original message and its best-effort `Overflow` substitute are lost.
    ///
    /// Owned solely by the current `watcher` generation (see `build_watcher`/`forget_watches`):
    /// never shared with a previous or future watcher, so a stale callback from an already-
    /// replaced watcher can't wrongly flag the current one.
    backend_error_pending: Arc<AtomicBool>,
    /// Set by the current generation when an event is lost because the bridge channel is full,
    /// or when the notify backend reports an OS-level overflow. Unlike a best-effort `Overflow`
    /// message, this survives a full channel and forces the next reconciliation to rebuild all
    /// registrations. It is generation-local for the same reason as `backend_error_pending`.
    overflow_pending: Arc<AtomicBool>,
    /// Maps logical paths stored in `watched_dirs` to their canonical aliases used by backends
    /// such as FSEvents. There can be multiple logical aliases for one canonical path, so this is
    /// intentionally keyed by the logical path rather than the other way around.
    watched_dir_aliases: HashMap<PathBuf, PathBuf>,
    /// Reverse index for `watched_dir_aliases`, so paths reported by notify can be matched to a
    /// watched logical directory without scanning every alias.
    watched_dir_aliases_by_canonical: HashMap<PathBuf, HashSet<PathBuf>>,
}

impl Drop for NotifyDiscovery {
    /// See the struct-level doc comment: this is what makes an ordinary drop of `NotifyDiscovery`
    /// (from anywhere -- an early `return`, `Option::take`, a panic unwind) safe against an
    /// unhealthy backend, without every call site needing to remember to do anything special.
    fn drop(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            let _spawned = spawn_teardown(watcher);
        }
    }
}

/// Bound on the notify event channel: without this, a sustained burst of filesystem events could
/// grow the channel (and the `Vec<PathBuf>` payload of each queued message) without limit while
/// `FileServer` is busy with a reconciliation pass or a slow downstream send. Large enough that
/// ordinary bursts (an editor doing several writes, a batch of files appearing at once) never hit
/// it; a full channel just means an `Overflow` is reported instead of the specific event, which
/// `FileServer` already treats as "something changed, go check everything" via the next
/// reconciliation pass -- the same fallback already used for the OS-level notify queue overflow.
const NOTIFY_CHANNEL_CAPACITY: usize = 8192;

/// Bound the number of detached teardown threads that may retain a wedged watcher. Once this is
/// reached, recovery falls back to polling and uses the separately bounded reaper queue; an
/// exhausted queue may leak a watcher as the last-resort way to preserve caller liveness.
const MAX_IN_FLIGHT_TEARDOWNS: usize = 8;
static IN_FLIGHT_TEARDOWNS: AtomicUsize = AtomicUsize::new(0);

/// Teardowns beyond the dedicated-thread limit are retained here until a background reaper can
/// run their potentially blocking `Drop`. The queue is bounded so a series of wedged backends
/// cannot retain an unbounded number of watcher values. If it is full, the value is deliberately
/// leaked as the last-resort way to keep the caller non-blocking and safe from a panicking `Drop`.
type TeardownTask = Box<dyn FnOnce() + Send + 'static>;
const FALLBACK_TEARDOWN_QUEUE_CAPACITY: usize = MAX_IN_FLIGHT_TEARDOWNS;
static FALLBACK_TEARDOWN_SENDER: OnceLock<std_mpsc::SyncSender<TeardownTask>> = OnceLock::new();

/// `NotifyDiscovery::watcher` is only ever `None` transiently inside `forget_watches`; every
/// other method observing `None` here indicates a bug in this module.
const WATCHER_INVARIANT: &str = "NotifyDiscovery::watcher must be Some outside forget_watches";

/// Get rid of `value` without ever running its `Drop` impl on the calling thread: hand it to a
/// detached thread to drop there instead. Used for the underlying `notify` watcher, whose `Drop`
/// impl can itself block (joining a backend thread) or panic (an `unwrap()` on a shutdown-channel
/// send) if the backend is already unhealthy -- which is exactly the situation this is usually
/// called from (recovering after a `BackendError`, or a dead notify channel).
///
/// A plain `move || drop(value)` closure would defeat this on the failure path: if
/// `Builder::spawn` can't create the OS thread, it drops the closure (and therefore `value`)
/// itself before returning `Err`, which runs the drop on the calling thread right here -- exactly
/// what this function exists to avoid. Instead, `value` goes into a `Mutex` shared via `Arc` with
/// the spawned closure: on the failure path, taking it back out of *this* handle and
/// `mem::forget`-ing it guarantees the calling thread never runs `value`'s `Drop`, regardless of
/// whether `spawn` already dropped the closure's own `Arc` clone (a no-op refcount decrement,
/// since this handle still holds `value`) or never got that far.
///
/// Returns `false` if the value could not be handed to a dedicated teardown thread. When the
/// dedicated-thread limit is reached, it is handed to a bounded background reaper queue when
/// possible; if that queue is full, the value is leaked rather than dropped on the caller. In
/// either case callers should treat the result as notify recovery being unavailable for this
/// generation.
#[must_use]
fn spawn_teardown<T: Send + 'static>(value: T) -> bool {
    let acquired = IN_FLIGHT_TEARDOWNS
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            (count < MAX_IN_FLIGHT_TEARDOWNS).then_some(count + 1)
        })
        .is_ok();
    if !acquired {
        // Dropping `value` here could block or panic on the calling thread. Retain it on a
        // dedicated reaper instead; the false return still disables notify recovery while the
        // old resource is waiting to be released.
        let task: TeardownTask = Box::new(move || drop(value));
        match fallback_teardown_sender() {
            Some(sender) => match sender.try_send(task) {
                Ok(()) => warn!(
                    message = "File watcher teardown thread limit reached; queued teardown and falling back to polling.",
                    limit = MAX_IN_FLIGHT_TEARDOWNS,
                ),
                Err(std_mpsc::TrySendError::Full(task)) => {
                    std::mem::forget(task);
                    warn!(
                        message = "File watcher teardown queue is full; leaking watcher and falling back to polling.",
                        capacity = FALLBACK_TEARDOWN_QUEUE_CAPACITY,
                    );
                }
                Err(std_mpsc::TrySendError::Disconnected(task)) => {
                    // The reaper catches teardown panics and should stay alive. Reaching this
                    // branch means the reaper itself failed unexpectedly; never run the task on
                    // the caller's thread.
                    std::mem::forget(task);
                    warn!(
                        message = "File watcher teardown reaper is unavailable; leaking watcher."
                    );
                }
            },
            None => {
                // There is no safe place left to run a potentially blocking Drop under process
                // resource exhaustion. Preserve caller liveness and report failure to polling.
                std::mem::forget(task);
                warn!(message = "Failed to start file watcher teardown reaper; leaking watcher.");
            }
        }
        return false;
    }

    let permit = TeardownPermit;
    let value = Arc::new(std::sync::Mutex::new(Some(value)));
    let for_thread = Arc::clone(&value);
    let spawned = std::thread::Builder::new()
        .name("notify-watcher-teardown".to_owned())
        .spawn(move || {
            let _permit = permit;
            drop(for_thread.lock().unwrap().take());
        });
    match spawned {
        Ok(_join_handle) => true,
        Err(error) => {
            std::mem::forget(value.lock().unwrap().take());
            warn!(message = "Failed to spawn file watcher teardown thread.", %error);
            false
        }
    }
}

fn fallback_teardown_sender() -> Option<&'static std_mpsc::SyncSender<TeardownTask>> {
    if let Some(sender) = FALLBACK_TEARDOWN_SENDER.get() {
        return Some(sender);
    }

    let (sender, receiver) =
        std_mpsc::sync_channel::<TeardownTask>(FALLBACK_TEARDOWN_QUEUE_CAPACITY);
    std::thread::Builder::new()
        .name("notify-watcher-teardown-reaper".to_owned())
        .spawn(move || {
            while let Ok(task) = receiver.recv() {
                // A broken backend can make Drop panic; keep reaping later resources.
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(task)) {
                    Ok(()) | Err(_) => {}
                }
            }
        })
        .ok()?;

    match FALLBACK_TEARDOWN_SENDER.set(sender) {
        Ok(()) => FALLBACK_TEARDOWN_SENDER.get(),
        Err(sender) => {
            // A concurrent initializer already installed its sender. Wake the reaper belonging
            // to this losing channel so its receiver does not wait forever on an unreachable
            // sender after this local sender is dropped.
            drop(sender.try_send(Box::new(|| {})));
            FALLBACK_TEARDOWN_SENDER.get()
        }
    }
}

struct TeardownPermit;

impl Drop for TeardownPermit {
    fn drop(&mut self) {
        IN_FLIGHT_TEARDOWNS.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Build a fresh `RecommendedWatcher` bridging its synchronous callback into `tx`, along with a
/// new recovery flags owned solely by this watcher generation.
///
/// The flag must not be shared across generations: `forget_watches` replaces the watcher (and
/// its callback thread) without waiting for the old one to actually stop, so callbacks from the
/// old, already-discarded generation can still fire after a new one is already in place. If both
/// generations shared recovery flags, a stale callback could tear down and rebuild the perfectly
/// healthy new watcher too -- forever, if the old backend keeps reporting errors.
fn build_watcher(
    tx: mpsc::Sender<NotifyMessage>,
) -> notify::Result<(RecommendedWatcher, Arc<AtomicBool>, Arc<AtomicBool>)> {
    let backend_error_pending = Arc::new(AtomicBool::new(false));
    let callback_backend_error_pending = Arc::clone(&backend_error_pending);
    let overflow_pending = Arc::new(AtomicBool::new(false));
    let callback_overflow_pending = Arc::clone(&overflow_pending);
    let watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            // This closure runs on a thread owned by the OS notification backend (e.g. the
            // inotify reader thread), so it must not block: `try_send` rather than the
            // blocking/async `send`.
            let msg = match res {
                Ok(event) => classify_event(event),
                Err(error) => {
                    if is_overflow(&error) {
                        callback_overflow_pending.store(true, Ordering::Relaxed);
                        Some(NotifyMessage::Overflow)
                    } else {
                        // Set this unconditionally, independent of whether the channel send
                        // below succeeds: a `BackendError` must always trigger
                        // `forget_watches` on the next `resync_watches`, and the channel
                        // (bounded, and possibly full) is not a reliable way to guarantee
                        // that. See `backend_error_pending`'s doc comment.
                        callback_backend_error_pending.store(true, Ordering::Relaxed);
                        Some(NotifyMessage::BackendError(error.to_string()))
                    }
                }
            };
            let Some(msg) = msg else { return };
            if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(msg) {
                callback_overflow_pending.store(true, Ordering::Relaxed);
                // The channel is full: fall back to reporting an overflow instead of this
                // specific event, same as the OS-level notify queue overflow case above --
                // `FileServer` treats both identically (trigger a full reconciliation pass).
                // If even that doesn't fit, the sticky flag below still guarantees the next
                // reconciliation rebuilds the registrations.
                drop(tx.try_send(NotifyMessage::Overflow));
            }
            // Any other send error means every receiver has been dropped (FileServer shut
            // down or never polled); nothing useful to do about it.
        },
        Config::default(),
    )?;
    Ok((watcher, backend_error_pending, overflow_pending))
}

impl NotifyDiscovery {
    /// Create a new [`NotifyDiscovery`], watching the directories implied by `include_patterns`.
    ///
    /// Returns `Err` if the underlying OS watcher could not be constructed at all (e.g. platform
    /// resource exhaustion, like hitting the inotify instance limit). Callers should treat this
    /// as "notify-based discovery is unavailable" and fall back to relying solely on the
    /// periodic reconciliation pass -- they should NOT treat it as fatal to the file source as a
    /// whole.
    pub async fn new<E: FileSourceInternalEvents>(
        include_patterns: &[PathBuf],
        emitter: &E,
    ) -> notify::Result<Self> {
        let (tx, receiver) = mpsc::channel(NOTIFY_CHANNEL_CAPACITY);
        let (watcher, backend_error_pending, overflow_pending) = build_watcher(tx)?;

        let mut discovery = Self {
            watcher: Some(watcher),
            watched_dirs: WantedDirs::new(),
            fallback_watches: HashMap::new(),
            receiver,
            backend_error_pending,
            overflow_pending,
            watched_dir_aliases: HashMap::new(),
            watched_dir_aliases_by_canonical: HashMap::new(),
        };
        // Can't fail here: `backend_error_pending` is freshly `false`, so this can't hit the
        // rebuild-failure path.
        let succeeded = discovery.resync_watches(include_patterns, emitter).await;
        debug_assert!(succeeded);
        Ok(discovery)
    }

    fn watcher_mut(&mut self) -> &mut RecommendedWatcher {
        self.watcher.as_mut().expect(WATCHER_INVARIANT)
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
    ///
    /// Returns `false` if a pending backend error forced a watcher rebuild that failed; callers
    /// should then stop using notify-based discovery entirely and fall back to polling.
    #[must_use]
    pub async fn resync_watches<E: FileSourceInternalEvents>(
        &mut self,
        include_patterns: &[PathBuf],
        emitter: &E,
    ) -> bool {
        // A backend error or overflow since the last call means registrations may be missing;
        // forget all bookkeeping so every directory below is re-`watch`ed from scratch. These
        // sticky flags are checked here rather than relying only on channel messages, because a
        // full channel can drop both the original event and the best-effort `Overflow` message.
        let backend_error_pending = self.backend_error_pending.swap(false, Ordering::Relaxed);
        let overflow_pending = self.overflow_pending.swap(false, Ordering::Relaxed);
        if overflow_pending {
            emitter.emit_file_watch_events_overflowed();
        }
        if (backend_error_pending || overflow_pending) && !self.forget_watches() {
            return false;
        }

        // Absolutize first: `notify` always resolves the path it's asked to `watch()` to an
        // absolute one internally and reports events using that form, but `include_patterns` can
        // be relative. Without this, `watched_dirs`/`fallback_watches` would be keyed by relative
        // paths that never match the absolute paths notify events carry (e.g. in
        // `is_watched_dir`/`forget_watch`).
        let cwd = std::env::current_dir().ok();
        let include_patterns: Vec<PathBuf> = include_patterns
            .iter()
            .map(|p| crate::absolutize(p, cwd.as_deref()))
            .collect();
        let wanted = compute_watch_directories(&include_patterns);

        // A watch opened through a symlink follows the target inode. If the symlink is retargeted,
        // the logical path and watch mode are unchanged, so the ordinary wanted-vs-watched diff
        // below would incorrectly keep the old target registered forever. Rebuild when a cached
        // canonical target changes so the new watcher follows the new directory.
        let watched_paths: Vec<PathBuf> = self.watched_dirs.keys().cloned().collect();
        for watched in watched_paths {
            let Some(previous_canonical) = self.watched_dir_aliases.get(&watched) else {
                continue;
            };
            if *previous_canonical != canonicalize_watch_path(&watched).await {
                if !self.forget_watches() {
                    return false;
                }
                return Box::pin(self.resync_watches(&include_patterns, emitter)).await;
            }
        }

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
            match self.watcher_mut().watch(path, mode.mode()) {
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
                    if is_missing_watch_path(&error, path).await {
                        match find_existing_ancestor(path).await {
                            Some(ancestor) => {
                                self.watch_fallback_ancestor(path, ancestor, emitter).await
                            }
                            None => {
                                warn!(message = "Failed to watch directory.", path = ?path, %error);
                                emitter.emit_file_watch_backend_error(&std::io::Error::other(
                                    error.to_string(),
                                ));
                            }
                        }
                    } else {
                        // Do not widen a permission or resource-limit failure to a recursive
                        // watch on a potentially very large ancestor. Leave this path unwatched;
                        // the next reconciliation pass will retry the direct registration.
                        warn!(message = "Failed to watch directory.", path = ?path, %error);
                        emitter.emit_file_watch_backend_error(&std::io::Error::other(
                            error.to_string(),
                        ));
                    }
                }
            }
        }

        self.fallback_watches
            .retain(|path, _ancestor| wanted.contains_key(path));
        // A directory stays watched if it's directly wanted, or if some still-wanted directory
        // depends on it as its fallback ancestor; anything else is stale. Do not call
        // `unwatch()` here: several notify backends synchronously wait for their worker thread,
        // so a broken backend could block the FileServer task. Rebuild the watcher instead; the
        // replacement gets the complete desired set below without any inline backend teardown.
        let ancestors_in_use: std::collections::HashSet<&PathBuf> =
            self.fallback_watches.values().collect();
        let has_stale_watches = self
            .watched_dirs
            .keys()
            .any(|path| !wanted.contains_key(path) && !ancestors_in_use.contains(path));
        if has_stale_watches {
            if !self.forget_watches() {
                return false;
            }
            return Box::pin(self.resync_watches(&include_patterns, emitter)).await;
        }

        self.watched_dir_aliases
            .retain(|watched, _| self.watched_dirs.contains_key(watched));
        // Canonicalization is only needed for a newly registered logical path. Recomputing every
        // alias on every reconciliation made the periodic backstop perform one synchronous
        // filesystem walk per watched directory.
        let missing_aliases: Vec<PathBuf> = self
            .watched_dirs
            .keys()
            .filter(|watched| !self.watched_dir_aliases.contains_key(*watched))
            .cloned()
            .collect();
        for watched in missing_aliases {
            self.watched_dir_aliases
                .insert(watched.clone(), canonicalize_watch_path(&watched).await);
        }
        self.rebuild_watched_dir_alias_index();

        emitter.emit_file_watch_directories(self.watched_dirs.len());
        true
    }

    fn rebuild_watched_dir_alias_index(&mut self) {
        self.watched_dir_aliases_by_canonical.clear();
        for (logical, canonical) in &self.watched_dir_aliases {
            self.watched_dir_aliases_by_canonical
                .entry(canonical.clone())
                .or_default()
                .insert(logical.clone());
        }
    }

    /// Watch `ancestor` (recursively, so creation of `wanted` underneath it is observed) as a
    /// stand-in for the not-yet-existing `wanted` directory, recording the substitution in
    /// `fallback_watches` so a later `resync_watches` call can detect once `wanted` exists and
    /// upgrade to watching it directly.
    async fn watch_fallback_ancestor<E: FileSourceInternalEvents>(
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
        match self
            .watcher_mut()
            .watch(&ancestor, RecursiveMode::Recursive)
        {
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

    /// Recover from a lost/uncertain watch state by discarding the underlying OS watcher and
    /// building a brand new one, rather than trying to `unwatch()` the old registrations (which
    /// isn't reliable cleanup on every backend, and can block if the backend is already
    /// unhealthy). Call this after a [`NotifyMessage::BackendError`] or [`NotifyMessage::Overflow`].
    ///
    /// The old watcher is moved onto a detached thread and dropped there, without awaiting that
    /// thread, since `notify`'s own `Drop` impls can themselves block or panic if the backend is
    /// already wedged. This is a bounded compromise, not a complete fix: `notify` gives no way to
    /// know when the old backend's thread actually exits, so one teardown can still retain its
    /// fd/thread for an unbounded time, and the new watcher is built without waiting for that
    /// teardown, so the two can briefly coexist under fd pressure. The number of detached teardown
    /// threads is bounded; once the limit is reached the old watcher is retained by a background
    /// reaper and this method returns `false`, so the caller falls back to polling. If the bounded
    /// reaper queue is also full, the old watcher is leaked as the last-resort way to avoid
    /// blocking or panicking in the FileServer task.
    ///
    /// Each generation gets its own `backend_error_pending` flag (from `build_watcher`), not one
    /// shared across generations: the old watcher's callback can still fire after the new one is
    /// installed, and sharing the flag would let it wrongly mark the new, healthy watcher for
    /// another (possibly endless) teardown.
    ///
    /// Returns `false` if the new watcher could not be created, or if the old one could not be
    /// handed off for asynchronous teardown. Callers should then stop using notify-based
    /// discovery and fall back to polling, the same as any other unrecoverable resource
    /// exhaustion here.
    #[must_use]
    pub fn forget_watches(&mut self) -> bool {
        let old_watcher_handed_off = match self.watcher.take() {
            Some(old_watcher) => spawn_teardown(old_watcher),
            None => true,
        };
        if !old_watcher_handed_off {
            // Do not construct a replacement after teardown failed: the old watcher is either
            // queued for asynchronous teardown or retained as an unavoidable last-resort leak;
            // creating another backend would only increase resource pressure before polling takes
            // over.
            return false;
        }

        let (tx, receiver) = mpsc::channel(NOTIFY_CHANNEL_CAPACITY);
        match build_watcher(tx) {
            Ok((watcher, backend_error_pending, overflow_pending)) => {
                self.watcher = Some(watcher);
                self.receiver = receiver;
                self.backend_error_pending = backend_error_pending;
                self.overflow_pending = overflow_pending;
                self.watched_dirs.clear();
                self.fallback_watches.clear();
                self.watched_dir_aliases.clear();
                self.watched_dir_aliases_by_canonical.clear();
                true
            }
            Err(error) => {
                warn!(message = "Failed to rebuild file watcher after backend error.", %error);
                false
            }
        }
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
        let mut watched_paths = HashSet::new();
        if self.watched_dirs.contains_key(path) {
            watched_paths.insert(path.to_path_buf());
        }
        if let Some(aliases) = self.watched_dir_aliases_by_canonical.get(path) {
            watched_paths.extend(aliases.iter().cloned());
        }
        for watched_path in watched_paths {
            self.watched_dirs.remove(&watched_path);
            if let Some(canonical) = self.watched_dir_aliases.remove(&watched_path)
                && let Some(logicals) = self.watched_dir_aliases_by_canonical.get_mut(&canonical)
            {
                logicals.remove(&watched_path);
                if logicals.is_empty() {
                    self.watched_dir_aliases_by_canonical.remove(&canonical);
                }
            }
        }
    }

    /// Whether `path` is currently believed to be a watched directory (as opposed to, say, a file
    /// inside one). Used to decide whether a [`NotifyMessage::PathsRemoved`] path warrants
    /// `forget_watch`.
    pub fn is_watched_dir(&self, path: &Path) -> bool {
        self.watched_dirs.contains_key(path)
            || self.watched_dir_aliases_by_canonical.contains_key(path)
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
async fn find_existing_ancestor(path: &Path) -> Option<PathBuf> {
    for ancestor in path.ancestors().skip(1) {
        let ancestor = if ancestor.as_os_str().is_empty() {
            Path::new(".")
        } else {
            ancestor
        };
        if fs::metadata(ancestor)
            .await
            .is_ok_and(|metadata| metadata.is_dir())
        {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

/// Canonicalize a watched directory when it exists, while preserving the unresolved suffix for a
/// path whose final components have not been created yet. This mirrors the path representation
/// used by FSEvents without preventing fallback watches for not-yet-created directories.
async fn canonicalize_watch_path(path: &Path) -> PathBuf {
    let mut unresolved = Vec::new();
    let mut candidate = path.to_path_buf();
    loop {
        if let Ok(canonical) = fs::canonicalize(&candidate).await {
            let mut result = canonical;
            for component in unresolved.iter().rev() {
                result.push(component);
            }
            return result;
        }

        let Some(file_name) = candidate.file_name() else {
            return path.to_path_buf();
        };
        unresolved.push(file_name.to_owned());
        let Some(parent) = candidate.parent() else {
            return path.to_path_buf();
        };
        candidate = parent.to_path_buf();
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
    // `Flag::Rescan` means notify itself knows that events may have been lost (for example,
    // inotify's kernel queue overflow). Preserve that signal instead of reducing it to a generic
    // path change, so the caller emits overflow telemetry and rebuilds its registrations.
    if event.need_rescan() {
        return Some(NotifyMessage::Overflow);
    }

    match event.kind {
        EventKind::Create(_) => Some(NotifyMessage::PathsCreated(event.paths)),
        EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_) | ModifyKind::Metadata(_)) => {
            Some(NotifyMessage::PathsChanged(event.paths))
        }
        // A rename can leave an inotify watch attached to the moved inode instead of the
        // configured path. Preserve the paths in the removal variant so FileServer can invalidate
        // any matching watched-root bookkeeping before the next resync.
        EventKind::Modify(ModifyKind::Name(_)) => Some(NotifyMessage::PathsRemoved(event.paths)),
        EventKind::Remove(_) => Some(NotifyMessage::PathsRemoved(event.paths)),
        EventKind::Modify(ModifyKind::Other) => Some(NotifyMessage::PathsChanged(event.paths)),
        EventKind::Any | EventKind::Other => {
            // Deliberately conservative: an event we can't classify might still be relevant
            // (some backends report generic "Any" for things we care about), so treat it as a
            // change rather than silently dropping it. Access events are the only kind we
            // intentionally ignore below.
            Some(NotifyMessage::PathsChanged(event.paths))
        }
        EventKind::Access(_) => {
            debug!(message = "Ignoring filesystem access event.", paths = ?event.paths);
            None
        }
    }
}

fn is_overflow(error: &notify::Error) -> bool {
    error.to_string().to_lowercase().contains("overflow")
}

async fn is_missing_watch_path(error: &notify::Error, path: &Path) -> bool {
    matches!(&error.kind, notify::ErrorKind::PathNotFound)
        || matches!(
            &error.kind,
            notify::ErrorKind::Io(error) if error.kind() == std::io::ErrorKind::NotFound
        )
        // The Windows backend checks the path with `Path::is_dir`/`is_file` before issuing
        // the OS call and reports that check as a generic error rather than `PathNotFound`.
        || (matches!(
            &error.kind,
            notify::ErrorKind::Generic(message)
                if message == "Input watch path is neither a file nor a directory."
        ) && fs::metadata(path)
            .await
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound))
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, RenameMode};

    #[test]
    fn spawn_teardown_drops_the_value_exactly_once_off_the_calling_thread() {
        // Regression test for a bug found in review: an earlier version of `spawn_teardown` used
        // a plain `move || drop(value)` closure, which `Builder::spawn` would itself drop --
        // running the drop on the calling thread -- if it failed to create the OS thread. This
        // can't easily force that failure path (spawning threads essentially never fails in a
        // test), but it does verify the success path's actual guarantee: the value is dropped
        // exactly once, and not on the thread that called `spawn_teardown`.
        struct DropRecorder {
            dropped_on: Arc<std::sync::Mutex<Option<std::thread::ThreadId>>>,
            drop_count: Arc<std::sync::atomic::AtomicUsize>,
        }
        impl Drop for DropRecorder {
            fn drop(&mut self) {
                *self.dropped_on.lock().unwrap() = Some(std::thread::current().id());
                self.drop_count
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }

        let dropped_on = Arc::new(std::sync::Mutex::new(None));
        let drop_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calling_thread = std::thread::current().id();
        let recorder = DropRecorder {
            dropped_on: Arc::clone(&dropped_on),
            drop_count: Arc::clone(&drop_count),
        };

        assert!(
            spawn_teardown(recorder),
            "spawning the teardown thread should succeed here"
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while drop_count.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for the teardown thread to drop the value"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert_eq!(
            drop_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the value must be dropped exactly once"
        );
        assert_ne!(
            dropped_on.lock().unwrap().unwrap(),
            calling_thread,
            "the value must not be dropped on the thread that called spawn_teardown"
        );
    }

    #[test]
    fn fallback_teardown_reaper_drops_queued_values_off_the_calling_thread() {
        struct DropRecorder(Arc<std::sync::Mutex<Option<std::thread::ThreadId>>>);
        impl Drop for DropRecorder {
            fn drop(&mut self) {
                *self.0.lock().unwrap() = Some(std::thread::current().id());
            }
        }

        let dropped_on = Arc::new(std::sync::Mutex::new(None));
        let dropped_on_for_task = Arc::clone(&dropped_on);
        let calling_thread = std::thread::current().id();
        let sender = fallback_teardown_sender().expect("teardown reaper should start");

        sender
            .try_send(Box::new(move || drop(DropRecorder(dropped_on_for_task))))
            .expect("teardown reaper should have queue capacity");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while dropped_on.lock().unwrap().is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for the fallback teardown reaper"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert_ne!(
            dropped_on.lock().unwrap().unwrap(),
            calling_thread,
            "queued teardown work must not run on the calling thread"
        );
    }

    #[test]
    fn max_files_watch_is_not_classified_as_event_overflow() {
        assert!(!is_overflow(&notify::Error::new(
            notify::ErrorKind::MaxFilesWatch,
        )));
        assert!(is_overflow(&notify::Error::generic(
            "filesystem event queue overflowed",
        )));
    }

    #[test]
    fn rescan_flag_is_classified_as_event_overflow() {
        assert!(matches!(
            classify_event(Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan)),
            Some(NotifyMessage::Overflow)
        ));
    }

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

    #[tokio::test]
    async fn dropping_discovery_hands_the_watcher_to_the_teardown_thread() {
        // Regression test for a bug found in review: `FileServer` used to drop the whole
        // `NotifyDiscovery` on several paths (`notify_discovery = None`, and an early `return Err`
        // when the output channel closed), each of which ran the underlying watcher's `Drop`
        // inline -- exactly the hang/panic risk `forget_watches`'s detached teardown avoids, just
        // reached another way. `NotifyDiscovery::drop` now routes through the same teardown, so
        // every such path is safe without the call site having to remember anything.
        let dir = tempfile::tempdir().unwrap();
        let pattern = dir.path().join("*.log");
        let discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();

        let start = std::time::Instant::now();
        drop(discovery);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(1),
            "dropping NotifyDiscovery must return promptly, handing teardown to another thread \
             rather than running the watcher's own (possibly blocking) Drop inline"
        );
    }

    #[tokio::test]
    async fn forget_watches_rebuilds_a_working_watcher() {
        // Regression test for a bug found in review: `forget_watches` used to only best-effort
        // `unwatch()` old paths and clear bookkeeping, which isn't reliable cleanup on every
        // notify backend and can block if the backend is already unhealthy. It now discards the
        // whole `RecommendedWatcher` (and its bridge channel) and builds a new one instead.
        let dir = tempfile::tempdir().unwrap();
        let pattern = dir.path().join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();

        assert!(discovery.forget_watches());

        // The old watcher (and its bridge channel) is gone at this point, and `resync_watches`
        // hasn't re-`watch`ed anything on the new one yet, so an event on the old, still-watched
        // directory must not reach `discovery.recv()` -- if it did, that would mean the old
        // watcher (or its channel) were somehow still wired up rather than genuinely replaced.
        std::fs::write(dir.path().join("stale.log"), b"hello\n").unwrap();
        let leaked_old_event =
            tokio::time::timeout(std::time::Duration::from_millis(200), discovery.recv());
        assert!(
            leaked_old_event.await.is_err(),
            "no event should arrive from the old, detached watcher/channel"
        );

        // Once resync_watches re-establishes watches on the *new* watcher, a real filesystem
        // event must be delivered end-to-end through the new channel.
        assert!(discovery.resync_watches(&[pattern], &NoopEmitter).await);
        std::fs::write(dir.path().join("new.log"), b"hello\n").unwrap();

        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), discovery.recv())
            .await
            .expect("timed out waiting for a notify event from the rebuilt watcher");
        assert!(
            msg.is_some(),
            "the rebuilt watcher's channel must still be alive"
        );
    }

    #[tokio::test]
    async fn stale_generations_backend_error_does_not_flag_the_new_watcher() {
        // Regression test for a bug found in review: `forget_watches` detaches the old watcher's
        // teardown from the critical path, so its callback thread isn't guaranteed to have
        // stopped by the time a new watcher is already installed. If both generations shared one
        // `backend_error_pending` flag, a `BackendError` the old (already-replaced) watcher
        // observes while winding down could still fire after the swap and wrongly flag the brand
        // new, healthy watcher for another teardown -- and again, and again, if the old backend
        // keeps erroring for a while. Each generation must own its own flag instead.
        let dir = tempfile::tempdir().unwrap();
        let pattern = dir.path().join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();

        // Capture the first generation's flag before replacing it.
        let old_generation_flag = Arc::clone(&discovery.backend_error_pending);

        assert!(discovery.forget_watches());
        assert!(discovery.resync_watches(&[pattern], &NoopEmitter).await);
        assert_eq!(
            discovery.watched_dirs.len(),
            1,
            "sanity check: the new generation re-established its watch"
        );

        // Simulate the old generation's callback firing late, after the new one is in place.
        old_generation_flag.store(true, Ordering::Relaxed);

        assert!(
            !discovery.backend_error_pending.load(Ordering::Relaxed),
            "the new generation's own flag must be untouched by the old generation's callback"
        );
    }

    #[tokio::test]
    async fn forget_watches_makes_resync_re_watch_everything() {
        // Regression test: forget_watches must leave resync_watches believing every directory is
        // unwatched, so a lost watch (e.g. after a BackendError) gets re-established rather than
        // being skipped as "already watched".
        let dir = tempfile::tempdir().unwrap();
        let pattern = dir.path().join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();

        assert_eq!(
            discovery.watched_dirs.len(),
            1,
            "resync_watches (called from `new`) should have recorded the one watched directory"
        );

        assert!(discovery.forget_watches());
        assert!(
            discovery.watched_dirs.is_empty(),
            "forget_watches must clear the watched-directories bookkeeping"
        );

        // With bookkeeping cleared but `include_patterns` unchanged, resync_watches must
        // re-`watch` (not skip) the directory, ending up back where it started.
        assert!(discovery.resync_watches(&[pattern], &NoopEmitter).await);
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

    #[tokio::test]
    async fn forget_watches_unwatches_and_clears_fallback_watches_too() {
        // Regression test: forget_watches must also clear fallback_watches, not just
        // watched_dirs, or an ancestor watch can become permanently orphaned.
        let root = tempfile::tempdir().unwrap();
        let missing_dir = root.path().join("newapp");
        let pattern = missing_dir.join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();

        assert!(
            discovery.is_watched_dir(root.path()),
            "sanity check: the fallback ancestor should be watched"
        );
        assert!(
            !discovery.fallback_watches.is_empty(),
            "sanity check: a fallback relationship should be recorded"
        );

        assert!(discovery.forget_watches());
        assert!(
            discovery.watched_dirs.is_empty(),
            "forget_watches must clear watched_dirs"
        );
        assert!(
            discovery.fallback_watches.is_empty(),
            "forget_watches must also clear fallback_watches, not just watched_dirs"
        );

        // resync_watches must still work correctly afterwards: it re-derives the fallback
        // relationship from scratch rather than relying on anything forget_watches left behind.
        assert!(discovery.resync_watches(&[pattern], &NoopEmitter).await);
        assert!(discovery.is_watched_dir(root.path()));
    }

    #[tokio::test]
    async fn backend_error_pending_forces_forget_watches_even_without_the_channel_message() {
        // Regression test for a bug found in review: the notify channel is bounded
        // (`NOTIFY_CHANNEL_CAPACITY`), so a `BackendError` can lose its race to a full channel.
        // `backend_error_pending` is set directly by the callback, independent of whether the
        // `BackendError` message itself made it onto the channel, so `resync_watches` must still
        // forget all bookkeeping even when no `BackendError` message was ever received.
        let dir = tempfile::tempdir().unwrap();
        let pattern = dir.path().join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();
        assert_eq!(discovery.watched_dirs.len(), 1);

        // Simulate the callback having observed a BackendError that lost the race for channel
        // space, without ever going through `handle_notify_message`.
        discovery
            .backend_error_pending
            .store(true, Ordering::Relaxed);

        assert!(discovery.resync_watches(&[pattern], &NoopEmitter).await);
        assert_eq!(
            discovery.watched_dirs.len(),
            1,
            "resync_watches must still re-establish the watch via the sticky flag alone"
        );
        assert!(
            !discovery.backend_error_pending.load(Ordering::Relaxed),
            "the flag must be cleared once acted on"
        );
    }

    #[tokio::test]
    async fn overflow_pending_forces_forget_watches_even_without_the_channel_message() {
        let dir = tempfile::tempdir().unwrap();
        let pattern = dir.path().join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();
        assert_eq!(discovery.watched_dirs.len(), 1);

        discovery.overflow_pending.store(true, Ordering::Relaxed);

        assert!(discovery.resync_watches(&[pattern], &NoopEmitter).await);
        assert!(discovery.is_watched_dir(dir.path()));
        assert!(!discovery.overflow_pending.load(Ordering::Relaxed));
    }

    #[test]
    fn rename_events_are_classified_as_removed_paths() {
        let path = PathBuf::from("/var/log/app");
        let event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
            .add_path(path.clone());

        assert!(matches!(
            classify_event(event),
            Some(NotifyMessage::PathsRemoved(paths)) if paths == vec![path]
        ));
    }

    #[test]
    fn unclassified_rename_events_are_kept_as_removed_paths() {
        let path = PathBuf::from("/var/log/app");
        let event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Other)))
            .add_path(path.clone());

        assert!(matches!(
            classify_event(event),
            Some(NotifyMessage::PathsRemoved(paths)) if paths == vec![path]
        ));
    }

    #[test]
    fn pathless_generic_events_trigger_reconciliation() {
        assert!(matches!(
            classify_event(Event::new(EventKind::Any)),
            Some(NotifyMessage::PathsChanged(paths)) if paths.is_empty()
        ));
        assert!(matches!(
            classify_event(Event::new(EventKind::Other)),
            Some(NotifyMessage::PathsChanged(paths)) if paths.is_empty()
        ));
    }

    #[test]
    fn created_events_are_classified_as_created_paths() {
        let path = PathBuf::from("/var/log/app.log.1");
        let event = Event::new(EventKind::Create(CreateKind::Any)).add_path(path.clone());

        assert!(matches!(
            classify_event(event),
            Some(NotifyMessage::PathsCreated(paths)) if paths == vec![path]
        ));
    }

    #[test]
    fn other_created_events_are_rename_candidates() {
        let path = PathBuf::from("/var/log/app.log.1");
        let event = Event::new(EventKind::Create(CreateKind::Other)).add_path(path.clone());

        assert!(matches!(
            classify_event(event),
            Some(NotifyMessage::PathsCreated(paths)) if paths == vec![path]
        ));
    }

    #[tokio::test]
    async fn only_missing_path_errors_use_ancestor_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let existing_path = dir.path().to_owned();
        let missing_path = dir.path().join("missing");
        let generic_error = notify::Error::new(notify::ErrorKind::Generic(
            "Input watch path is neither a file nor a directory.".to_owned(),
        ));

        assert!(is_missing_watch_path(&notify::Error::path_not_found(), &missing_path).await);
        assert!(
            is_missing_watch_path(
                &notify::Error::io(std::io::Error::from(std::io::ErrorKind::NotFound)),
                &missing_path
            )
            .await
        );
        assert!(is_missing_watch_path(&generic_error, &missing_path).await);
        assert!(!is_missing_watch_path(&generic_error, &existing_path).await);
        assert!(
            !is_missing_watch_path(
                &notify::Error::new(notify::ErrorKind::MaxFilesWatch),
                &existing_path
            )
            .await
        );
        assert!(
            !is_missing_watch_path(
                &notify::Error::io(std::io::Error::from(std::io::ErrorKind::PermissionDenied),),
                &existing_path
            )
            .await
        );
    }

    #[tokio::test]
    async fn forget_watch_makes_resync_re_watch_one_directory() {
        // Regression test for a bug found in review: removing a watched *directory* on
        // Linux/inotify invalidates the watch on that inode, but a `PathsRemoved` notification for
        // it used to be handled the same as any other path event (just triggering a reconciliation
        // pass), leaving the directory recorded in `watched_dirs`. `resync_watches`'s "only
        // `watch()` a directory we don't already believe is watched" check would then skip
        // re-`watch`-ing it even after it was recreated, permanently falling back to the
        // much-less-frequent backstop reconciliation for that directory.
        let dir = tempfile::tempdir().unwrap();
        let pattern = dir.path().join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();

        assert!(discovery.is_watched_dir(dir.path()));

        discovery.forget_watch(dir.path());
        assert!(
            !discovery.is_watched_dir(dir.path()),
            "forget_watch must remove just this directory's bookkeeping"
        );

        assert!(discovery.resync_watches(&[pattern], &NoopEmitter).await);
        assert!(
            discovery.is_watched_dir(dir.path()),
            "resync_watches must re-establish the watch after it was forgotten"
        );
    }

    #[tokio::test]
    async fn resync_watches_absolutizes_relative_include_patterns() {
        // Regression test for a bug found in review: `watched_dirs` used to be keyed by whatever
        // form `include_patterns` came in, which can be relative (e.g. `include: ["logs/*.log"]`).
        // `notify` always reports its events using absolute paths, so `is_watched_dir`/
        // `forget_watch` (used by `PathsRemoved` handling to invalidate a lost watch) would never
        // match a relative key against the absolute path notify reports for the same directory,
        // leaving a stale registration in place after the directory is removed and recreated.
        let cwd = std::env::current_dir().unwrap();
        // Created directly under the test process's cwd (rather than the system temp directory,
        // which `tempfile::tempdir()` would use and which usually isn't under `cwd`), so a
        // relative pattern can always be constructed via `strip_prefix` below -- otherwise this
        // test would silently skip its own assertion on most platforms/setups.
        let dir = tempfile::tempdir_in(&cwd).unwrap();
        let relative_dir = dir
            .path()
            .strip_prefix(&cwd)
            .expect("dir was created under cwd");
        let pattern = relative_dir.join("*.log");
        let discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();

        assert!(
            discovery.is_watched_dir(dir.path()),
            "watched_dirs must be keyed by the absolute path, matching what notify reports, \
             even though the include pattern was relative"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_watch_paths_match_canonical_and_logical_removal_events() {
        let root = tempfile::tempdir().unwrap();
        let real_dir = root.path().join("real");
        std::fs::create_dir(&real_dir).unwrap();
        let logical_dir = root.path().join("logical");
        std::os::unix::fs::symlink(&real_dir, &logical_dir).unwrap();
        let pattern = logical_dir.join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();
        let canonical_dir = std::fs::canonicalize(&real_dir).unwrap();

        assert!(discovery.is_watched_dir(&canonical_dir));
        assert!(discovery.is_watched_dir(&logical_dir));

        // The canonical target may already be gone when the removal event is handled. The
        // retained alias must still identify the logical watch in that case.
        std::fs::remove_dir(&real_dir).unwrap();
        assert!(discovery.is_watched_dir(&canonical_dir));
        discovery.forget_watch(&canonical_dir);
        assert!(!discovery.is_watched_dir(&canonical_dir));
        assert!(!discovery.is_watched_dir(&logical_dir));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn resync_rebuilds_a_watch_when_a_symlink_target_changes() {
        let root = tempfile::tempdir().unwrap();
        let first_target = root.path().join("first");
        let second_target = root.path().join("second");
        std::fs::create_dir(&first_target).unwrap();
        std::fs::create_dir(&second_target).unwrap();
        let logical_dir = root.path().join("logical");
        std::os::unix::fs::symlink(&first_target, &logical_dir).unwrap();
        let pattern = logical_dir.join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();

        std::fs::remove_file(&logical_dir).unwrap();
        std::os::unix::fs::symlink(&second_target, &logical_dir).unwrap();

        assert!(discovery.resync_watches(&[pattern], &NoopEmitter).await);
        let second_canonical = std::fs::canonicalize(&second_target).unwrap();
        assert!(
            discovery.is_watched_dir(&second_canonical),
            "resync must replace a watch that still points at the old symlink target"
        );
        assert_eq!(
            discovery.watched_dir_aliases.get(&logical_dir),
            Some(&second_canonical)
        );
    }

    #[tokio::test]
    async fn missing_root_falls_back_to_watching_existing_ancestor() {
        // Regression test for a bug found in review: if the literal prefix of an `include`
        // pattern doesn't exist yet at startup (e.g. `/var/log/newapp/*.log` before `newapp` has
        // been created), `watch()`-ing it directly fails and, prior to this fix, nothing was
        // watched at all for that pattern -- its creation would only be noticed on the next
        // `reconcile_interval` backstop tick (potentially minutes away), rather than promptly via
        // a notify event.
        let root = tempfile::tempdir().unwrap();
        let missing_dir = root.path().join("newapp");
        let pattern = missing_dir.join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();

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
        assert!(discovery.resync_watches(&[pattern], &NoopEmitter).await);
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

    #[tokio::test]
    async fn find_existing_ancestor_treats_relative_top_level_root_as_current_dir() {
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
            .await
            .expect("the current directory must be found as an existing ancestor");
        assert!(
            ancestor.is_dir(),
            "the returned ancestor must actually exist and be a directory"
        );
    }

    #[tokio::test]
    async fn fallback_ancestor_that_is_also_directly_wanted_stays_recursive() {
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
        .await
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
        assert!(
            discovery
                .resync_watches(&[missing_pattern, direct_pattern], &NoopEmitter)
                .await
        );
        assert_eq!(
            discovery.watched_dirs.get(root.path()),
            Some(&WatchMode::Recursive),
            "root must remain Recursive across repeated resync_watches calls"
        );
    }
}
