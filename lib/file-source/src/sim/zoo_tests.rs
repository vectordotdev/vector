use std::{path::PathBuf, time::Duration};

use async_compression::tokio::bufread::GzipEncoder;
use bytes::Bytes;
use file_source_common::{Checkpointer, Clock, FingerprintStrategy, Fingerprinter, ReadFrom};
use proptest::prelude::*;
use tokio::io::AsyncReadExt as _;

use crate::file_server::{FileServer, FileServerState, Line};

use super::{
    action::{DstConfig, arb_actions},
    clock::SimClock,
    emitter::RecordingEmitter,
    fs::{SimFileSystem, SimPathsProvider},
    runner::run_dst_scenario,
};

fn base_server(
    fs: &SimFileSystem,
    emitter: RecordingEmitter,
    strategy: FingerprintStrategy,
) -> FileServer<SimPathsProvider, RecordingEmitter> {
    FileServer {
        paths_provider: fs.paths_provider(&["/logs/*"], &[]),
        max_read_bytes: 2048,
        ignore_checkpoints: false,
        read_from: ReadFrom::Beginning,
        ignore_before: None,
        max_line_bytes: 4096,
        line_delimiter: Bytes::from_static(b"\n"),
        data_dir: PathBuf::from("/data"),
        glob_minimum_cooldown: Duration::from_secs(1),
        fingerprinter: Fingerprinter::new(strategy, 4096, false),
        oldest_first: false,
        remove_after: None,
        emitter,
        rotate_wait: Duration::from_secs(300),
    }
}

async fn step_and_ack(
    state: &mut FileServerState<SimPathsProvider, RecordingEmitter, SimFileSystem, SimClock>,
    out: &mut Vec<Line>,
) {
    let start_len = out.len();
    let _ = state.step(out).await;
    for line in &out[start_len..] {
        state.checkpoints().update(line.file_id, line.end_offset);
    }
}

async fn gzip_bytes(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    GzipEncoder::new(data).read_to_end(&mut out).await.unwrap();
    out
}

/// Zoo 1.2 (`#22954`, `#23076`): Torn read / premature flush when an unfindable
/// file hits EOF mid-line while the writer still holds the open inode and appends
/// the rest of the line later.
#[tokio::test]
async fn zoo_torn_read_premature_flush_on_unlinked_inode() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    let writer = fs.open_writer("/logs/app.log");
    writer.append(b"first_line\npartial_start");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = base_server(&fs, emitter, FingerprintStrategy::DevInode);
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text.as_ref(), b"first_line");

    // File is unlinked while writer still holds its open fd (`SimWriter`) and has not yet written `\n`.
    fs.unlink_file("/logs/app.log").unwrap();
    clock.advance(Duration::from_secs(1));
    step_and_ack(&mut state, &mut lines).await;

    // CURRENT BEHAVIOR (bug): single EOF while `!findable` immediately flushes the
    // partial line without a newline AND marks the watcher dead!
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].text.as_ref(), b"partial_start");
    assert_eq!(state.open_file_count(), 0);
    // Writer handle still holds 1 open fd on the unlinked inode.
    assert_eq!(fs.active_open_handles(), 1);

    // Writer now finishes the line on its live open fd — silently dropped because watcher died.
    writer.append(b"_and_the_rest\n");
    clock.advance(Duration::from_secs(1));
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 2);

    drop(writer);
    assert_eq!(fs.active_open_handles(), 0);
}

/// Zoo 1.3 (`#21084`): Wrong `start_offset` reported when a line arrives in
/// multiple chunks separated by an EOF poll.
#[tokio::test]
async fn zoo_partial_line_reports_wrong_start_offset() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/app.log", b"hello ");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = base_server(&fs, emitter, FingerprintStrategy::DevInode);
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert!(lines.is_empty());

    clock.advance(Duration::from_secs(1));
    fs.append_bytes("/logs/app.log", b"world\n");
    step_and_ack(&mut state, &mut lines).await;

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text.as_ref(), b"hello world");
    // CURRENT BEHAVIOR (bug): `start_offset` is 6 (the start of the second chunk)
    // instead of 0 (the true start of the line).
    assert_eq!(lines[0].start_offset, 6);
    assert_eq!(lines[0].end_offset, 12);
}

