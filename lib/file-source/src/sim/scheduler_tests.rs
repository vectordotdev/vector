use std::{collections::HashMap, path::PathBuf, time::Duration};

use bytes::Bytes;
use file_source_common::{
    Checkpointer, Clock, FingerprintStrategy, Fingerprinter, MonotonicInstant, ReadFrom,
};
use proptest::prelude::*;

use crate::{
    file_server::{FileServer, FileServerState, Line},
    scheduler::{
        AdaptiveEventScheduler, AdaptiveSchedulerConfig, FsEventSource, LegacyPollingScheduler,
        NoopFsEventSource, ReadScheduler,
    },
};

use super::{
    action::{arb_actions, arb_dst_config},
    clock::SimClock,
    emitter::RecordingEmitter,
    fs::{EventDelivery, SimFileSystem, SimPathsProvider},
    runner::run_dst_scenario_adaptive,
};

#[derive(Debug, Clone, Copy)]
struct WorkloadMetrics {
    lines_delivered: usize,
    total_reads: u64,
    eof_reads: u64,
    p50_latency_ms: u64,
    p99_latency_ms: u64,
    max_latency_ms: u64,
}

fn summarize_latencies(mut latencies_ms: Vec<u64>) -> (u64, u64, u64) {
    if latencies_ms.is_empty() {
        return (0, 0, 0);
    }
    latencies_ms.sort_unstable();
    let n = latencies_ms.len();
    let p50 = latencies_ms[n / 2];
    let p99 = latencies_ms[(n * 99) / 100];
    let max = *latencies_ms.last().unwrap();
    (p50, p99, max)
}

fn make_bench_server(
    fs: &SimFileSystem,
    emitter: RecordingEmitter,
) -> FileServer<SimPathsProvider, RecordingEmitter> {
    FileServer {
        paths_provider: fs.paths_provider(&["/logs/*"], &[]),
        max_read_bytes: 4096,
        ignore_checkpoints: false,
        read_from: ReadFrom::Beginning,
        ignore_before: None,
        max_line_bytes: 1024,
        line_delimiter: Bytes::from_static(b"\n"),
        data_dir: PathBuf::from("/data"),
        glob_minimum_cooldown: Duration::from_secs(1),
        fingerprinter: Fingerprinter::new(FingerprintStrategy::DevInode, 1024, false),
        oldest_first: false,
        remove_after: None,
        emitter,
        rotate_wait: Duration::from_secs(300),
    }
}

/// Workload A: 1,000 idle files + 2 hot writers appending every 200ms over 30s simulated time.
async fn run_idle_heavy_workload<S: ReadScheduler, Ev: FsEventSource>(
    scheduler: S,
    make_ev: impl FnOnce(&SimFileSystem) -> Ev,
    delivery: EventDelivery,
) -> WorkloadMetrics {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    fs.set_event_delivery(delivery);
    let emitter = RecordingEmitter::new();

    let idle_files = 1_000;
    for i in 0..idle_files {
        fs.append_bytes(format!("/logs/idle_{i:04}.log"), b"seed\n");
    }
    fs.append_bytes("/logs/hot_0.log", b"hot0_init\n");
    fs.append_bytes("/logs/hot_1.log", b"hot1_init\n");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = make_bench_server(&fs, emitter);
    let ev = make_ev(&fs);
    let mut state = FileServerState::init_with_scheduler(
        server,
        &mut cp,
        fs.clone(),
        clock.clone(),
        scheduler,
        ev,
    )
    .await;

    // Drain initial seed lines.
    let mut lines = Vec::new();
    let _ = state.step(&mut lines).await;
    assert_eq!(lines.len(), idle_files + 2);
    lines.clear();

    // Reset syscall counters after startup drain so we measure steady-state cost.
    fs.reset_stats();
    let mut write_times: HashMap<String, MonotonicInstant> = HashMap::new();
    let mut latencies_ms = Vec::new();

    // 150 ticks × 200ms = 30s simulated time.
    for tick in 0..150u64 {
        clock.advance(Duration::from_millis(200));
        let now = clock.now();
        for h in 0..2 {
            let tag = format!("h{h}_{tick}");
            write_times.insert(tag.clone(), now);
            fs.append_bytes(format!("/logs/hot_{h}.log"), format!("{tag}\n").as_bytes());
        }
        let mut batch = Vec::new();
        let _ = state.step(&mut batch).await;
        let read_at = clock.now();
        for line in batch {
            let text = String::from_utf8(line.text.to_vec()).unwrap();
            if let Some(written_at) = write_times.remove(&text) {
                let lat = u64::try_from(read_at.saturating_duration_since(written_at).as_millis())
                    .unwrap_or(u64::MAX);
                latencies_ms.push(lat);
            }
            lines.push(line);
        }
    }

    let fs_stats = fs.stats();
    let (p50, p99, max) = summarize_latencies(latencies_ms);
    WorkloadMetrics {
        lines_delivered: lines.len(),
        total_reads: fs_stats.reads,
        eof_reads: fs_stats.eof_reads,
        p50_latency_ms: p50,
        p99_latency_ms: p99,
        max_latency_ms: max,
    }
}

