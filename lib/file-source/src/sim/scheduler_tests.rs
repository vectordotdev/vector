use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::Duration,
};

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
    runner::{
        run_dst_scenario_adaptive, run_dst_scenario_adaptive_outcome, run_dst_scenario_outcome,
    },
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

/// Workload C: 200 files where each file writes 1 line every 15s over 60s
/// (4 waves of sporadic writes), measuring both wasted EOF reads and p50/p99/max latency.
async fn run_continuous_sporadic_workload<S: ReadScheduler, Ev: FsEventSource>(
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
        fs.append_bytes(format!("/logs/sporadic_{i:03}.log"), b"init\n");
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

    fs.reset_stats();
    let mut write_times: HashMap<String, MonotonicInstant> = HashMap::new();
    let mut latencies_ms = Vec::new();

    // 300 ticks × 200ms = 60s simulated time; write a wave to all 200 files at t=15s, 30s, 45s.
    for tick in 1..=300u64 {
        clock.advance(Duration::from_millis(200));
        let now = clock.now();
        if tick % 75 == 0 && tick < 300 {
            let wave = tick / 75;
            for i in 0..file_count {
                let tag = format!("w{wave}_f{i}");
                write_times.insert(tag.clone(), now);
                fs.append_bytes(
                    format!("/logs/sporadic_{i:03}.log"),
                    format!("{tag}\n").as_bytes(),
                );
            }
        }
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

#[allow(clippy::cast_precision_loss)]
fn pct_reduction(legacy: u64, adaptive: u64) -> f64 {
    if legacy == 0 {
        return 0.0;
    }
    ((legacy as f64 - adaptive as f64) / legacy as f64) * 100.0
}

#[allow(clippy::print_stderr)]
fn print_scorecard_row(
    workload: &str,
    scheduler: &str,
    m: WorkloadMetrics,
    baseline_eof_reads: Option<u64>,
) {
    let reduction = baseline_eof_reads.map_or_else(
        || "—".to_string(),
        |base| format!("-{:.1}%", pct_reduction(base, m.eof_reads)),
    );
    eprintln!(
        "| {workload} | {scheduler} | {} | {} | {} | {reduction} | {}ms | {}ms | {}ms |",
        m.lines_delivered,
        m.total_reads,
        m.eof_reads,
        m.p50_latency_ms,
        m.p99_latency_ms,
        m.max_latency_ms,
    );
}

#[allow(clippy::print_stderr)]
fn print_scorecard(rows: &[(&str, &str, WorkloadMetrics, Option<u64>)]) {
    eprintln!("\n=== ReadScheduler Deterministic Simulation Scorecard ===");
    eprintln!(
        "| Workload | Scheduler | Lines | Total Reads | Wasted EOF Reads | EOF Read Reduction | p50 Latency | p99 Latency | Max Latency |"
    );
    eprintln!("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for &(workload, scheduler, m, baseline_eof_reads) in rows {
        print_scorecard_row(workload, scheduler, m, baseline_eof_reads);
    }
}

fn adaptive_default() -> AdaptiveEventScheduler {
    AdaptiveEventScheduler::new(AdaptiveSchedulerConfig::default())
}

#[tokio::test]
async fn scheduler_scorecard_comparison() {
    let legacy_idle = run_idle_heavy_workload(
        LegacyPollingScheduler::default(),
        |_| NoopFsEventSource,
        EventDelivery::Normal,
    )
    .await;
    let adaptive_idle = run_idle_heavy_workload(
        adaptive_default(),
        SimFileSystem::event_source,
        EventDelivery::Normal,
    )
    .await;

    let legacy_sporadic = run_sporadic_cliff_workload(
        LegacyPollingScheduler::default(),
        |_| NoopFsEventSource,
        EventDelivery::Normal,
    )
    .await;
    let adaptive_sporadic = run_sporadic_cliff_workload(
        adaptive_default(),
        SimFileSystem::event_source,
        EventDelivery::Normal,
    )
    .await;
    let adaptive_nfs = run_sporadic_cliff_workload(
        adaptive_default(),
        SimFileSystem::event_source,
        EventDelivery::DropAll,
    )
    .await;

    let legacy_waves = run_continuous_sporadic_workload(
        LegacyPollingScheduler::default(),
        |_| NoopFsEventSource,
        EventDelivery::Normal,
    )
    .await;
    let adaptive_waves = run_continuous_sporadic_workload(
        adaptive_default(),
        SimFileSystem::event_source,
        EventDelivery::Normal,
    )
    .await;

    print_scorecard(&[
        ("A: 1000 idle + 2 hot (30s)", "Legacy", legacy_idle, None),
        (
            "A: 1000 idle + 2 hot (30s)",
            "Adaptive+Notify",
            adaptive_idle,
            Some(legacy_idle.eof_reads),
        ),
        (
            "B: 200 quiet after 11s idle",
            "Legacy",
            legacy_sporadic,
            None,
        ),
        (
            "B: 200 quiet after 11s idle",
            "Adaptive+Notify",
            adaptive_sporadic,
            Some(legacy_sporadic.eof_reads),
        ),
        (
            "B: 200 quiet after 11s idle",
            "Adaptive/NFS (DropAll)",
            adaptive_nfs,
            Some(legacy_sporadic.eof_reads),
        ),
        ("C: 200 sporadic waves (60s)", "Legacy", legacy_waves, None),
        (
            "C: 200 sporadic waves (60s)",
            "Adaptive+Notify",
            adaptive_waves,
            Some(legacy_waves.eof_reads),
        ),
    ]);

    // Correctness: all hot & burst lines delivered in every configuration.
    assert_eq!(legacy_idle.lines_delivered, 300);
    assert_eq!(adaptive_idle.lines_delivered, 300);
    assert_eq!(legacy_sporadic.lines_delivered, 200);
    assert_eq!(adaptive_sporadic.lines_delivered, 200);
    assert_eq!(adaptive_nfs.lines_delivered, 200);
    assert_eq!(legacy_waves.lines_delivered, 600);
    assert_eq!(adaptive_waves.lines_delivered, 600);

    // Quantitative efficiency wins:
    assert!(
        adaptive_idle.eof_reads * 3 < legacy_idle.eof_reads,
        "expected >3x reduction in wasted EOF reads on Workload A (legacy={}, adaptive={})",
        legacy_idle.eof_reads,
        adaptive_idle.eof_reads
    );
    assert!(
        adaptive_waves.eof_reads * 3 < legacy_waves.eof_reads,
        "expected >3x reduction in wasted EOF reads on Workload C (legacy={}, adaptive={})",
        legacy_waves.eof_reads,
        adaptive_waves.eof_reads
    );

    // Quantitative latency wins:
    assert!(legacy_sporadic.max_latency_ms >= 9_000);
    assert_eq!(adaptive_sporadic.max_latency_ms, 0);
    assert!(adaptive_nfs.max_latency_ms <= 10_000);
    assert!(legacy_waves.max_latency_ms >= 5_000);
    assert_eq!(adaptive_waves.max_latency_ms, 0);
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

    // Drop normal events and write to all 50 files.
    fs.set_event_delivery(EventDelivery::DropAll);
    for i in 0..50 {
        fs.append_bytes(format!("/logs/f_{i:02}.log"), b"after_overflow\n");
    }

    // Negative control: before `Overflow` and before the 250ms fallback timer expires,
    // stepping MUST read 0 lines (proving stale `dirty` entries did NOT survive EOF!).
    clock.advance(Duration::from_millis(50));
    let _ = state.step(&mut lines).await;
    assert_eq!(
        lines.len(),
        50,
        "negative control: parked files must not be polled before fallback or Overflow"
    );

    // Now inject `IN_Q_OVERFLOW`: a single step immediately reads all 50 files!
    fs.inject_event_overflow();
    let _ = state.step(&mut lines).await;
    assert_eq!(lines.len(), 100);
}

/// Regression test for P0 #2: `oldest_first = true`, `max_read_bytes = 1`, and
/// `remove_after_secs = 0` must NEVER unlink a newer skipped file before reading it.
#[tokio::test]
async fn p0_remove_after_zero_with_oldest_first_never_deletes_unread_file() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/a_old.log", b"old_line_1\nold_line_2\n");
    fs.append_bytes("/logs/b_new.log", b"new_line_1\n");
    fs.set_created(
        "/logs/a_old.log",
        clock.now_utc() - chrono::Duration::seconds(10),
    );
    fs.set_created("/logs/b_new.log", clock.now_utc());

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let mut server = make_bench_server(&fs, emitter);
    server.oldest_first = true;
    server.max_read_bytes = 1;
    server.remove_after = Some(Duration::ZERO);

    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    // Step 1: `a_old.log` exceeds `max_read_bytes=1` on line 1 and stops the loop.
    // `b_new.log` was skipped and MUST still exist unread!
    let mut lines = Vec::new();
    let _ = state.step(&mut lines).await;
    assert_eq!(lines.len(), 1);
    assert!(
        fs.exists("/logs/b_new.log"),
        "skipped newer file must not be deleted before being read"
    );

    // Drain remaining lines: all 3 lines across both files must be delivered!
    for _ in 0..5 {
        clock.advance(Duration::from_secs(1));
        let _ = state.step(&mut lines).await;
    }
    let texts: Vec<&[u8]> = lines.iter().map(|l| l.text.as_ref()).collect();
    assert_eq!(
        texts,
        vec![&b"old_line_1"[..], &b"old_line_2"[..], &b"new_line_1"[..]]
    );
}

/// Regression test for P0 #3: a read ending at EOF yields no candidate before its
/// fallback deadline and exactly one candidate after a fresh `FsEvent::Modified`
/// (never double-selecting the same `FileFingerprint` in one tick).
#[test]
fn p0_adaptive_read_ending_at_eof_no_duplicate_or_early_select() {
    use file_source_common::FileFingerprint;

    let clock = SimClock::new();
    let mut sched = AdaptiveEventScheduler::new(AdaptiveSchedulerConfig::default());
    let fp = FileFingerprint::DevInode(1, 1);
    let path = PathBuf::from("/logs/a.log");

    let t0 = clock.now();
    sched.on_file_added(fp, &path, t0, t0);

    let mut out = Vec::new();
    sched.select_readable(t0, &mut out);
    assert_eq!(out, vec![fp]);

    // Simulate reading 1 line and then hitting EOF.
    sched.on_file_eof(fp, 1, t0);

    // 50ms later (< 250ms fallback): MUST yield 0 candidates.
    clock.advance(Duration::from_millis(50));
    out.clear();
    sched.select_readable(clock.now(), &mut out);
    assert!(out.is_empty(), "EOF file must not be re-selected early");

    // Feed a fresh FsEvent::Modified AND advance past 250ms fallback: MUST yield `fp` ONCE.
    sched.on_fs_event(&crate::scheduler::FsEvent::Modified(path), clock.now());
    clock.advance(Duration::from_millis(300));
    out.clear();
    sched.select_readable(clock.now(), &mut out);
    assert_eq!(
        out,
        vec![fp],
        "file must never be selected twice in one tick"
    );
}

/// Regression test for P0 #4: a live stream of empty lines (`\n`) must NOT be
/// throttled to the 10-second idle cliff by `LegacyPollingScheduler`.
#[tokio::test]
async fn p0_legacy_blank_line_stream_not_throttled_at_10s() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/blank.log", b"\n");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = make_bench_server(&fs, emitter);
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    // Append a blank line every 2s for 14s (past the 10s cliff).
    for expected in 1..=7 {
        let _ = state.step(&mut lines).await;
        assert_eq!(
            lines.len(),
            expected,
            "blank-line stream was wrongly throttled at step {expected}"
        );
        clock.advance(Duration::from_secs(2));
        fs.append_bytes("/logs/blank.log", b"\n");
    }
}