/// Zoo 3.2 (`#25576`, `#25079`): Under `DevInode`, inode reuse within the 60s
/// checkpoint expiration grace period causes Vector to resume at the old file's
/// offset, silently skipping the new file's contents.
#[tokio::test]
async fn zoo_dev_inode_reuse_skips_new_file() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    fs.enable_inode_reuse(true);
    let emitter = RecordingEmitter::new();

    let ino1 = fs.append_bytes(
        "/logs/a.log",
        b"01234567890123456789012345678901234567890123456789\n",
    );

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = base_server(&fs, emitter, FingerprintStrategy::DevInode);
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 1);

    // Unlink and reap `a.log`, freeing `ino1` while its checkpoint (offset 51)
    // remains in the 60s expiration grace period.
    fs.unlink_file("/logs/a.log").unwrap();
    clock.advance(Duration::from_secs(1));
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(state.open_file_count(), 0);
    assert_eq!(fs.active_open_handles(), 0);

    // Create a brand-new shorter file which recycles `ino1`.
    let ino2 = fs.create_file("/logs/b.log");
    assert_eq!(ino2, ino1, "inode should be recycled");
    fs.append_bytes("/logs/b.log", b"new_short_line\n");

    clock.advance(Duration::from_secs(2));
    step_and_ack(&mut state, &mut lines).await;

    // CURRENT BEHAVIOR (bug): `b.log` inherits checkpoint offset 51 from `a.log`,
    // so "new_short_line" (15 bytes) is never read.
    assert_eq!(lines.len(), 1);
}

/// Zoo 3.3 (`#25079`, `#17240`): Two distinct files sharing the same first line
/// collide under `FirstLinesChecksum`.
#[tokio::test]
async fn zoo_checksum_fingerprint_collision() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/a.log", b"COMMON_HEADER\nfrom_a\n");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = base_server(
        &fs,
        emitter,
        FingerprintStrategy::FirstLinesChecksum {
            ignored_header_bytes: 0,
            lines: 1,
        },
    );
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 2);

    // Second file created later with identical first line and a shorter second line.
    clock.advance(Duration::from_secs(2));
    fs.append_bytes("/logs/b.log", b"COMMON_HEADER\nb2\n");
    step_and_ack(&mut state, &mut lines).await;

    // CURRENT BEHAVIOR: collision switches watcher to newer file `b.log` at the old
    // offset (21 bytes), which is past `b.log`'s length (17 bytes), so "b2" is missed.
    assert_eq!(lines.len(), 2);
}

/// Zoo 5.1 (`#26552`): Combining `ignore_older_secs` and `remove_after_secs`
/// immediately deletes unread old files.
#[tokio::test]
async fn zoo_remove_after_deletes_ignore_older_file_unread() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/old.log", b"never_read\n");
    let old_mtime = clock.now_utc() - chrono::Duration::seconds(120);
    fs.set_modified("/logs/old.log", old_mtime);

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let mut server = base_server(&fs, emitter.clone(), FingerprintStrategy::DevInode);
    server.ignore_before = Some(clock.now_utc() - chrono::Duration::seconds(60));
    server.remove_after = Some(Duration::from_secs(30));

    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;
    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;

    // CURRENT BEHAVIOR (bug `#26552`): 0 lines read, yet `old.log` is deleted!
    assert!(lines.is_empty());
    assert!(!fs.exists("/logs/old.log"));
    emitter.with_events(|ev| assert_eq!(ev.deleted.len(), 1));
}

/// Zoo 5.3 (`#22786`): Empty / small files in `known_small_files` are cleaned up
/// once `remove_after_secs` elapses.
#[tokio::test]
async fn zoo_remove_after_cleans_up_small_files() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.create_file("/logs/empty.log");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let mut server = base_server(
        &fs,
        emitter.clone(),
        FingerprintStrategy::FirstLinesChecksum {
            ignored_header_bytes: 0,
            lines: 1,
        },
    );
    server.remove_after = Some(Duration::from_secs(10));

    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;
    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert!(fs.exists("/logs/empty.log"));

    clock.advance(Duration::from_secs(11));
    step_and_ack(&mut state, &mut lines).await;
    assert!(!fs.exists("/logs/empty.log"));
    emitter.with_events(|ev| assert_eq!(ev.deleted.len(), 1));
}

