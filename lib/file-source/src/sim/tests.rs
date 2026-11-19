use std::{path::PathBuf, time::Duration};

use async_compression::tokio::bufread::GzipEncoder;
use bytes::Bytes;
use file_source_common::{Checkpointer, FingerprintStrategy, Fingerprinter, ReadFrom};
use proptest::prelude::*;
use tokio::io::AsyncReadExt as _;

use crate::file_server::{FileServer, FileServerState, Line};

use super::{
    action::{DstConfig, arb_actions, arb_dst_config},
    clock::SimClock,
    emitter::RecordingEmitter,
    fs::{SimFileSystem, SimPathsProvider},
    runner::run_dst_scenario,
};

fn make_server(
    fs: &SimFileSystem,
    emitter: RecordingEmitter,
    strategy: FingerprintStrategy,
    max_line_bytes: usize,
    line_delimiter: &'static [u8],
) -> FileServer<SimPathsProvider, RecordingEmitter> {
    FileServer {
        paths_provider: fs.paths_provider(&["/logs/*"], &[]),
        max_read_bytes: 2048,
        ignore_checkpoints: false,
        read_from: ReadFrom::Beginning,
        ignore_before: None,
        max_line_bytes,
        line_delimiter: Bytes::from_static(line_delimiter),
        data_dir: PathBuf::from("/data"),
        glob_minimum_cooldown: Duration::from_secs(1),
        fingerprinter: Fingerprinter::new(strategy, max_line_bytes, false),
        oldest_first: false,
        remove_after: None,
        emitter,
        rotate_wait: Duration::from_secs(300),
    }
}

async fn step_and_ack(
    state: &mut FileServerState<SimPathsProvider, RecordingEmitter, SimFileSystem, SimClock>,
    out: &mut Vec<Line>,
) -> Duration {
    let start_len = out.len();
    let backoff = state.step(out).await;
    for line in &out[start_len..] {
        state.checkpoints().update(line.file_id, line.end_offset);
    }
    backoff
}

// ---------------------------------------------------------------------------
// Characterization tests pinning CURRENT behavior (reproducing known bugs)
// ---------------------------------------------------------------------------

/// Reproduces bug #1: Under `DevInode`, in-place truncation (`copytruncate`) is
/// not detected by `FileWatcher`. New writes shorter than the previous file size
/// are silently missed until the file grows past the old offset.
#[tokio::test]
async fn characterize_copytruncate_stalls_under_dev_inode() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/app.log", b"first line of considerable length\n");

    let mut checkpointer =
        Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = make_server(&fs, emitter, FingerprintStrategy::DevInode, 1024, b"\n");
    let mut state =
        FileServerState::init(server, &mut checkpointer, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text.as_ref(), b"first line of considerable length");

    // In-place truncate (copytruncate) and write a shorter line.
    fs.truncate_file("/logs/app.log", 0);
    fs.append_bytes("/logs/app.log", b"after\n");

    for _ in 0..5 {
        clock.advance(Duration::from_secs(2));
        step_and_ack(&mut state, &mut lines).await;
    }

    // CURRENT BEHAVIOR (bug): "after" is never read because cursor > file_size.
    assert_eq!(lines.len(), 1);
}

/// Reproduces bug #2: A multi-byte delimiter (`\r\n`) split across an EOF boundary
/// loses `partial_delim` between `read_line_at` calls, corrupting line framing.
#[tokio::test]
async fn characterize_multibyte_delimiter_split_across_eof() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/app.log", b"hello\r");

    let mut checkpointer =
        Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = make_server(&fs, emitter, FingerprintStrategy::DevInode, 1024, b"\r\n");
    let mut state =
        FileServerState::init(server, &mut checkpointer, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert!(lines.is_empty());

    // Now the writer flushes the second byte of `\r\n` plus the next line.
    clock.advance(Duration::from_secs(1));
    fs.append_bytes("/logs/app.log", b"\nworld\r\n");
    step_and_ack(&mut state, &mut lines).await;

    // CURRENT BEHAVIOR (bug): `\r` was flushed into `buf` on EOF, so `\n` failed
    // to complete `\r\n`, merging the two lines into one.
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text.as_ref(), b"hello\r\nworld");
}

/// Reproduces bug #3: An oversized line (`> max_line_bytes`) split across EOF
/// resets the local `discarding` flag and emits duplicate `FileLineTooLong` events.
#[tokio::test]
async fn characterize_oversized_line_split_across_eof_emits_duplicate_events() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/app.log", b"0123456789abcdef"); // 16 bytes > max_line_bytes=10

    let mut checkpointer =
        Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = make_server(
        &fs,
        emitter.clone(),
        FingerprintStrategy::DevInode,
        10,
        b"\n",
    );
    let mut state =
        FileServerState::init(server, &mut checkpointer, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    // Poll 1: hits EOF before `\n` -> returns raw_line: None (note: poll_files only
    // emits `discarded_for_size_and_truncated` when `raw_line: Some(...)`!).
    step_and_ack(&mut state, &mut lines).await;

    // Poll 2: more bytes of the same oversized line arrive, still no `\n`.
    clock.advance(Duration::from_secs(1));
    fs.append_bytes("/logs/app.log", b"ghijk");
    step_and_ack(&mut state, &mut lines).await;

    // Poll 3: `\n` finally arrives, followed by a valid short line.
    clock.advance(Duration::from_secs(1));
    fs.append_bytes("/logs/app.log", b"\nok\n");
    step_and_ack(&mut state, &mut lines).await;

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text.as_ref(), b"ok");
    // CURRENT BEHAVIOR: the oversized line was discarded across 3 calls; the first
    // two discarded events were silently dropped by `while let Ok(RawLineResult { raw_line: Some(...) })`
    // and the third emitted a `line_too_long` event on the final call.
    emitter.with_events(|ev| {
        assert_eq!(ev.line_too_long.len(), 1);
    });
}

