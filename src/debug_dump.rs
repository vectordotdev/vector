/// SIGTERM debug dump: on SIGTERM, dump diagnostic info to stderr before normal shutdown.
///
/// This avoids signal-safety issues by using an AtomicBool flag set in the signal handler
/// and a watcher async task that detects the flag and performs the dump from async context.
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

static SIGTERM_RECEIVED: AtomicBool = AtomicBool::new(false);

/// Called from the signal handler or signal-receiving code when SIGTERM is detected.
/// Signal-safe: only sets an atomic flag.
pub fn mark_sigterm() {
    SIGTERM_RECEIVED.store(true, Ordering::SeqCst);
}

/// Spawn as a Tokio task. Polls for the SIGTERM flag and dumps debug info when triggered.
/// Returns after the dump is complete.
pub async fn dump_debug_info_on_sigterm() {
    loop {
        if SIGTERM_RECEIVED.load(Ordering::SeqCst) {
            do_dump();
            return;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
}

fn do_dump() {
    let stderr = std::io::stderr();
    let mut err = stderr.lock();

    let _ = writeln!(err, "");
    let _ = writeln!(err, "=== VECTOR SIGTERM DEBUG DUMP ===");
    let _ = writeln!(
        err,
        "timestamp: {:?}",
        std::time::SystemTime::now()
    );
    let _ = writeln!(err, "pid: {}", std::process::id());

    // /proc/self/status: threads, VmRSS, VmPeak, etc.
    match std::fs::read_to_string("/proc/self/status") {
        Ok(status) => {
            let _ = writeln!(err, "--- /proc/self/status ---");
            let _ = writeln!(err, "{}", status);
        }
        Err(e) => {
            let _ = writeln!(err, "[/proc/self/status unavailable: {}]", e);
        }
    }

    // Open file descriptors
    match std::fs::read_dir("/proc/self/fd") {
        Ok(entries) => {
            let fds: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            let _ = writeln!(err, "--- Open FDs: {} ---", fds.len());
            for fd in fds.iter().take(200) {
                if let Ok(target) = std::fs::read_link(fd.path()) {
                    let _ = writeln!(err, "  {:?} -> {:?}", fd.file_name(), target);
                }
            }
        }
        Err(e) => {
            let _ = writeln!(err, "[/proc/self/fd unavailable: {}]", e);
        }
    }

    // Per-thread summary via /proc/self/task.
    // We emit one compact line per thread (tid, state, wchan, syscall-nr) and then a
    // wchan frequency table.  The full /proc/self/task/{tid}/status is ~30 lines of
    // mostly-redundant process-level fields and is not useful for async stall diagnosis —
    // the async task tree below is the high-value signal.
    match std::fs::read_dir("/proc/self/task") {
        Ok(tasks) => {
            let task_list: Vec<_> = tasks.filter_map(|t| t.ok()).collect();
            let _ = writeln!(err, "--- Threads: {} ---", task_list.len());
            let mut wchan_counts: std::collections::BTreeMap<String, usize> =
                std::collections::BTreeMap::new();
            for task in &task_list {
                let tid = task.file_name();
                let tids = tid.to_string_lossy();

                // State: single character from /proc/self/task/{tid}/status (line "State: S ...")
                let state = std::fs::read_to_string(
                    format!("/proc/self/task/{}/status", tids))
                    .ok()
                    .and_then(|s| {
                        s.lines()
                            .find(|l| l.starts_with("State:"))
                            .and_then(|l| l.split_whitespace().nth(1))
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| "?".to_string());

                let wchan = std::fs::read_to_string(
                    format!("/proc/self/task/{}/wchan", tids))
                    .unwrap_or_else(|_| "?".to_string());
                let wchan = wchan.trim().to_string();

                // syscall: first token (syscall number or "-1" if not in a syscall)
                let syscall = std::fs::read_to_string(
                    format!("/proc/self/task/{}/syscall", tids))
                    .unwrap_or_else(|_| "?".to_string());
                let syscall_nr = syscall.split_whitespace().next()
                    .unwrap_or("?").to_string();

                let _ = writeln!(err, "  tid={} state={} wchan={} syscall_nr={}",
                    tids, state, wchan, syscall_nr);
                *wchan_counts.entry(wchan).or_insert(0) += 1;
            }
            let _ = writeln!(err, "--- wchan summary ---");
            for (wchan, count) in &wchan_counts {
                let _ = writeln!(err, "  {:>4}x  {}", count, wchan);
            }
        }
        Err(e) => {
            let _ = writeln!(err, "[/proc/self/task unavailable: {}]", e);
        }
    }

    // Backtrace of the current (dumper) thread
    let _ = writeln!(err, "--- Current thread backtrace ---");
    let bt = backtrace::Backtrace::new();
    let _ = writeln!(err, "{:?}", bt);

    // Async task dump: shows every suspended #[async_backtrace::framed] future and its
    // current await chain. Use wait_for_running_tasks=false so we capture immediately —
    // suspended tasks (the ones we care about in a deadlock) appear unconditionally.
    let _ = writeln!(err, "--- Async task dump (async-backtrace) ---");
    let tree = async_backtrace::taskdump_tree(false);
    if tree.is_empty() {
        let _ = writeln!(err, "[no framed tasks suspended]");
    } else {
        let _ = writeln!(err, "{}", tree);
    }

    // Tokio runtime metrics (requires --cfg tokio_unstable at build time)
    #[cfg(tokio_unstable)]
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let metrics = handle.metrics();
            let _ = writeln!(err, "--- Tokio Runtime Metrics ---");
            let _ = writeln!(err, "  num_workers: {}", metrics.num_workers());
            let _ = writeln!(err, "  num_alive_tasks: {}", metrics.num_alive_tasks());
            let _ = writeln!(err, "  global_queue_depth: {}", metrics.global_queue_depth());
            for i in 0..metrics.num_workers() {
                let _ = writeln!(
                    err,
                    "  worker[{}] queue_depth={} total_park={} total_noop={} total_steal={} total_polls={}",
                    i,
                    metrics.worker_local_queue_depth(i),
                    metrics.worker_park_count(i),
                    metrics.worker_noop_count(i),
                    metrics.worker_steal_count(i),
                    metrics.worker_poll_count(i),
                );
            }
        }
    }

    let _ = writeln!(err, "=== END VECTOR SIGTERM DEBUG DUMP ===");
    let _ = writeln!(err, "");
    let _ = err.flush();
}