/// Zoo 2.2 (`#1053`): Atomic symlink target rotation.
#[tokio::test]
async fn zoo_symlink_target_rotation() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/targets/app-1.log", b"from_target_1\n");
    fs.set_symlink("/logs/current.log", "/targets/app-1.log");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = base_server(&fs, emitter, FingerprintStrategy::DevInode);
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text.as_ref(), b"from_target_1");

    // Rotate symlink to a new target file.
    fs.append_bytes("/targets/app-2.log", b"from_target_2\n");
    fs.set_symlink("/logs/current.log", "/targets/app-2.log");

    clock.advance(Duration::from_secs(2));
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].text.as_ref(), b"from_target_2");
}

/// Zoo 2.3 & 6.2 (`#22050`, `#24280`, `#25614`): Rotation with gzip compression
/// deduplicates against plain checkpoint, and multi-member gzip streams read all members.
#[tokio::test]
async fn zoo_gzip_rotation_dedup_and_multi_member() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/app.log", b"line_one\nline_two\n");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = base_server(
        &fs,
        emitter,
        FingerprintStrategy::FirstLinesChecksum {
            ignored_header_bytes: 0,
            lines: 1,
        },
    );
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 2);

    // Rotate and compress `app.log` into `app.log.1.gz` inside the watched glob.
    let gz = gzip_bytes(b"line_one\nline_two\n").await;
    fs.unlink_file("/logs/app.log").unwrap();
    fs.append_bytes("/logs/app.log.1.gz", &gz);

    clock.advance(Duration::from_secs(2));
    step_and_ack(&mut state, &mut lines).await;
    // Uncompressed-content fingerprint matches the existing checkpoint -> NOT re-read.
    assert_eq!(lines.len(), 2);

    // A brand-new multi-member concatenated gzip file IS read completely.
    let mut multi = gzip_bytes(b"member_1\n").await;
    multi.extend(gzip_bytes(b"member_2\n").await);
    fs.append_bytes("/logs/multi.gz", &multi);

    clock.advance(Duration::from_secs(2));
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[2].text.as_ref(), b"member_1");
    assert_eq!(lines[3].text.as_ref(), b"member_2");
}

/// Zoo 2.4 (`#24676`, `#18904`): `rotate_wait` expiration reaps unfindable files,
/// releases the fd, and reports accurate `bytes_unread`.
#[tokio::test]
async fn zoo_rotate_wait_expiration_reports_bytes_unread() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    // 3 lines of 6 bytes each = 18 bytes; limit max_read_bytes so only 1 line is read.
    fs.append_bytes("/logs/app.log", b"line1\nline2\nline3\n");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let mut server = base_server(&fs, emitter.clone(), FingerprintStrategy::DevInode);
    server.max_read_bytes = 1;
    server.rotate_wait = Duration::from_secs(5);

    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;
    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 1);

    // Unlink file and advance past `rotate_wait` without reading the remaining 2 lines:
    // call `discover_files` + `reap_dead_files` to simulate backpressure preventing reads.
    fs.unlink_file("/logs/app.log").unwrap();
    clock.advance(Duration::from_secs(1));
    state.discover_files().await;
    clock.advance(Duration::from_secs(6));
    state.reap_dead_files().await;

    assert_eq!(state.open_file_count(), 0);
    assert_eq!(fs.active_open_handles(), 0);
    emitter.with_events(|ev| {
        assert_eq!(ev.unwatched.len(), 1);
        assert_eq!(ev.unwatched[0].bytes_unread, Some(12));
        assert!(!ev.unwatched[0].reached_eof);
    });
}

/// Zoo 4.2 (`#20753`): Crash / partial write to `checkpoints.new.json` does not
/// destroy recovery from the previous stable `checkpoints.json`.
#[tokio::test]
async fn zoo_crash_mid_checkpoint_recovers_stable_file() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/app.log", b"line1\n");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = base_server(&fs, emitter.clone(), FingerprintStrategy::DevInode);
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 1);
    cp.write_checkpoints().await.unwrap();

    // Append line2, read & ack in memory, then inject a partial-write crash when
    // persisting `checkpoints.new.json` (leaving a truncated/corrupt tmp file).
    clock.advance(Duration::from_secs(1));
    fs.append_bytes("/logs/app.log", b"line2\n");
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 2);

    fs.inject_partial_write_crash(Some(12));
    assert!(cp.write_checkpoints().await.is_err());
    drop(state);

    // Restart: corrupted `checkpoints.new.json` is skipped and stable `checkpoints.json`
    // (at end of line1) is loaded cleanly, so line2 is re-delivered without re-reading line1.
    let mut cp2 = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server2 = base_server(&fs, emitter, FingerprintStrategy::DevInode);
    let mut state2 = FileServerState::init(server2, &mut cp2, fs.clone(), clock.clone()).await;

    let mut restarted_lines = Vec::new();
    step_and_ack(&mut state2, &mut restarted_lines).await;
    assert_eq!(restarted_lines.len(), 1);
    assert_eq!(restarted_lines[0].text.as_ref(), b"line2");
}