/// Workload B: 200 quiet files sit idle for 12s (triggering the legacy 10s idle cliff),
/// then each file receives a sporadic burst line while `FileServer` ticks every 100ms.
async fn run_sporadic_cliff_workload<S: ReadScheduler, Ev: FsEventSource>(
    scheduler: S,
    make_ev: impl FnOnce(&SimFileSystem) -> Ev,
    delivery: EventDelivery,
) -> WorkloadMetrics {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    fs.set_event_delivery(delivery);
    let emitter = RecordingEmitter::new();

    let file_count = 200;
    for i in 0..file_count {
        fs.append_bytes(format!("/logs/quiet_{i:03}.log"), b"init\n");
    }

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = make_bench_server(&fs, emitter);
    let ev = make_ev(&fs);
    let mut state = FileServerState::init_with_scheduler(
        server,
        &mut cp,
        fs.clone(),
        clock.clone(),
        scheduler,
        ev,
    )
    .await;

    let mut lines = Vec::new();
    let _ = state.step(&mut lines).await;
    assert_eq!(lines.len(), file_count);
    lines.clear();

    // Advance 11s and poll once so all files cross the legacy 10s idle cliff.
    clock.advance(Duration::from_secs(11));
    let _ = state.step(&mut lines).await;
    assert!(lines.is_empty());

    fs.reset_stats();
    let mut write_times: HashMap<String, MonotonicInstant> = HashMap::new();
    let mut latencies_ms = Vec::new();

    // Advance 1s, then append 1 sporadic line to each of the 200 quiet files.
    clock.advance(Duration::from_secs(1));
    let write_ts = clock.now();
    for i in 0..file_count {
        let tag = format!("burst_{i}");
        write_times.insert(tag.clone(), write_ts);
        fs.append_bytes(
            format!("/logs/quiet_{i:03}.log"),
            format!("{tag}\n").as_bytes(),
        );
    }

    // Poll every 100ms for up to 12s until all 200 burst lines are delivered.
    for _ in 0..120 {
        let mut batch: Vec<Line> = Vec::new();
        let _ = state.step(&mut batch).await;
        let read_at = clock.now();
        for line in batch {
            let text = String::from_utf8(line.text.to_vec()).unwrap();
            if let Some(written_at) = write_times.remove(&text) {
                let lat = u64::try_from(read_at.saturating_duration_since(written_at).as_millis())
                    .unwrap_or(u64::MAX);
                latencies_ms.push(lat);
            }
            lines.push(line);
        }
        if lines.len() == file_count {
            break;
        }
        clock.advance(Duration::from_millis(100));
    }

    let fs_stats = fs.stats();
    let (p50, p99, max) = summarize_latencies(latencies_ms);
    WorkloadMetrics {
        lines_delivered: lines.len(),
        total_reads: fs_stats.reads,
        eof_reads: fs_stats.eof_reads,
        p50_latency_ms: p50,
        p99_latency_ms: p99,
        max_latency_ms: max,
    }
}