/// Regression test for P0 #5: with `rotate_wait = 0` and dropped FS events (`DropAll`),
/// moving a parked file outside the watched glob must still drain its unread tail
/// before `reap_dead_files` closes it.
#[tokio::test]
async fn p0_adaptive_unfindable_file_drains_before_rotate_wait_zero_reap() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    fs.set_event_delivery(EventDelivery::DropAll);
    let emitter = RecordingEmitter::new();

    let writer = fs.open_writer("/logs/app.log");
    writer.append(b"line1\n");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let mut server = make_bench_server(&fs, emitter);
    server.rotate_wait = Duration::ZERO;
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

    // Park the watcher well into fallback backoff (e.g. 500ms), then append a final
    // line and move the file outside `/logs/*` while events are dropped.
    clock.advance(Duration::from_millis(500));
    let _ = state.step(&mut lines).await; // hits EOF -> next fallback is +1s
    writer.append(b"final_tail\n");
    fs.rename_file("/logs/app.log", "/archive/app.log.1")
        .unwrap();

    // Advance 1s to trigger discovery (note: fallback timer is still in the future
    // if we only advance 500ms; here glob cooldown is 1s, so advance 1s after setting
    // a 2s fallback by doing one more idle step, or verify directly on glob tick).
    clock.advance(Duration::from_millis(600));
    // Trigger discovery directly + poll + reap (as `step()` does when glob is due):
    state.discover_files().await;
    let _ = state.poll_files(&mut lines).await;
    state.reap_dead_files().await;

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].text.as_ref(), b"final_tail");
    drop(writer);
}