/// Zoo 7.4 (`#8499`): An unreadable file (`EACCES`) does not stall other files,
/// and is picked up once permissions are restored.
#[tokio::test]
async fn zoo_permission_denied_isolation_and_recovery() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/good.log", b"good1\n");
    fs.append_bytes("/logs/secret.log", b"secret1\n");
    fs.set_readable("/logs/secret.log", false);

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = base_server(&fs, emitter.clone(), FingerprintStrategy::DevInode);
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text.as_ref(), b"good1");
    emitter.with_events(|ev| assert_ne!(ev.fingerprint_read_errors.len(), 0));

    // Restore permissions -> secret.log is discovered and read on the next glob.
    fs.set_readable("/logs/secret.log", true);
    clock.advance(Duration::from_secs(2));
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].text.as_ref(), b"secret1");
}

/// Zoo 7.3 (`#21132`): Under Windows-like `strict_delete_while_open`, `remove_after`
/// fails with `PermissionDenied` because `FileWatcher` still holds an open handle.
#[tokio::test]
async fn zoo_windows_strict_delete_while_open_blocks_remove_after() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    fs.set_strict_delete_while_open(true);
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/win.log", b"hello\n");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let mut server = base_server(&fs, emitter.clone(), FingerprintStrategy::DevInode);
    server.remove_after = Some(Duration::from_secs(5));
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 1);

    clock.advance(Duration::from_secs(6));
    step_and_ack(&mut state, &mut lines).await;

    // CURRENT BEHAVIOR: `FileServer` tries to remove while `FileWatcher` still holds
    // the file handle open -> `PermissionDenied` error recorded, file remains on disk.
    assert!(fs.exists("/logs/win.log"));
    emitter.with_events(|ev| assert_eq!(ev.delete_errors.len(), 1));
}

/// Zoo 2.5 (`#2681`, `#20602`): Preallocated NULL-padded file overwritten in place.
#[tokio::test]
async fn zoo_preallocated_null_padded_file() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.create_file("/logs/prealloc.log");
    fs.write_at("/logs/prealloc.log", 0, &[0u8; 64]);
    fs.write_at("/logs/prealloc.log", 0, b"log1\n");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = base_server(&fs, emitter, FingerprintStrategy::DevInode);
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text.as_ref(), b"log1");

    // Subsequent overwrite of the NULL padding at offset 5 is invisible because
    // the watcher already consumed the 59 NUL bytes into `self.buf`!
    fs.write_at("/logs/prealloc.log", 5, b"log2\n");
    clock.advance(Duration::from_secs(1));
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 1);
}

/// Zoo 7.2 (`#24327`): `oldest_first` prioritizes draining the oldest file before
/// moving to newer files.
#[tokio::test]
async fn zoo_oldest_first_drains_oldest_before_newer() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/old.log", b"old_1\nold_2\nold_3\n");
    fs.append_bytes("/logs/new.log", b"new_1\nnew_2\n");
    fs.set_created(
        "/logs/old.log",
        clock.now_utc() - chrono::Duration::seconds(100),
    );
    fs.set_created("/logs/new.log", clock.now_utc());

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let mut server = base_server(&fs, emitter, FingerprintStrategy::DevInode);
    server.oldest_first = true;
    server.max_read_bytes = 1; // force 1 line per file per step

    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;
    let mut lines = Vec::new();
    for _ in 0..6 {
        step_and_ack(&mut state, &mut lines).await;
    }

    let texts: Vec<&[u8]> = lines.iter().map(|l| l.text.as_ref()).collect();
    assert_eq!(
        texts,
        vec![
            &b"old_1"[..],
            &b"old_2"[..],
            &b"old_3"[..],
            &b"new_1"[..],
            &b"new_2"[..],
        ]
    );
}