#[allow(clippy::print_stderr)]
#[tokio::test]
async fn scheduler_scorecard_comparison() {
    // Workload A: 1,000 idle files + 2 hot writers
    let legacy_idle = run_idle_heavy_workload(
        LegacyPollingScheduler::default(),
        |_| NoopFsEventSource,
        EventDelivery::Normal,
    )
    .await;
    let adaptive_idle = run_idle_heavy_workload(
        AdaptiveEventScheduler::new(AdaptiveSchedulerConfig::default()),
        SimFileSystem::event_source,
        EventDelivery::Normal,
    )
    .await;

    // Workload B: 200 sporadic quiet files after 11s idle
    let legacy_sporadic = run_sporadic_cliff_workload(
        LegacyPollingScheduler::default(),
        |_| NoopFsEventSource,
        EventDelivery::Normal,
    )
    .await;
    let adaptive_sporadic = run_sporadic_cliff_workload(
        AdaptiveEventScheduler::new(AdaptiveSchedulerConfig::default()),
        SimFileSystem::event_source,
        EventDelivery::Normal,
    )
    .await;

    // Workload B with 100% dropped events (NFS fallback)
    let adaptive_nfs = run_sporadic_cliff_workload(
        AdaptiveEventScheduler::new(AdaptiveSchedulerConfig::default()),
        SimFileSystem::event_source,
        EventDelivery::DropAll,
    )
    .await;

    eprintln!("\n=== ReadScheduler Deterministic Simulation Scorecard ===");
    eprintln!(
        "Workload A (1000 idle + 2 hot, 30s):
  Legacy:   lines={}, reads={}, eof_reads={}, p50={}ms, p99={}ms, max={}ms
  Adaptive: lines={}, reads={}, eof_reads={}, p50={}ms, p99={}ms, max={}ms",
        legacy_idle.lines_delivered,
        legacy_idle.total_reads,
        legacy_idle.eof_reads,
        legacy_idle.p50_latency_ms,
        legacy_idle.p99_latency_ms,
        legacy_idle.max_latency_ms,
        adaptive_idle.lines_delivered,
        adaptive_idle.total_reads,
        adaptive_idle.eof_reads,
        adaptive_idle.p50_latency_ms,
        adaptive_idle.p99_latency_ms,
        adaptive_idle.max_latency_ms,
    );
    eprintln!(
        "Workload B (200 sporadic files after 11s idle):
  Legacy:       lines={}, reads={}, eof_reads={}, p50={}ms, p99={}ms, max={}ms
  Adaptive:     lines={}, reads={}, eof_reads={}, p50={}ms, p99={}ms, max={}ms
  Adaptive/NFS: lines={}, reads={}, eof_reads={}, p50={}ms, p99={}ms, max={}ms",
        legacy_sporadic.lines_delivered,
        legacy_sporadic.total_reads,
        legacy_sporadic.eof_reads,
        legacy_sporadic.p50_latency_ms,
        legacy_sporadic.p99_latency_ms,
        legacy_sporadic.max_latency_ms,
        adaptive_sporadic.lines_delivered,
        adaptive_sporadic.total_reads,
        adaptive_sporadic.eof_reads,
        adaptive_sporadic.p50_latency_ms,
        adaptive_sporadic.p99_latency_ms,
        adaptive_sporadic.max_latency_ms,
        adaptive_nfs.lines_delivered,
        adaptive_nfs.total_reads,
        adaptive_nfs.eof_reads,
        adaptive_nfs.p50_latency_ms,
        adaptive_nfs.p99_latency_ms,
        adaptive_nfs.max_latency_ms,
    );

    // Correctness: all hot & burst lines delivered in every configuration.
    assert_eq!(legacy_idle.lines_delivered, 300);
    assert_eq!(adaptive_idle.lines_delivered, 300);
    assert_eq!(legacy_sporadic.lines_delivered, 200);
    assert_eq!(adaptive_sporadic.lines_delivered, 200);
    assert_eq!(adaptive_nfs.lines_delivered, 200);

    // Quantitative efficiency win on Workload A: Adaptive executes > 3x fewer wasted EOF reads!
    assert!(
        adaptive_idle.eof_reads * 3 < legacy_idle.eof_reads,
        "expected >3x reduction in wasted EOF reads (legacy={}, adaptive={})",
        legacy_idle.eof_reads,
        adaptive_idle.eof_reads
    );

    // Quantitative latency win on Workload B: Legacy hits the ~9s idle cliff;
    // Adaptive delivers immediately (0ms sim latency), and even NFS fallback beats legacy!
    assert!(legacy_sporadic.max_latency_ms >= 9_000);
    assert_eq!(adaptive_sporadic.max_latency_ms, 0);
    assert!(adaptive_nfs.max_latency_ms <= 10_000);
}

