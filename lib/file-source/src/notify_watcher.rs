//! An OS-level, event-driven alternative/augmentation to the periodic glob-rescan discovery
//! mechanism in [`crate::file_server::FileServer`].
//!
//! # Design
//!
//! Re-globbing `include` on a fixed interval is expensive with many matched files (see
//! <https://github.com/vectordotdev/vector/issues/3567>): every rescan fingerprints every file, and
//! each keeps an open handle for its whole lifetime, even when `ignore_older` excludes it from
//! reading.
//!
//! This module watches the *parent directories* instead, via [`notify`] (inotify, FSEvents,
//! `ReadDirectoryChangesW`), turning OS events into [`NotifyMessage`]s. A much rarer reconciliation
//! pass remains as a correctness backstop: notification queues can overflow silently, and there is a
//! TOCTOU gap between scanning a directory and establishing its watch.
//!
//! # Directory selection
//!
//! `notify` watches directories, not patterns, so each `include` contributes its longest literal
//! prefix. Recursive only when the pattern can reach arbitrarily deep (`**`), which approximates how
//! far the glob reaches below that prefix.
//!
//! A prefix that does not exist yet is stood in for by its nearest existing ancestor, so its creation
//! is still seen without recursively watching a large tree; the next `resync_watches` upgrades to the
//! real directory.
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
    /// tracks the nearest existing ancestor we're watching non-recursively instead, keyed by the
    /// *wanted* directory. `resync_watches` uses this to notice once the wanted directory has
    /// been created and upgrade to watching it directly (dropping the temporary ancestor watch)
    /// rather than watching the ancestor forever. See `resync_watches` for details.
    fallback_watches: HashMap<PathBuf, PathBuf>,
    /// Parent directories watched non-recursively to observe replacement of symlink components in
    /// the logical path. These registrations are supplemental: they are retained even though
    /// they are not themselves implied by an include directory.
    symlink_parent_watches: HashSet<PathBuf>,
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
    /// Set when a `resync_watches` call consumed a sticky recovery flag, meaning registrations were
    /// rebuilt and arbitrary events may have been lost. Read (and cleared) by the caller via
    /// `take_full_scan_required`, which must then run a full glob pass instead of a targeted one.
    full_scan_required: bool,
    /// Whether some configured directory could not be watched -- a permission error, or the backend
    /// watch limit. No event can arrive for anything under it, so events alone no longer cover the
    /// configuration and the caller must keep reconciling on the polling cadence.
    uncovered_roots: bool,
    /// Set when an already-watched directory needs a different recursive mode than it has.
    ///
    /// Handled by rebuilding the watcher rather than re-`watch()`-ing in place, which leaks a
    /// directory handle on the Windows backend (see `watch_fallback_ancestor`).
    mode_change_pending: bool,
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
/// A plain `move || drop(value)` closure defeats this: a failed `Builder::spawn` drops the closure --
/// and `value` with it -- on the calling thread. So `value` goes into an `Arc<Mutex<_>>`, and the
/// failure path takes it back out and `mem::forget`s it.
///
/// `false` means no dedicated teardown thread was available; past the thread limit the value goes to
/// a bounded reaper queue, and if that is full it is leaked rather than dropped on the caller.
/// Either way, notify recovery is unavailable for this generation.
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
            symlink_parent_watches: HashSet::new(),
            receiver,
            backend_error_pending,
            overflow_pending,
            watched_dir_aliases: HashMap::new(),
            watched_dir_aliases_by_canonical: HashMap::new(),
            full_scan_required: false,
            uncovered_roots: false,
            mode_change_pending: false,
        };
        // A rebuild can still fail here under resource pressure, and a `debug_assert` is compiled
        // out of release builds -- leaving `watcher: None` while the caller treats notify as live,
        // which panics in `watcher_mut()` instead of falling back to polling.
        if !discovery.resync_watches(include_patterns, emitter).await {
            return Err(notify::Error::generic(
                "failed to establish initial file system watches",
            ));
        }
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
    /// fails; this falls back to non-recursively watching the nearest existing ancestor instead,
    /// so that creating the first missing path component is noticed promptly without watching an
    /// unexpectedly large tree. Once the wanted directory exists, a later call upgrades to
    /// watching it directly and drops the temporary ancestor watch (unless some other wanted
    /// directory still needs that same ancestor as its own fallback).
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
        // Recomputed from scratch each pass: a directory that could not be watched last time may
        // well succeed now, and the retry happens below.
        self.uncovered_roots = false;
        let backend_error_pending = self.backend_error_pending.swap(false, Ordering::Relaxed);
        let overflow_pending = self.overflow_pending.swap(false, Ordering::Relaxed);
        if overflow_pending {
            emitter.emit_file_watch_events_overflowed();
        }
        if backend_error_pending || overflow_pending {
            // These flags are the durable signal that events were lost -- they are set precisely
            // when the bounded channel could not even carry the `Overflow` substitute, so the
            // pending wakeup may name only a few paths while a creation event went missing. Record
            // that a full glob pass is owed; a targeted pass would skip it (see `discover`).
            self.full_scan_required = true;
            if !self.forget_watches() {
                return false;
            }
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
        self.fallback_watches
            .retain(|path, _ancestor| wanted.contains_key(path));

        // Which ancestors will stand in for a missing directory, and under which mode, resolved
        // *before* any watch is installed. Deriving this from `fallback_watches` instead would read
        // an empty map right after a rebuild cleared it, so the direct loop would install the
        // narrower mode and the fallback would immediately ask for another rebuild -- measured at 4
        // rebuilds across 6 reconciliations, each of which drops events and owes a full glob pass.
        let mut fallback_modes: HashMap<PathBuf, WatchMode> = HashMap::new();
        for path in wanted.keys() {
            if fs::metadata(path).await.is_ok() {
                continue;
            }
            if let Some(ancestor) = find_existing_ancestor(path).await {
                let mode = fallback_watch_mode(path, &ancestor);
                fallback_modes
                    .entry(ancestor)
                    .and_modify(|existing| *existing = existing.merge(mode))
                    .or_insert(mode);
            }
        }

        // Watching a symlinked directory follows its current target. Replacing the symlink emits
        // the relevant event in the symlink's parent instead of the old target, so keep a
        // supplemental non-recursive watch on that parent. The helper also finds symlinks in a
        // prefix of a not-yet-existing directory, where watching `wanted.parent()` would itself
        // still follow the symlink and miss its replacement.
        let mut symlink_parent_watches = HashSet::new();
        for path in wanted.keys() {
            symlink_parent_watches.extend(find_symlink_parents(path).await);
        }
        self.symlink_parent_watches = symlink_parent_watches;

        // A watch opened through a symlink follows the target inode, so retargeting the symlink
        // leaves the old target registered while the logical path and watch mode look unchanged. The
        // canonical path does change, so comparing it catches this; rebuild on a change.
        //
        // Directory *identity* cannot serve the same purpose for a deleted-and-recreated directory:
        // on Linux the freed inode is routinely reused, and `dev`, `ino` and `created` can all be
        // byte-identical (verified on overlayfs, birth time included).
        let watched_paths: Vec<PathBuf> = self.watched_dirs.keys().cloned().collect();
        for watched in watched_paths {
            let Some(previous_canonical) = self.watched_dir_aliases.get(&watched) else {
                continue;
            };
            if *previous_canonical != canonicalize_watch_path(&watched).await {
                // Files created under the new target while the watch still followed the old one
                // were reported by nothing, so the caller owes a full glob pass. Set this before
                // the recursive call, which returns straight to the caller.
                self.full_scan_required = true;
                if !self.forget_watches() {
                    return false;
                }
                return Box::pin(self.resync_watches(&include_patterns, emitter)).await;
            }
        }

        // No *periodic* rebuild of the registrations, deliberately. It would catch a silently
        // dropped Windows registration, but each way of doing it costs more than that gap:
        //
        // - Re-`watch()`-ing in place leaks a handle per call: `notify` 8.2.0's Windows backend
        //   overwrites the `WatchState` without `stop_watch` (windows.rs:238).
        // - `forget_watches` drops events for every watched directory during teardown, so it owes an
        //   immediate full glob pass -- fingerprinting everything on the rebuild interval, which is
        //   what notify mode exists to avoid.
        // - `unwatch()` can block the `FileServer` task: several backends wait on their worker
        //   thread synchronously.
        //
        // The explicit signals remain, each forcing a rebuild and a full pass: `PathsRemoved` via
        // `forget_watch`, a backend error or overflow via the sticky flags above, a retargeted
        // symlink via the canonical comparison. `reconcile_interval` backstops the rest.

        // Directories we're not already watching under the mode we now want. A *mode change* on an
        // already-watched directory is deferred to a rebuild rather than re-`watch()`-ed in place:
        // `notify` 8.2.0's Windows backend inserts the new `WatchState` over the old one without
        // calling `stop_watch` (windows.rs:238), so the old directory handle and its pending read leak.
        for (path, mode) in &wanted {
            // Fallback ancestors are deliberately watched non-recursively, so they do not widen
            // a direct watch into a potentially very large subtree.
            let mode = if self.symlink_parent_watches.contains(path) {
                mode.merge(WatchMode::NonRecursive)
            } else {
                *mode
            };
            // A directly watched directory can also be a fallback ancestor; without merging, this
            // loop downgrades the mode that fallback needs.
            let mode = mode.merge(
                fallback_modes
                    .get(path)
                    .copied()
                    .unwrap_or(WatchMode::NonRecursive),
            );
            match self.watched_dirs.get(path) {
                Some(existing) if *existing == mode => continue,
                // Already watched, but under a different mode: defer to the rebuild below.
                Some(_) => {
                    self.mode_change_pending = true;
                    continue;
                }
                None => {}
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
                                self.watch_fallback_ancestor(
                                    path,
                                    ancestor,
                                    &fallback_modes,
                                    emitter,
                                )
                                .await
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
                        //
                        // Until one succeeds no event can arrive for anything under it, so the
                        // caller is told its coverage is incomplete rather than trusting events for
                        // a directory nothing is watching.
                        self.uncovered_roots = true;
                        warn!(message = "Failed to watch directory.", path = ?path, %error);
                        emitter.emit_file_watch_backend_error(&std::io::Error::other(
                            error.to_string(),
                        ));
                    }
                }
            }
        }

        // Install the parent registrations after direct/fallback watches have been reconciled, so
        // a parent that is also wanted or used as a fallback gets the strongest required mode.
        let symlink_parent_watches: Vec<PathBuf> =
            self.symlink_parent_watches.iter().cloned().collect();
        for parent in &symlink_parent_watches {
            // The fallback requirement is merged in, not just `wanted`: a parent that is also standing
            // in for a deeper missing include was installed `Recursive` by the loops above, and taking
            // only `wanted` here would overwrite that with `NonRecursive` -- leaving the missing levels
            // unobserved until the backstop.
            let mode = wanted
                .get(parent)
                .copied()
                .unwrap_or(WatchMode::NonRecursive)
                .merge(
                    fallback_modes
                        .get(parent)
                        .copied()
                        .unwrap_or(WatchMode::NonRecursive),
                );
            match self.watched_dirs.get(parent) {
                Some(existing) if *existing == mode => continue,
                // Changing the mode in place leaks a handle on Windows; defer to the rebuild.
                Some(_) => {
                    self.mode_change_pending = true;
                    continue;
                }
                None => {}
            }
            match self.watcher_mut().watch(parent, mode.mode()) {
                Ok(()) => {
                    trace!(
                        message = "Watching symlink parent for target replacement.",
                        path = ?parent,
                        ?mode,
                    );
                    self.watched_dirs.insert(parent.clone(), mode);
                }
                Err(error) => {
                    warn!(message = "Failed to watch symlink parent.", path = ?parent, %error);
                    emitter
                        .emit_file_watch_backend_error(&std::io::Error::other(error.to_string()));
                }
            }
        }

        // A directory stays watched if it's directly wanted, or if some still-wanted directory
        // depends on it as its fallback ancestor; anything else is stale. Do not call
        // `unwatch()` here: several notify backends synchronously wait for their worker thread,
        // so a broken backend could block the FileServer task. Rebuild the watcher instead; the
        // replacement gets the complete desired set below without any inline backend teardown.
        let ancestors_in_use: std::collections::HashSet<&PathBuf> =
            self.fallback_watches.values().collect();
        let has_stale_watches = self.watched_dirs.keys().any(|path| {
            !wanted.contains_key(path)
                && !ancestors_in_use.contains(path)
                && !self.symlink_parent_watches.contains(path)
        });
        if has_stale_watches {
            // Rebuilding drops every registration and re-adds them, so anything created in between
            // is reported by nothing. The caller owes a full glob pass, exactly as for a retargeted
            // symlink above -- set before the recursive call, which returns straight to the caller.
            self.full_scan_required = true;
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

        // Some watched directory needs a different mode than it has (set by any of the three loops
        // above). Rebuild rather than re-`watch()` it in place, which leaks its directory handle on
        // Windows; the rebuild loses events for its teardown window, so it also owes a full glob pass,
        // exactly like the stale-watch path above.
        if std::mem::take(&mut self.mode_change_pending) {
            self.full_scan_required = true;
            if !self.forget_watches() {
                return false;
            }
            return Box::pin(self.resync_watches(&include_patterns, emitter)).await;
        }

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

    /// Watch `ancestor` as a stand-in for the not-yet-existing `wanted` directory. A creation event
    /// under it triggers a resync, which then watches `wanted` directly. See [`fallback_watch_mode`]
    /// for why the mode depends on how many levels are missing.
    async fn watch_fallback_ancestor<E: FileSourceInternalEvents>(
        &mut self,
        wanted: &Path,
        ancestor: PathBuf,
        fallback_modes: &HashMap<PathBuf, WatchMode>,
        emitter: &E,
    ) {
        // The aggregate for this ancestor, not just what this root needs: registering the narrower
        // mode first makes a deeper root force a full rebuild moments later, and `wanted` is a
        // HashMap, so the rebuild can repeat the same order and churn.
        let watch_mode = fallback_modes
            .get(&ancestor)
            .copied()
            .unwrap_or_else(|| fallback_watch_mode(wanted, &ancestor));
        let mode = watch_mode.mode();
        if let Some(existing) = self.watched_dirs.get(&ancestor) {
            // Some other wanted directory already caused us to watch this ancestor. Record the
            // dependency and flag the widening for the rebuild at the end of `resync_watches`, rather
            // than re-`watch()`-ing in place, which leaks a handle on Windows (windows.rs:238).
            if watch_mode == WatchMode::Recursive && *existing == WatchMode::NonRecursive {
                self.mode_change_pending = true;
            }
            self.fallback_watches.insert(wanted.to_path_buf(), ancestor);
            return;
        }
        match self.watcher_mut().watch(&ancestor, mode) {
            Ok(()) => {
                debug!(
                    message = "Configured directory does not exist yet; watching nearest existing ancestor instead.",
                    wanted = ?wanted,
                    ancestor = ?ancestor,
                    mode = ?watch_mode,
                );
                self.watched_dirs.insert(ancestor.clone(), watch_mode);
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
    /// The old watcher is dropped on a detached thread without awaiting it, since `notify`'s `Drop`
    /// can block or panic on a wedged backend. A bounded compromise, not a fix: the two can briefly
    /// coexist under fd pressure, and there is no way to know when the old backend's thread exits.
    /// Detached teardowns are capped; past the cap a background reaper holds the old watcher and
    /// this returns `false`, and if that queue is also full the watcher is leaked rather than risk
    /// blocking the `FileServer` task.
    ///
    /// Each generation gets its own `backend_error_pending` flag: the old callback can still fire
    /// after the new watcher is installed, and a shared flag would condemn the healthy one.
    ///
    /// `false` means notify discovery must be abandoned for polling.
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
                self.symlink_parent_watches.clear();
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
    /// Use this only when the backend registration is already known to be invalid; removal/rename
    /// handling uses `forget_watches` instead so a watch that follows a moved inode is detached.
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

    /// Whether a full glob/fingerprint pass is owed, without consuming the demand. Used by the main
    /// loop to schedule that pass immediately; `take_full_scan_required` then consumes it inside
    /// `discover`.
    pub fn full_scan_required(&self) -> bool {
        self.full_scan_required
    }

    /// Whether a full glob/fingerprint pass is owed because registrations were rebuilt after a
    /// lost-event signal, clearing the flag. A targeted notify pass only fingerprints the paths an
    /// event named, so it cannot discover a file whose creation event was among the lost ones.
    pub fn take_full_scan_required(&mut self) -> bool {
        std::mem::take(&mut self.full_scan_required)
    }

    /// Whether some configured directory is not being watched, so events do not cover the whole
    /// configuration and reconciliation must keep to the polling cadence until one does.
    pub fn has_uncovered_roots(&self) -> bool {
        self.uncovered_roots
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

/// Return the parent directory of every symlink component in `path`. Checking the components
/// individually matters for a path whose final directory does not exist yet: its canonical form can
/// preserve the unresolved suffix, but the parent of that suffix would still be reached through the
/// symlink and would not observe replacement of the symlink itself.
///
/// Every one, not just the first: in `/a/link1/link2/*.log` each link can be retargeted
/// independently, and a watch on `/a` alone sees nothing when `link2` is replaced. The walk keeps
/// the logical spelling rather than canonicalizing as it goes, since `symlink_metadata` resolves
/// every component but the last -- so a nested link is still reported as a link -- and the watch
/// belongs on the path the configuration named.
async fn find_symlink_parents(path: &Path) -> Vec<PathBuf> {
    let mut parents = Vec::new();
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component.as_os_str());
        let metadata = match fs::symlink_metadata(&prefix).await {
            Ok(metadata) => metadata,
            Err(_) => break,
        };
        if metadata.file_type().is_symlink() {
            parents.push(
                prefix
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from(".")),
            );
        }
    }
    parents
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
            // A bare relative filename (`foo.log`) has `parent() == Some("")`, not `None`, so
            // filtering on emptiness is required as well: asking notify to watch "" registers
            // nothing, leaving the pattern with no directory watch at all.
            let dir = literal_prefix
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
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

pub(crate) fn contains_glob_metachar(component: &str) -> bool {
    component.contains(['*', '?', '[', '{'])
}

/// The mode a fallback watch on `ancestor` needs in order to see `wanted` appear.
///
/// Recursive beyond one missing level: a non-recursive watch reports only immediate children, so
/// `mkdir -p <ancestor>/a/b` reports `a` alone and a file written into `b` waits for the backstop. At
/// exactly one level the immediate child *is* `wanted`, where recursing would mean watching all of,
/// say, `/var/log`.
fn fallback_watch_mode(wanted: &Path, ancestor: &Path) -> WatchMode {
    let missing_levels = wanted
        .strip_prefix(ancestor)
        .map_or(1, |remainder| remainder.components().count());
    if missing_levels > 1 {
        WatchMode::Recursive
    } else {
        WatchMode::NonRecursive
    }
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
    fn bare_relative_literal_watches_the_current_directory() {
        // Regression test for a bug found in review: `Path::new("foo.log").parent()` is
        // `Some("")`, not `None`, so the `unwrap_or_else(|| ".")` fallback never fired and notify
        // was asked to watch an empty path -- registering nothing, and leaving the pattern with no
        // directory watch, so new or idle files waited for the reconciliation backstop.
        let dirs = compute_watch_directories(&[PathBuf::from("foo.log")]);
        assert_eq!(dirs.len(), 1);
        let (path, mode) = dirs.into_iter().next().unwrap();
        assert_eq!(
            path,
            PathBuf::from("."),
            "a bare relative filename must watch the current directory, not an empty path"
        );
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

    /// Regression test for a bug found in review: when an include names two or more missing
    /// directories (`<root>/a/b/*.log`), the fallback watch sat non-recursively on the nearest
    /// existing ancestor. Creating `a` was observed, but creating `b` *inside* it was not -- so files
    /// there stayed undiscovered until the reconciliation backstop, and a short-lived file could be
    /// missed entirely.
    #[tokio::test]
    async fn a_fallback_watch_observes_a_nested_directory_appearing() {
        let root = tempfile::tempdir().unwrap();
        // Two missing levels: the fallback lands on `root`, two levels above the wanted directory.
        let wanted = root.path().join("first").join("second");
        let pattern = wanted.join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();
        assert!(
            discovery.is_watched_dir(root.path()),
            "test setup requires the fallback to land on the existing root"
        );

        // Create *both* levels at once, the way `mkdir -p` in a deployment would. The fallback sits
        // on `root` non-recursively, so it reports `first` -- but the file landing in `second` is the
        // event that matters, and nothing watches `first` yet to report it.
        std::fs::create_dir_all(&wanted).unwrap();
        std::fs::write(wanted.join("app.log"), b"line\n").unwrap();

        // The nested creation must be observable: either the watch is recursive, or the event for
        // `first` arrives so a resync can extend the watch downwards. Drain what the backend has.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut saw_first = false;
        while std::time::Instant::now() < deadline && !saw_first {
            while let Ok(message) = discovery.receiver.try_recv() {
                let paths = match &message {
                    NotifyMessage::PathsChanged(paths)
                    | NotifyMessage::PathsCreated(paths)
                    | NotifyMessage::PathsRemoved(paths) => paths.clone(),
                    // A coarse wakeup also means "go look", which is enough for this test.
                    NotifyMessage::Overflow | NotifyMessage::BackendError(_) => {
                        saw_first = true;
                        break;
                    }
                };
                // The log file itself, or the directory holding it -- either tells the caller to
                // look inside `second`. Seeing only `first` does not.
                if paths
                    .iter()
                    .any(|path| path.ends_with("app.log") || path.ends_with("second"))
                {
                    saw_first = true;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(
            saw_first,
            "creating the intermediate directory must be observed, or the wanted directory is \
             never watched before the reconciliation backstop"
        );

        // The deeper level already exists (the setup created both), so a resync must now watch it
        // directly rather than leaving the fallback in place.
        assert!(discovery.resync_watches(&[pattern], &NoopEmitter).await);
        assert!(
            discovery.is_watched_dir(&wanted),
            "once it exists, the wanted directory itself must be watched"
        );
    }

    /// Widening a fallback ancestor must not re-`watch()` it in place: `notify` 8.2.0's Windows
    /// backend overwrites the `WatchState` without `stop_watch`, leaking the directory handle -- the
    /// very leak this branch exists to fix. The upgrade goes through a rebuild instead, which also
    /// owes a full glob pass for its teardown window.
    #[tokio::test]
    async fn widening_a_fallback_ancestor_rebuilds_instead_of_rewatching() {
        let root = tempfile::tempdir().unwrap();
        // A directly watched root, plus a deeper missing include that needs it recursive.
        let direct = root.path().join("*.log");
        let nested = root.path().join("first").join("second").join("*.log");

        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&direct), &NoopEmitter)
            .await
            .unwrap();
        assert_eq!(
            discovery.watched_dirs.get(root.path()),
            Some(&WatchMode::NonRecursive),
            "the direct include alone needs only a non-recursive watch"
        );
        discovery.take_full_scan_required();

        // Adding the nested include upgrades the shared ancestor.
        assert!(
            discovery
                .resync_watches(&[direct, nested], &NoopEmitter)
                .await
        );
        assert_eq!(
            discovery.watched_dirs.get(root.path()),
            Some(&WatchMode::Recursive),
            "the ancestor must end up recursive so the missing levels are observed"
        );
        assert!(
            discovery.full_scan_required(),
            "the rebuild drops events, so a full glob pass is owed -- and its presence is what \
             shows the upgrade went through a rebuild rather than an in-place re-watch"
        );
        assert!(
            !discovery.mode_change_pending,
            "the pending flag must be consumed, or every later resync rebuilds again"
        );
    }

    /// The same leak applies to a *directly* watched directory whose mode changes, which the main
    /// reconciliation loop used to re-`watch()` in place.
    #[tokio::test]
    async fn changing_a_direct_watch_mode_rebuilds_instead_of_rewatching() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("deep");
        std::fs::create_dir(&nested).unwrap();

        // `<root>/*.log` alone: non-recursive on `root`.
        let shallow = root.path().join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&shallow), &NoopEmitter)
            .await
            .unwrap();
        assert_eq!(
            discovery.watched_dirs.get(root.path()),
            Some(&WatchMode::NonRecursive)
        );
        discovery.take_full_scan_required();

        // Adding `<root>/**/*.log` needs the same directory recursive.
        let recursive = root.path().join("**").join("*.log");
        assert!(
            discovery
                .resync_watches(&[shallow, recursive], &NoopEmitter)
                .await
        );
        assert_eq!(
            discovery.watched_dirs.get(root.path()),
            Some(&WatchMode::Recursive),
            "the mode must end up widened"
        );
        assert!(
            discovery.full_scan_required(),
            "the widening must go through a rebuild, which owes a full glob pass"
        );
    }

    /// A directly watched directory that is also a fallback ancestor must keep the recursive mode the
    /// fallback needs; the reconciliation loop used to downgrade it, depending on `HashMap` order.
    #[tokio::test]
    async fn a_shared_ancestor_settles_without_rebuilding_forever() {
        // A direct include and a deeper missing one share `root`. The direct loop installs
        // non-recursive, the fallback then wants recursive and asks for a rebuild -- which clears
        // `fallback_watches`, so the next attempt repeats the same order. Left unguarded this never
        // settles, hanging `NotifyDiscovery::new`.
        let root = tempfile::tempdir().unwrap();
        let patterns = [
            root.path().join("*.log"),
            root.path().join("first").join("second").join("*.log"),
        ];

        let mut discovery = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            NotifyDiscovery::new(&patterns, &NoopEmitter),
        )
        .await
        .expect("construction must settle rather than rebuild forever")
        .unwrap();

        assert_eq!(
            discovery.watched_dirs.get(root.path()),
            Some(&WatchMode::Recursive),
            "the shared ancestor must end up recursive for the missing levels"
        );

        // Repeated resyncs against a *populated* `watched_dirs`, where the "already watched under a
        // different mode" branch is live and a disagreement would rebuild on every pass.
        for pass in 0..5 {
            assert!(
                tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    discovery.resync_watches(&patterns, &NoopEmitter),
                )
                .await
                .unwrap_or_else(|_| panic!("resync {pass} never settled")),
            );
            assert_eq!(
                discovery.watched_dirs.get(root.path()),
                Some(&WatchMode::Recursive),
                "pass {pass} downgraded the shared ancestor"
            );
        }
    }

    #[tokio::test]
    async fn a_resync_keeps_a_fallback_upgraded_ancestor_recursive() {
        let root = tempfile::tempdir().unwrap();
        // One include watches `root` directly; the other needs two missing levels under it.
        let direct = root.path().join("*.log");
        let nested = root.path().join("first").join("second").join("*.log");
        let patterns = [direct, nested];

        let mut discovery = NotifyDiscovery::new(&patterns, &NoopEmitter).await.unwrap();
        assert_eq!(
            discovery.watched_dirs.get(root.path()),
            Some(&WatchMode::Recursive),
            "the shared ancestor must be recursive to see the missing nested levels appear"
        );

        for _ in 0..3 {
            assert!(discovery.resync_watches(&patterns, &NoopEmitter).await);
            assert_eq!(
                discovery.watched_dirs.get(root.path()),
                Some(&WatchMode::Recursive),
                "a resync must not downgrade an ancestor a fallback still needs recursive"
            );
        }
    }

    /// Rebuilding registrations because a watch went stale drops and re-adds every watch, so events
    /// in between are reported by nothing. That owes the caller a full glob pass, like a retargeted
    /// symlink does; without it a targeted pass could return having checked only its named paths.
    #[tokio::test]
    async fn a_stale_watch_rebuild_requires_a_full_scan() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();

        let mut discovery = NotifyDiscovery::new(&[first.join("*.log")], &NoopEmitter)
            .await
            .unwrap();
        assert!(discovery.is_watched_dir(&first));
        // Consume the flag the initial build may have set, so the assertion below is about the rebuild.
        discovery.take_full_scan_required();

        // The include no longer covers `first`, so its watch is stale and triggers a rebuild.
        assert!(
            discovery
                .resync_watches(&[second.join("*.log")], &NoopEmitter)
                .await
        );
        assert!(
            discovery.is_watched_dir(&second),
            "the new directory must be watched after the rebuild"
        );
        assert!(
            discovery.full_scan_required(),
            "a rebuild loses events, so the caller must be told to run a full glob pass"
        );
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
    async fn lost_event_signal_leaves_a_full_scan_owed_for_the_caller() {
        // Regression test for a bug found in review: re-registering a watch is not enough when
        // events were actually lost -- files created in that window were reported by nothing, so
        // only a glob pass finds them. The demand must be visible to the caller *without* being
        // consumed, so the main loop can schedule that pass at once rather than leaving it to
        // `reconcile_interval` (300s by default), long enough to miss a short-lived file.
        //
        // Note what does *not* demand it: the periodic re-registration on `WATCH_VERIFY_INTERVAL`
        // (see `periodic_verification_reregisters_every_watched_directory`). Only a genuine
        // lost-event signal does, which is what the sticky overflow flag represents here.
        let root = tempfile::tempdir().unwrap();
        let watched = root.path().join("logs");
        std::fs::create_dir(&watched).unwrap();
        let pattern = watched.join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();
        assert!(
            !discovery.take_full_scan_required(),
            "a clean startup owes no full scan"
        );

        discovery.overflow_pending.store(true, Ordering::Relaxed);
        assert!(
            discovery
                .resync_watches(std::slice::from_ref(&pattern), &NoopEmitter)
                .await
        );

        assert!(
            discovery.full_scan_required(),
            "a lost-event signal must leave a full scan owed"
        );
        assert!(
            discovery.full_scan_required(),
            "the non-consuming getter must not clear the demand -- `discover` consumes it"
        );
        assert!(
            discovery.take_full_scan_required(),
            "`discover` must still be able to consume the demand"
        );
        assert!(
            !discovery.full_scan_required(),
            "the demand is cleared once consumed"
        );
    }

    #[tokio::test]
    async fn sticky_recovery_flag_demands_a_full_scan() {
        // Regression test for a bug found in review: the sticky overflow/backend-error flags are
        // consumed inside `resync_watches` and nothing was reported to the caller. `discover` would
        // then still take its targeted notify path, fingerprinting only the paths an event named --
        // but those flags are raised exactly when the channel was too full to carry even the coarse
        // `Overflow` substitute, so an unrelated file-creation event may have been among the lost
        // ones. Only a full glob pass can discover a file that nothing named.
        let dir = tempfile::tempdir().unwrap();
        let pattern = dir.path().join("*.log");
        let mut discovery = NotifyDiscovery::new(std::slice::from_ref(&pattern), &NoopEmitter)
            .await
            .unwrap();

        // `new` performs a resync of its own; no recovery flag was pending during it.
        assert!(
            !discovery.take_full_scan_required(),
            "an ordinary resync must not demand a full scan"
        );

        discovery.overflow_pending.store(true, Ordering::Relaxed);
        assert!(
            discovery
                .resync_watches(std::slice::from_ref(&pattern), &NoopEmitter)
                .await
        );
        assert!(
            discovery.take_full_scan_required(),
            "consuming a sticky recovery flag must demand a full glob pass"
        );
        assert!(
            !discovery.take_full_scan_required(),
            "the demand must be cleared once taken, so later passes stay targeted"
        );

        discovery
            .backend_error_pending
            .store(true, Ordering::Relaxed);
        assert!(
            discovery
                .resync_watches(std::slice::from_ref(&pattern), &NoopEmitter)
                .await
        );
        assert!(
            discovery.take_full_scan_required(),
            "a sticky backend error must demand a full glob pass too"
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

        assert!(
            discovery.symlink_parent_watches.contains(root.path()),
            "retargetable symlinks must keep their parent directory watched"
        );

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

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_parent_watch_reports_retargeting() {
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

        let message = tokio::time::timeout(std::time::Duration::from_secs(5), discovery.recv())
            .await
            .expect("replacing a watched symlink must wake discovery through its parent")
            .expect("notify channel must stay open");

        assert!(matches!(
            message,
            NotifyMessage::PathsCreated(paths)
                | NotifyMessage::PathsChanged(paths)
                | NotifyMessage::PathsRemoved(paths)
                if !paths.is_empty()
        ));
    }

    /// Regression test for a bug found in review: only the first symlink component was watched, so
    /// in `/a/link1/link2/*.log` retargeting `link2` was observed by nothing.
    #[cfg(unix)]
    #[tokio::test]
    async fn every_symlink_parent_in_a_nested_path_is_watched() {
        let root = tempfile::tempdir().unwrap();
        let real_outer = root.path().join("real_outer");
        let real_inner = real_outer.join("real_inner");
        std::fs::create_dir_all(&real_inner).unwrap();

        // /root/link1 -> /root/real_outer, and /root/real_outer/link2 -> real_inner
        let link1 = root.path().join("link1");
        std::os::unix::fs::symlink(&real_outer, &link1).unwrap();
        let link2 = real_outer.join("link2");
        std::os::unix::fs::symlink(&real_inner, &link2).unwrap();

        let parents = find_symlink_parents(&link1.join("link2")).await;
        assert!(
            parents.contains(&root.path().to_path_buf()),
            "the parent of the outer link must be watched, got {parents:?}"
        );
        assert!(
            parents.contains(&link1),
            "the parent of the inner link must be watched too, got {parents:?}"
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
            Some(&WatchMode::NonRecursive),
            "the nearest existing ancestor should be watched non-recursively as a stand-in"
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
    async fn fallback_ancestor_that_is_also_directly_wanted_stays_non_recursive() {
        // A missing nested root only needs its nearest existing ancestor to report creation of
        // the missing first component. It must not upgrade a directly-wanted ancestor to a
        // recursive watch, even when both relationships use the same directory.
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
            Some(&WatchMode::NonRecursive),
            "root must stay NonRecursive: the fallback watch must not widen the direct watch"
        );

        // Re-running resync_watches must not change that bounded mode either.
        assert!(
            discovery
                .resync_watches(&[missing_pattern, direct_pattern], &NoopEmitter)
                .await
        );
        assert_eq!(
            discovery.watched_dirs.get(root.path()),
            Some(&WatchMode::NonRecursive),
            "root must remain NonRecursive across repeated resync_watches calls"
        );
    }
}