/// Discovered by `proptest` DST! `FileWatcher::is_gzipped` calls `fill_buf()`
/// once and checks `header_bytes.starts_with(GZIP_MAGIC)`. If the OS returns a
/// 1-byte short read (`\x1f`), a complete valid gzip file is permanently
/// misclassified as plain text.
#[tokio::test]
async fn zoo_is_gzipped_fails_on_one_byte_short_read() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    fs.set_max_bytes_per_read(Some(1));
    let emitter = RecordingEmitter::new();

    let gz = gzip_bytes(b"hello_gzip\n").await;
    fs.append_bytes("/logs/short_read.gz", &gz);

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = base_server(&fs, emitter, FingerprintStrategy::DevInode);
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;

    // CURRENT BEHAVIOR (bug): misclassified as plain text because first read returned 1 byte.
    assert!(
        lines.iter().all(|l| l.text.as_ref() != b"hello_gzip"),
        "expected current buggy behavior to fail to decompress gzip on 1-byte short read"
    );
}

/// Discovered by `proptest` DST! Transient `EACCES` on a watched file causes
/// `discover_files` to treat it identically to deletion (`!file_findable()`),
/// causing `FileWatcher` to immediately flush a buffered partial line without a
/// newline and reap the watcher — tearing a live log line in half.
#[tokio::test]
async fn zoo_transient_eacces_tears_partial_line() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    fs.append_bytes("/logs/app.log", b"partial_without_newline");

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server = base_server(&fs, emitter, FingerprintStrategy::DevInode);
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert!(lines.is_empty());

    // Transient EACCES during discovery marks watcher `!findable` -> flushes torn partial line!
    fs.set_readable("/logs/app.log", false);
    clock.advance(Duration::from_secs(1));
    step_and_ack(&mut state, &mut lines).await;

    // CURRENT BEHAVIOR (bug): torn partial line flushed without a newline and watcher reaped.
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text.as_ref(), b"partial_without_newline");
    assert_eq!(state.open_file_count(), 0);
}

/// Discovered by `proptest` DST! Restarting `FileServer` midway through reading
/// a gzip file (when a non-zero checkpoint has been persisted) opens the file as
/// `FileReader::Null`, permanently dropping all remaining unread lines in the archive.
#[tokio::test]
async fn zoo_gzip_partial_checkpoint_restart_drops_remainder() {
    let clock = SimClock::new();
    let fs = SimFileSystem::new(clock.clone());
    let emitter = RecordingEmitter::new();

    let mut multi = gzip_bytes(b"line1_aaaaaaaaaaaaaaaa\nline2_bbbbbbbbbbbbbbbb\n").await;
    multi.extend(gzip_bytes(b"line3_cccccccccccccccc\nline4_dddddddddddddddd\n").await);
    fs.append_bytes("/logs/archive.gz", &multi);

    let mut cp = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let mut server = base_server(&fs, emitter.clone(), FingerprintStrategy::DevInode);
    // Force stopping after the first 2 lines.
    server.max_read_bytes = 32;
    let mut state = FileServerState::init(server, &mut cp, fs.clone(), clock.clone()).await;

    let mut lines = Vec::new();
    step_and_ack(&mut state, &mut lines).await;
    assert_eq!(lines.len(), 2);

    cp.write_checkpoints().await.unwrap();
    drop(state);

    // Restart from persisted checkpoint -> remaining 2 lines in the gzip archive are skipped!
    let mut cp2 = Checkpointer::new_with(&PathBuf::from("/data"), fs.clone(), clock.clone());
    let server2 = base_server(&fs, emitter, FingerprintStrategy::DevInode);
    let mut state2 = FileServerState::init(server2, &mut cp2, fs.clone(), clock.clone()).await;

    let mut after_restart = Vec::new();
    for _ in 0..3 {
        clock.advance(Duration::from_secs(2));
        step_and_ack(&mut state2, &mut after_restart).await;
    }

    // CURRENT BEHAVIOR (bug): `FileWatcher::new` opens checkpointed gzip as `FileReader::Null`.
    assert!(after_restart.is_empty());
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 30,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Zoo 1.1: Extreme torn/short-read stress test where every `AsyncRead::poll_read`
    /// returns at most 1 byte, with automatic `proptest` shrinking on failure.
    #[test]
    fn zoo_dst_extreme_one_byte_short_reads(actions in arb_actions(100)) {
        run_dst_scenario(
            DstConfig {
                strategy: FingerprintStrategy::DevInode,
                max_bytes_per_read: Some(1),
                ..DstConfig::default()
            },
            &actions,
        )?;
    }
}