fn arb_event_delivery() -> impl Strategy<Value = EventDelivery> {
    prop_oneof![
        Just(EventDelivery::Normal),
        Just(EventDelivery::DropAll),
        Just(EventDelivery::Flaky { drop_every_n: 2 }),
        Just(EventDelivery::Flaky { drop_every_n: 3 }),
    ]
}

fn dedup_non_empty(lines: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for line in lines {
        if !line.is_empty() && seen.insert(line.clone()) {
            out.push(line.clone());
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// True lockstep differential DST: executes the *exact same* `(DstConfig, Vec<SimAction>)`
    /// trace on BOTH `LegacyPollingScheduler` and `AdaptiveEventScheduler` (under normal,
    /// flaky, and 100% dropped FS events + mid-stream `InjectFsOverflow`) and asserts
    /// quiescent behavioral equivalence across all live streams and final filesystem state.
    #[test]
    fn dst_lockstep_differential_legacy_vs_adaptive(
        config in arb_dst_config(),
        delivery in arb_event_delivery(),
        actions in arb_actions(120),
    ) {
        let legacy = run_dst_scenario_outcome(config.clone(), &actions)?;
        let adaptive = run_dst_scenario_adaptive_outcome(config, &actions, delivery)?;

        // 1. Final watched filesystem state (all non-checkpoint log files & symlinks)
        // must match identically. (`/data/checkpoints.new.json` depends on whether a
        // mid-trace crashed flush had dirty checkpoints at that exact tick.)
        let log_paths = |paths: &[PathBuf]| -> Vec<PathBuf> {
            paths
                .iter()
                .filter(|p| !p.starts_with("/data"))
                .cloned()
                .collect()
        };
        prop_assert_eq!(
            log_paths(&adaptive.final_paths),
            log_paths(&legacy.final_paths),
            "final log filesystem paths diverged between Legacy and Adaptive"
        );

        // 2. Compare per-stream delivered lines for every stream not lost across a restart.
        let all_inos: HashSet<u64> = legacy
            .delivered_by_ino
            .keys()
            .chain(adaptive.delivered_by_ino.keys())
            .copied()
            .collect();

        for ino in all_inos {
            if legacy.lost_across_restart_inos.contains(&ino)
                || adaptive.lost_across_restart_inos.contains(&ino)
            {
                continue;
            }
            let legacy_lines = legacy.delivered_by_ino.get(&ino).map_or(&[][..], Vec::as_slice);
            let adaptive_lines = adaptive.delivered_by_ino.get(&ino).map_or(&[][..], Vec::as_slice);

            if !legacy.allow_replay_inos.contains(&ino)
                && !adaptive.allow_replay_inos.contains(&ino)
            {
                prop_assert_eq!(
                    adaptive_lines,
                    legacy_lines,
                    "ino={}: exact delivered lines diverged between Legacy and Adaptive",
                    ino
                );
            } else {
                prop_assert_eq!(
                    dedup_non_empty(adaptive_lines),
                    dedup_non_empty(legacy_lines),
                    "ino={}: deduplicated first-seen lines diverged after unpersisted restart",
                    ino
                );
            }
        }
    }

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