/// Reproduces bug #4: Under `DevInode`, a freshly created 0-byte `.gz` file is
/// opened before its 2-byte gzip magic header is written and permanently treated
/// as plain text.
#[tokio::test]
async fn characterize_dev_inode_zero_byte_gzip_misclassified() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.create_file("/logs/archive.gz");

    let mut checkpointer =
        Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = make_server(&fs, emitter, FingerprintStrategy::DevInode, 1024, b"\n");
    let mut state =
        FileServerState::init(server, &mut checkpointer, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert!(lines.is_empty());

    // Now write valid gzip data containing "compressed\n" plus a trailing newline
    // so a plain reader can emit bytes.
    let mut encoded = Vec::new();
    GzipEncoder::new(&b"compressed\n"[..])
        .read_to_end(&mut encoded)
        .await
        .unwrap();
    encoded.push(b'\n');

    clock.advance(Duration::from_secs(1));
    fs.append_bytes("/logs/archive.gz", &encoded);
    step_and_ack(&mut state, &mut lines).await;

    // CURRENT BEHAVIOR (bug): Read as raw gzip bytes, NOT decompressed "compressed".
    assert_eq!(lines.len(), 1);
    assert_ne!(lines[0].text.as_ref(), b"compressed");
}

/// Reproduces quirk #5: Once a file has been idle for >= 10s, `FileWatcher::should_read_at`
/// drops to polling only once every 10s, causing up to 10s read latency on newly appended data.
#[tokio::test]
async fn characterize_idle_file_10s_poll_cliff() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/quiet.log", b"initial\n");

    let mut checkpointer =
        Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = make_server(&fs, emitter, FingerprintStrategy::DevInode, 1024, b"\n");
    let mut state =
        FileServerState::init(server, &mut checkpointer, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 1);

    // Go idle for 11 seconds, then poll once (recording last_read_attempt at t=11s).
    clock.advance(Duration::from_secs(11));
    step_and_ack(&mut state, &mut lines).await;

    // New data arrives 1 second later (t=12s).
    clock.advance(Duration::from_secs(1));
    fs.append_bytes("/logs/quiet.log", b"wakeup\n");

    // Polling at t=12s, t=15s, t=20s STILL skips the file because
    // `last_read_attempt` was at t=11s and `< 10s` have elapsed.
    step_and_ack(&mut state, &mut lines).await;
    clock.advance(Duration::from_secs(3));
    step_and_ack(&mut state, &mut lines).await;
    clock.advance(Duration::from_secs(5));
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 1, "still blocked by 10s idle cliff at +9s");

    // Only after > 10s since last_read_attempt does it finally read "wakeup".
    clock.advance(Duration::from_secs(2));
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].text.as_ref(), b"wakeup");
}

// ---------------------------------------------------------------------------
// Pathological idle-file overhead baseline (for future scheduler comparisons)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn characterize_many_idle_files_read_overhead() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    let idle_count = 200;
    for i in 0..idle_count {
        fs.append_bytes(format!("/logs/idle_{i:03}.log"), b"seed\n");
    }
    fs.append_bytes("/logs/hot_0.log", b"hot0_init\n");
    fs.append_bytes("/logs/hot_1.log", b"hot1_init\n");

    let mut checkpointer =
        Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = make_server(&fs, emitter, FingerprintStrategy::DevInode, 1024, b"\n");
    let mut state =
        FileServerState::init(server, &mut checkpointer, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    // Initial drain
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), idle_count + 2);

    fs.reset_stats();
    let hot_writes = 50;
    for i in 0..hot_writes {
        clock.advance(Duration::from_millis(300));
        fs.append_bytes("/logs/hot_0.log", format!("h0_{i}\n").as_bytes());
        fs.append_bytes("/logs/hot_1.log", format!("h1_{i}\n").as_bytes());
        step_and_ack(&mut state, &mut lines).await;
    }

    assert_eq!(lines.len(), idle_count + 2 + hot_writes * 2);
    let fs_stats = fs.stats();
    // Baseline: with 200 idle files over 50 * 300ms = 15s, the current polling loop
    // executes thousands of wasted `read()` calls against idle files.
    assert!(
        fs_stats.reads > (hot_writes as u64) * 10,
        "expected polling baseline to execute many idle reads, got {}",
        fs_stats.reads
    );
}

// ---------------------------------------------------------------------------
// Randomized Deterministic Simulation Testing (DST) across multiple files
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    #[test]
    fn dst_multi_file_lifecycle_dev_inode(actions in arb_actions(120)) {
        run_dst_scenario(
            DstConfig {
                strategy: FingerprintStrategy::DevInode,
                ..DstConfig::default()
            },
            &actions,
        )?;
    }

    #[test]
    fn dst_multi_file_lifecycle_checksum(actions in arb_actions(120)) {
        run_dst_scenario(
            DstConfig {
                strategy: FingerprintStrategy::FirstLinesChecksum {
                    ignored_header_bytes: 0,
                    lines: 1,
                },
                ..DstConfig::default()
            },
            &actions,
        )?;
    }

    #[test]
    fn dst_full_matrix_config_and_actions(
        config in arb_dst_config(),
        actions in arb_actions(120),
    ) {
        run_dst_scenario(config, &actions)?;
    }
}