#[tokio::test]
async fn adaptive_scheduler_respects_remove_after_when_idle() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/ephemeral.log", b"done\n");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let mut server = make_bench_server(&fs, emitter.clone());
    // `remove_after` (2s) is shorter than `fallback_max` (10s).
    server.remove_after = Some(Duration::from_secs(2));
    let ev = fs.event_source();
    let mut state = FileServerState::init_with_scheduler(
        server,
        &mut cp,
        fs.clone(),
        clock.clone(),
        AdaptiveEventScheduler::new(AdaptiveSchedulerConfig::default()),
        ev,
    )
    .await;

    let mut lines = Vec::new();
    let _ = state.step(&mut lines).await;
    assert_eq!(lines.len(), 1);
    assert!(fs.exists("/logs/ephemeral.log"));

    // Push the file well into fallback backoff, then verify `remove_after = 2s`
    // fires promptly at +2s without waiting for the 10s fallback timer.
    clock.advance(Duration::from_secs(2));
    let _ = state.step(&mut lines).await;
    assert!(
        !fs.exists("/logs/ephemeral.log"),
        "remove_after should delete idle file promptly even if scheduler didn't poll it"
    );
    assert_eq!(state.open_file_count(), 0);
    assert_eq!(fs.active_open_handles(), 0);
    emitter.with_events(|e| assert_eq!(e.deleted.len(), 1));
}

#[tokio::test]
async fn adaptive_scheduler_recovers_from_event_overflow() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    for i in 0..50 {
        fs.append_bytes(format!("/logs/f_{i:02}.log"), b"init\n");
    }

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = make_bench_server(&fs, emitter);
    let ev = fs.event_source();
    let mut state = FileServerState::init_with_scheduler(
        server,
        &mut cp,
        fs.clone(),
        clock.clone(),
        AdaptiveEventScheduler::new(AdaptiveSchedulerConfig::default()),
        ev,
    )
    .await;

    let mut lines = Vec::new();
    let _ = state.step(&mut lines).await;
    assert_eq!(lines.len(), 50);

    // Drop normal events, write to all 50 files, then inject `IN_Q_OVERFLOW`.
    fs.set_event_delivery(EventDelivery::DropAll);
    for i in 0..50 {
        fs.append_bytes(format!("/logs/f_{i:02}.log"), b"after_overflow\n");
    }
    fs.inject_event_overflow();

    // A single step after `Overflow` immediately reads all 50 files without waiting for fallback!
    let _ = state.step(&mut lines).await;
    assert_eq!(lines.len(), 100);
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Differential DST: `AdaptiveEventScheduler` with healthy FS events satisfies
    /// the exact same multi-file invariants across randomized configs & actions.
    #[test]
    fn dst_adaptive_scheduler_normal_events(
        config in arb_dst_config(),
        actions in arb_actions(120),
    ) {
        run_dst_scenario_adaptive(config, &actions, EventDelivery::Normal)?;
    }

    /// Differential DST: `AdaptiveEventScheduler` with 100% dropped FS events (NFS)
    /// still satisfies all multi-file invariants via fallback polling.
    #[test]
    fn dst_adaptive_scheduler_nfs_dropped_events(
        config in arb_dst_config(),
        actions in arb_actions(120),
    ) {
        run_dst_scenario_adaptive(config, &actions, EventDelivery::DropAll)?;
    }
}
