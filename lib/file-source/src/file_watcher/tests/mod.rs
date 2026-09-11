mod experiment;
mod experiment_no_truncations;

use std::{path::PathBuf, str, thread};

use bytes::{Bytes, BytesMut};
use quickcheck::{Arbitrary, Gen};
use tokio::time::Instant;

use super::{
    EOF_READ_BACKOFF_MAX, EOF_READ_BACKOFF_MIN, FileWatcher, SKIP_CHUNK_BYTES, SkipPrefixReader,
    WatcherState, null_reader,
};

// Welcome.
//
// This suite of tests is structured as an interpreter of file system
// actions. You'll find two interpreters here, `experiment` and
// `experiment_no_truncations`. These differ in one key respect: the later
// does not interpret the 'truncation' instruction.
//
// What do I mean by all this? Well, what we're trying to do is validate the
// behaviour of the file_watcher in the presence of arbitrary file-system
// actions. These actions we call `FWAction`.
#[derive(Clone, Debug)]
pub enum FileWatcherAction {
    WriteLine(String),
    RotateFile,
    DeleteFile,
    TruncateFile,
    Read,
    Pause(u32),
    Exit,
}
// WriteLine writes an arbitrary line of text -- plus newline -- RotateFile
// rotates the file as a log rotator might etc etc. Our interpreter
// functions take these instructions and apply them to the system under test
// (SUT), being a file_watcher pointed at a certain directory on-disk. In
// this way we can drive the behaviour of file_watcher. Validation requires
// a model, which we scattered between the interpreters -- as the model
// varies slightly in the presence of truncation vs. not -- and FWFile.
pub struct FileWatcherFile {
    contents: Vec<u8>,
    read_idx: usize,
    previous_read_size: usize,
    reads_available: usize,
}
// FWFile mimics an actual Unix file, at least for our purposes here. The
// operations available on FWFile have to do with reading and writing lines,
// truncation and resets, which mimic a delete/create cycle on the file
// system. The function `FWFile::read_line` is the most complex and you're
// warmly encouraged to read the documentation present there.
impl FileWatcherFile {
    pub fn new() -> FileWatcherFile {
        FileWatcherFile {
            contents: vec![],
            read_idx: 0,
            previous_read_size: 0,
            reads_available: 0,
        }
    }

    pub fn reset(&mut self) {
        self.contents.truncate(0);
        self.read_idx = 0;
        self.previous_read_size = 0;
        self.reads_available = 0;
    }

    pub fn truncate(&mut self) {
        self.reads_available = 0;
        self.contents.truncate(0);
    }

    pub fn write_line(&mut self, input: &str) {
        self.contents.extend_from_slice(input.as_bytes());
        self.contents.push(b'\n');
        self.reads_available += 1;
    }

    /// Read a line from storage, if a line is available to be read.
    pub fn read_line(&mut self) -> Option<String> {
        // FWFile mimics a unix file being read in a buffered fashion,
        // driven by file_watcher. We _have_ to keep on top of where the
        // reader's read index -- called read_idx -- is between reads and
        // the size of the file -- called previous_read_size -- in the event
        // of truncation.
        //
        // If we detect in file_watcher that a truncation has happened then
        // the buffered reader is seeked back to 0. This is performed in
        // like kind when we reset read_idx to 0, as in the following case
        // where there are no reads available.
        if self.contents.is_empty() && self.reads_available == 0 {
            self.read_idx = 0;
            self.previous_read_size = 0;
            return None;
        }
        // Now, the above is done only when nothing has been written to the
        // FWFile or the contents have been totally removed. The trickier
        // case is where there are maybe _some_ things to be read but the
        // read_idx might be mis-set owing to truncations.
        //
        // `read_line` is performed in a line-wise fashion. start_idx
        // and end_idx are pulled apart from one another to find the
        // start and end of the line, if there's a line to be found.
        let mut end_idx;
        let start_idx;
        // Here's where we do truncation detection. When our file has
        // shrunk, restart the search at zero index. If the file is the
        // same size -- implying that it's either not changed or was
        // truncated and then filled back in before a read could occur
        // -- we return None. Else, start searching at the present
        // read_idx.
        let max = self.contents.len();
        if self.previous_read_size > max {
            self.read_idx = 0;
            start_idx = 0;
            end_idx = 0;
        } else if self.read_idx == max {
            return None;
        } else {
            start_idx = self.read_idx;
            end_idx = self.read_idx;
        }
        // Seek end_idx forward until we hit the newline character.
        while self.contents[end_idx] != b'\n' {
            end_idx += 1;
            if end_idx == max {
                return None;
            }
        }
        // Produce the read string -- minus its newline character -- and
        // set the control variables appropriately.
        let ret = str::from_utf8(&self.contents[start_idx..end_idx]).unwrap();
        self.read_idx = end_idx + 1;
        self.reads_available -= 1;
        self.previous_read_size = max;
        // There's a trick here. What happens if we _only_ read a
        // newline character. Well, that'll happen when truncations
        // cause trimmed reads and the only remaining character in the
        // line is the newline. Womp womp
        if !ret.is_empty() {
            Some(ret.to_string())
        } else {
            None
        }
    }
}

impl Arbitrary for FileWatcherAction {
    fn arbitrary(g: &mut Gen) -> FileWatcherAction {
        let i: usize = *g.choose(&(0..100).collect::<Vec<_>>()).unwrap();
        match i {
            // These weights are more or less arbitrary. 'Pause' maybe
            // doesn't have a use but we keep it in place to allow for
            // variations in file-system flushes.
            0..=50 => {
                const GEN_ASCII_STR_CHARSET: &[u8] =
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
                let ln_sz = *g.choose(&(1..32).collect::<Vec<_>>()).unwrap();
                FileWatcherAction::WriteLine(
                    std::iter::repeat_with(|| *g.choose(GEN_ASCII_STR_CHARSET).unwrap())
                        .take(ln_sz)
                        .map(|v| -> char { v.into() })
                        .collect(),
                )
            }
            51..=69 => FileWatcherAction::Read,
            70..=75 => {
                let pause = *g.choose(&(1..3).collect::<Vec<_>>()).unwrap();
                FileWatcherAction::Pause(pause)
            }
            76..=85 => FileWatcherAction::RotateFile,
            86..=90 => FileWatcherAction::TruncateFile,
            91..=95 => FileWatcherAction::DeleteFile,
            _ => FileWatcherAction::Exit,
        }
    }
}

#[tokio::test]
async fn gzip_multi_stream_reads_all_members() {
    use async_compression::tokio::bufread::GzipEncoder;
    use std::fs;
    use tokio::io::AsyncReadExt as _;

    let dir = tempfile::TempDir::new().expect("could not create tempdir");
    let path = dir.path().join("multi.gz");

    async fn encode(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        GzipEncoder::new(data).read_to_end(&mut out).await.unwrap();
        out
    }

    // Write two separate gzip members into one file — the bug dropped the second.
    let mut bytes = encode(b"first\n").await;
    bytes.extend(encode(b"second\n").await);
    fs::write(&path, &bytes).unwrap();

    let mut fw = FileWatcher::new(
        path,
        file_source_common::ReadFrom::Beginning,
        None,
        100_000,
        Bytes::from("\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    let mut lines = Vec::new();
    for _ in 0..10 {
        fw.track_read_attempt();
        let result = fw.read_line().await.expect("read_line error");
        if let Some(raw) = result.raw_line {
            lines.push(String::from_utf8(raw.bytes.to_vec()).unwrap());
        }
        if lines.len() == 2 {
            break;
        }
    }

    assert_eq!(lines, vec!["first", "second"]);
}

fn watcher_for_timing() -> FileWatcher {
    let now = Instant::now();

    FileWatcher {
        path: PathBuf::new(),
        findable: true,
        state: WatcherState::Active {
            reader: Box::new(null_reader()),
            reached_eof: false,
            last_read_attempt: now,
            last_read_success: now,
            read_retry_delay: EOF_READ_BACKOFF_MIN,
            buf: BytesMut::new(),
        },
        file_position: 0,
        identity: None,
        gzip_read_skipped: false,
        is_dead: false,
        last_seen: now,
        max_line_bytes: 1024,
        line_delimiter: Bytes::from_static(b"\n"),
    }
}

fn read_retry_delay(watcher: &FileWatcher) -> std::time::Duration {
    match &watcher.state {
        WatcherState::Active {
            read_retry_delay, ..
        } => *read_retry_delay,
        WatcherState::Idle { .. } => panic!("watcher is idle, expected active"),
    }
}

#[test]
fn backs_off_after_eof() {
    let mut watcher = watcher_for_timing();

    watcher.track_read_attempt();
    watcher.track_read_eof();

    assert_eq!(read_retry_delay(&watcher), EOF_READ_BACKOFF_MIN);
    assert!(!watcher.should_read());

    thread::sleep(EOF_READ_BACKOFF_MIN);

    assert!(watcher.should_read());

    watcher.track_read_attempt();
    watcher.track_read_eof();

    assert_eq!(
        read_retry_delay(&watcher),
        EOF_READ_BACKOFF_MIN.saturating_mul(2)
    );
}

#[test]
fn caps_and_resets_eof_backoff() {
    let mut watcher = watcher_for_timing();

    for _ in 0..16 {
        watcher.track_read_attempt();
        watcher.track_read_eof();
    }

    assert_eq!(read_retry_delay(&watcher), EOF_READ_BACKOFF_MAX);

    watcher.track_read_success();

    assert_eq!(read_retry_delay(&watcher), EOF_READ_BACKOFF_MIN);
    assert!(!watcher.reached_eof());
}

#[test]
fn mark_ready_to_read_overrides_eof_backoff() {
    // Regression test for a bug found in review: a notify filesystem event naming an already-
    // tracked, still-`Active` watcher's path should let it read promptly even if it's currently
    // mid-EOF-backoff, rather than leaving it to wait out its own independent backoff timer (up
    // to `EOF_READ_BACKOFF_MAX`) despite a concrete "something changed" signal having just
    // arrived.
    let mut watcher = watcher_for_timing();

    watcher.track_read_attempt();
    watcher.track_read_eof();
    assert!(
        !watcher.should_read(),
        "sanity check: freshly backed off, should not read yet"
    );

    watcher.mark_ready_to_read();
    assert!(
        watcher.should_read(),
        "mark_ready_to_read must override EOF backoff immediately"
    );
}

#[test]
fn mark_ready_to_read_overrides_quiet_file_throttle() {
    // Regression test for a bug found in review: `should_read` throttles a *quiet* (long since
    // successfully read) file to at most one attempt per 10 seconds, to avoid needlessly
    // hammering `read_line` on files nobody is writing to. But that throttle is meant to pace
    // *unprompted* polling -- it must not also delay a read that a genuine notify event, naming
    // this exact path, just justified. Before this fix, notify mode's "prompt wakeup" promise
    // could be defeated for up to 10 seconds by this throttle alone.
    let mut watcher = watcher_for_timing();

    // Simulate "quiet for a while, but an attempt was just made:" long past last_read_success,
    // recent last_read_attempt -- the one combination `should_read` throttles.
    if let WatcherState::Active {
        last_read_success,
        last_read_attempt,
        ..
    } = &mut watcher.state
    {
        *last_read_success = Instant::now() - std::time::Duration::from_secs(20);
        *last_read_attempt = Instant::now();
    } else {
        unreachable!("watcher_for_timing() always returns an Active watcher");
    }
    assert!(
        !watcher.should_read(),
        "sanity check: quiet file, recent attempt, should be throttled"
    );

    watcher.mark_ready_to_read();
    assert!(
        watcher.should_read(),
        "mark_ready_to_read must override the quiet-file throttle immediately"
    );
}

// --- Idle-state tests -------------------------------------------------
//
// These exercise the fix for https://github.com/vectordotdev/vector/issues/3567:
// old, fully-read files should never get an open file handle, and
// actively-open files that go quiet should have their handle closed and be
// polled cheaply instead.

use chrono::Utc;
use file_source_common::ReadFrom;
use std::fs;
use tempfile::tempdir;

/// Write a file and return an `ignore_before` timestamp that is guaranteed to
/// postdate it -- i.e. this file counts as "too old" per `ignore_older`
/// relative to the returned cutoff. We can't reliably backdate a freshly
/// written file's mtime without a filesystem-timestamp-manipulation crate
/// (not a dependency here), so instead we push `ignore_before` into the
/// future relative to the write, which is equivalent for the purposes of the
/// `too_old` comparison in `FileWatcher::new` (`modified_time < ignore_before`).
fn write_file_and_ignore_before(path: &std::path::Path, contents: &[u8]) -> chrono::DateTime<Utc> {
    fs::write(path, contents).unwrap();
    Utc::now() + chrono::Duration::seconds(60)
}

#[tokio::test]
async fn new_old_fully_read_file_starts_idle_without_opening() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("old.log");
    let contents = b"line one\nline two\n";
    let ignore_before = Some(write_file_and_ignore_before(&path, contents));

    // Checkpoint position equal to the full file size: nothing new to read.
    let checkpoint = contents.len() as u64;

    let watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Checkpoint(checkpoint),
        ignore_before,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    assert!(
        watcher.is_idle(),
        "old, fully-read file should start in the Idle state"
    );
    assert!(!watcher.is_active());
    assert_eq!(watcher.get_file_position(), checkpoint);
}

#[tokio::test]
async fn idle_on_startup_false_keeps_old_file_active() {
    // Regression test for a bug found in review: `idle_timeout: null` is documented as
    // restoring the prior always-open behavior entirely, but the startup fast path (this same
    // scenario as `new_old_fully_read_file_starts_idle_without_opening` above) used to ignore
    // that opt-out completely -- it's a separate mechanism from the runtime `deactivate()`
    // transition that `idle_timeout` alone gates, so an `ignore_older`-excluded file would still
    // start `Idle` at discovery time regardless of `idle_timeout` being disabled. Passing
    // `idle_on_startup: false` (what `FileServer` does when `self.idle_timeout.is_none()`) must
    // skip the fast path and open the file normally instead.
    let dir = tempdir().unwrap();
    let path = dir.path().join("old_but_idle_disabled.log");
    let contents = b"line one\nline two\n";
    let ignore_before = Some(write_file_and_ignore_before(&path, contents));
    let checkpoint = contents.len() as u64;

    let watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Checkpoint(checkpoint),
        ignore_before,
        1024,
        Bytes::from_static(b"\n"),
        false,
    )
    .await
    .expect("FileWatcher::new failed");

    assert!(
        watcher.is_active(),
        "idle_on_startup: false must keep even an ignore_older-excluded file Active, not \
         silently start it Idle regardless of the opt-out"
    );
    assert!(!watcher.is_idle());
}

#[tokio::test]
async fn new_old_uncompressed_file_starts_idle_regardless_of_checkpoint() {
    // For a *non-gzip* file, once it's deemed `too_old` (per `ignore_before`),
    // the pre-existing open path always seeks straight to EOF regardless of
    // any stored checkpoint -- old files are simply not read from, whether
    // there's unread data behind a stale checkpoint or not. Because that
    // outcome doesn't depend on the checkpoint at all, the fast (stat-only,
    // no-open) idle path doesn't need to match against it either: it only
    // needs to confirm the file isn't gzip (see the gzip-specific test
    // below). So even with a checkpoint well behind the actual file size, an
    // old non-gzip file should still start Idle without ever being opened,
    // parked at the file's current size (== where an open would have left
    // it).
    let dir = tempdir().unwrap();
    let path = dir.path().join("old_with_new_data.log");
    let contents = b"line one\nline two\n";
    let ignore_before = Some(write_file_and_ignore_before(&path, contents));

    // Checkpoint position well behind the actual file size.
    let checkpoint = 5u64;

    let watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Checkpoint(checkpoint),
        ignore_before,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    assert!(
        watcher.is_idle(),
        "an old, non-gzip file should start idle even with a stale checkpoint, \
         since a full open would end up at EOF regardless"
    );
    assert_eq!(watcher.get_file_position(), contents.len() as u64);
}

#[tokio::test]
async fn new_old_uncompressed_file_without_checkpoint_starts_idle() {
    // The "cold start" case: no stored checkpoint at all (e.g. first run, or
    // `ignore_checkpoints`), just `ReadFrom::Beginning`. An old, non-gzip
    // file should still start Idle without being opened -- this is what
    // keeps a large `include` glob of old files cheap even when Vector has
    // never seen them before, not just on restart with existing checkpoints.
    let dir = tempdir().unwrap();
    let path = dir.path().join("old_cold_start.log");
    let contents = b"line one\nline two\n";
    let ignore_before = Some(write_file_and_ignore_before(&path, contents));

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        ignore_before,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    assert!(
        watcher.is_idle(),
        "an old, non-gzip file with no checkpoint should still start idle"
    );
    assert_eq!(watcher.get_file_position(), contents.len() as u64);

    // Regression coverage for a bug found in review: this watcher has never opened the file (it
    // took the startup fast path, so `identity` is still `None`), which must not be confused with
    // "the file was replaced" the first time it reactivates. Append new data and confirm
    // reactivation resumes from where the old content ended, rather than resetting to 0 and
    // re-sending the content `ignore_older` deliberately skipped in the first place.
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    use std::io::Write as _;
    writeln!(f, "line three").unwrap();
    f.flush().unwrap();
    drop(f);

    let changed = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(changed, "growth should be detected via cheap stat");
    watcher.reactivate().await.expect("reactivate failed");
    assert!(watcher.is_active());
    assert_eq!(
        watcher.get_file_position(),
        contents.len() as u64,
        "the first reactivation of a never-opened idle watcher must resume from the \
         position recorded at discovery, not reset to 0 and re-read the old, \
         ignore_older-excluded content"
    );

    let result = watcher.read_line().await.expect("read_line error");
    assert_eq!(
        result
            .raw_line
            .map(|l| String::from_utf8(l.bytes.to_vec()).unwrap()),
        Some("line three".to_string()),
        "must read only the newly appended line, not re-send the old content"
    );
}

#[tokio::test]
async fn new_old_gzip_file_without_checkpoint_starts_active() {
    // Gzip is the one case the fast path must not take: an old gzip file's
    // "too old" handling starts back at position 0 (not EOF, unlike the
    // uncompressed case), which requires actually decoding the gzip header,
    // so it must go through the full open path.
    use async_compression::tokio::bufread::GzipEncoder;
    use tokio::io::AsyncReadExt as _;

    let dir = tempdir().unwrap();
    let path = dir.path().join("old.log.gz");

    let mut encoder = GzipEncoder::new(std::io::Cursor::new(b"line one\n".to_vec()));
    let mut compressed = Vec::new();
    encoder.read_to_end(&mut compressed).await.unwrap();
    let ignore_before = Some(write_file_and_ignore_before(&path, &compressed));

    let watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        ignore_before,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    assert!(
        watcher.is_active(),
        "gzip files must always go through the full open path, even when old"
    );
}

#[tokio::test]
async fn new_file_without_ignore_before_starts_active() {
    // Sanity check: without `ignore_before` configured at all, nothing
    // should ever start idle, regardless of checkpoint/size.
    let dir = tempdir().unwrap();
    let path = dir.path().join("recent.log");
    let contents = b"only line\n";
    fs::write(&path, contents).unwrap();

    let watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Checkpoint(contents.len() as u64),
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    assert!(watcher.is_active());
}

#[tokio::test]
async fn deactivate_closes_handle_and_retains_checkpoint() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("goes_idle.log");
    fs::write(&path, b"hello\n").unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");
    assert!(watcher.is_active());

    // Read the one line so the watcher has a real position to preserve.
    let result = watcher.read_line().await.expect("read_line error");
    assert!(result.raw_line.is_some());
    let position_before = watcher.get_file_position();
    assert!(position_before > 0);

    watcher.deactivate().await;

    assert!(
        watcher.is_idle(),
        "deactivate() should transition Active -> Idle"
    );
    assert_eq!(
        watcher.get_file_position(),
        position_before,
        "checkpoint position must survive deactivation"
    );
}

#[tokio::test]
async fn deactivate_preserves_reached_eof() {
    // Regression test for a bug found in review: `reached_eof()` used to look only at
    // `WatcherState::Active`'s own flag, always reporting `false` for an `Idle` watcher.
    // `deactivate` only ever runs after EOF was reached (checked by `FileServer` before calling
    // it), so every `Idle` watcher had, by construction, reached EOF -- but that fact was lost
    // the moment the state switched, making `FileServer`'s `emit_file_unwatched` telemetry
    // misreport such watchers as abandoned mid-file when they're later reaped.
    let dir = tempdir().unwrap();
    let path = dir.path().join("reaches_eof.log");
    fs::write(&path, b"hello\n").unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    // Read the one line, then read again to actually hit EOF.
    let result = watcher.read_line().await.expect("read_line error");
    assert!(result.raw_line.is_some());
    let result = watcher.read_line().await.expect("read_line error");
    assert!(result.raw_line.is_none());
    assert!(watcher.reached_eof(), "sanity check: watcher hit EOF");

    watcher.deactivate().await;
    assert!(watcher.is_idle());
    assert!(
        watcher.reached_eof(),
        "reached_eof() must stay true across the Active -> Idle transition"
    );
}

#[tokio::test]
async fn deactivate_rewinds_past_unterminated_partial_line() {
    // `read_until_with_max_size` advances `file_position` for every byte it
    // reads into its buffer, delimiter or not: a partial line with no
    // trailing delimiter yet is bytes-read-but-not-yet-emitted, tracked in
    // the watcher's internal `buf`, waiting for a future call to complete it.
    // If `deactivate` naively idle-izes on top of that -- discarding `buf`
    // (it's part of the `Active` state being replaced) without rewinding
    // `file_position` back behind those bytes -- the partial line is gone:
    // `reactivate` would resume reading from *after* it, and since those
    // bytes were already counted as read, they'd never be retried. The fix
    // is for `deactivate` to rewind `file_position` by exactly `buf.len()`,
    // so the unterminated bytes get read again from disk (along with
    // whatever completes them) once the watcher reactivates.
    let dir = tempdir().unwrap();
    let path = dir.path().join("partial_line.log");
    // No trailing newline: `partial` is the entire, unterminated content of
    // the file at this point.
    let partial = b"unterminated-line-no-newline-yet";
    fs::write(&path, partial).unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");
    assert!(watcher.is_active());

    // Attempt a read: hits EOF with no delimiter, so nothing is emitted, but
    // (per `read_until_with_max_size`'s contract) the bytes are still
    // consumed from the reader and counted into `file_position`, buffered
    // internally awaiting the delimiter.
    let result = watcher.read_line().await.expect("read_line error");
    assert!(
        result.raw_line.is_none(),
        "no delimiter yet, so nothing should be emitted"
    );
    assert_eq!(
        watcher.get_file_position(),
        partial.len() as u64,
        "position should advance past the buffered-but-unterminated bytes"
    );

    watcher.deactivate().await;
    assert!(watcher.is_idle());
    assert_eq!(
        watcher.get_file_position(),
        0,
        "deactivate must rewind position back behind the unterminated partial line"
    );

    // Complete the line and confirm reactivation reads the *whole* line back
    // from disk, not just the newly-appended suffix.
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    use std::io::Write as _;
    writeln!(file).unwrap(); // just the trailing newline
    drop(file);

    let changed = watcher
        .check_for_new_data()
        .await
        .expect("check_for_new_data error");
    assert!(changed, "appending the delimiter should be detected");
    watcher.reactivate().await.expect("reactivate failed");
    assert!(watcher.is_active());

    let result = watcher.read_line().await.expect("read_line error");
    let line = result.raw_line.expect("expected a complete line now");
    assert_eq!(
        &line.bytes[..],
        &partial[..],
        "the full original line must be read back, not just the appended newline"
    );
}

#[tokio::test]
async fn take_final_partial_line_salvages_unterminated_bytes_when_idle_is_reaped() {
    // Regression test for a bug found in review: unlike an `Active` watcher (whose `read_line`
    // flushes a buffered-but-unterminated line the moment its file is found unfindable), an
    // `Idle` watcher is never read at all while unfindable (`FileServer::poll_idle_watchers`
    // skips it outright), so it has no path of its own to flush a trailing record with no final
    // delimiter. Before this fix, such a record was silently dropped whenever the watcher was
    // reaped (e.g. its file was rotated out of the include glob) while still `Idle`.
    // `take_final_partial_line` gives `FileServer`'s reap path a way to recover it as a
    // last-resort measure right before the watcher is discarded for good.
    let dir = tempdir().unwrap();
    let path = dir.path().join("partial_line_reaped_while_idle.log");
    let partial = b"unterminated-line-lost-if-not-salvaged";
    fs::write(&path, partial).unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");
    assert!(watcher.is_active());

    // Consume the unterminated bytes into the buffer, exactly as in
    // `deactivate_rewinds_past_unterminated_partial_line` above.
    let result = watcher.read_line().await.expect("read_line error");
    assert!(result.raw_line.is_none());

    watcher.deactivate().await;
    assert!(watcher.is_idle());

    // Simulate the file being rotated out of the include glob and the watcher being reaped,
    // without ever getting a chance to reactivate: take the salvaged line instead.
    let line = watcher
        .take_final_partial_line()
        .expect("the buffered-but-unterminated line must be salvageable after deactivate()");
    assert_eq!(
        &line.bytes[..],
        &partial[..],
        "the salvaged line must contain exactly the bytes that were buffered, unterminated"
    );
    assert_eq!(
        line.offset, 0,
        "the salvaged line's offset must be where it started in the file, not the rewound \
         (post-deactivate) file_position"
    );

    assert!(
        watcher.take_final_partial_line().is_none(),
        "take_final_partial_line must not return the same line twice"
    );
}

#[tokio::test]
async fn take_final_partial_line_salvages_from_active_watcher_too() {
    // Regression test for a bug found in review: `remove_after`-driven removal of an `Active`
    // watcher that wasn't read this cycle (e.g. `should_read()` was false) can also discard a
    // buffered-but-unterminated line without `read_line`'s own not-`file_findable` flush ever
    // running. `take_final_partial_line` must salvage it for `Active` watchers too, not just
    // `Idle` ones.
    let dir = tempdir().unwrap();
    let path = dir.path().join("partial_line_active.log");
    let partial = b"unterminated-active-line";
    fs::write(&path, partial).unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");
    assert!(watcher.is_active());

    let result = watcher.read_line().await.expect("read_line error");
    assert!(result.raw_line.is_none());

    let line = watcher
        .take_final_partial_line()
        .expect("an Active watcher's buffered-but-unterminated bytes must be salvageable too");
    assert_eq!(&line.bytes[..], &partial[..]);
    assert_eq!(line.offset, 0);

    assert!(watcher.take_final_partial_line().is_none());
}

#[tokio::test]
async fn idle_watcher_detects_new_data_and_resumes_from_correct_offset() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("resumes.log");
    fs::write(&path, b"first\n").unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    // Drain what's there, then go idle.
    let result = watcher.read_line().await.expect("read_line error");
    assert_eq!(
        result
            .raw_line
            .map(|l| String::from_utf8(l.bytes.to_vec()).unwrap()),
        Some("first".to_string())
    );
    let position_before = watcher.get_file_position();
    watcher.deactivate().await;
    assert!(watcher.is_idle());

    // No new data yet: check_for_new_data should report no change and the
    // watcher should remain idle.
    let changed = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(!changed);
    assert!(watcher.is_idle());

    // Now append new data while idle (no handle held).
    use std::io::Write;
    let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(f, "second").unwrap();
    f.flush().unwrap();
    drop(f);

    let changed = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(changed, "growth should be detected via cheap stat");

    watcher.reactivate().await.expect("reactivate failed");
    assert!(watcher.is_active());
    assert_eq!(
        watcher.get_file_position(),
        position_before,
        "reactivation must seek back to the retained checkpoint"
    );

    let result = watcher.read_line().await.expect("read_line error");
    assert_eq!(
        result
            .raw_line
            .map(|l| String::from_utf8(l.bytes.to_vec()).unwrap()),
        Some("second".to_string()),
        "resumed read must pick up exactly the new content, not re-read old data"
    );
}

#[tokio::test]
async fn invalidate_idle_bookkeeping_forces_next_check_to_report_changed() {
    // Regression test for a bug found in review: check_for_new_data unconditionally records
    // whatever size/mtime it just observed, before the caller has decided what to do about a
    // reported change. If the caller's subsequent reactivate() attempt then fails (e.g. a
    // transient permission or I/O error) and the file doesn't change *again* in the meantime, a
    // naive next poll would compare against the size/mtime already recorded from that failed
    // attempt, see no difference, and never retry -- silently stranding the watcher Idle with
    // unread data sitting on disk. invalidate_idle_bookkeeping exists to force the next poll to
    // report a change (and thus retry) regardless of what it actually observes.
    //
    // Rather than trying to simulate a reactivate() failure via filesystem tricks (unreliable:
    // filesystem timestamp granularity means a file swapped out and back can easily end up with a
    // different mtime even with identical content, which would make check_for_new_data correctly
    // report "changed" on its own, independent of whether invalidate_idle_bookkeeping does
    // anything -- exactly the kind of test that would still pass with a no-op implementation),
    // this tests the property directly: two consecutive check_for_new_data calls with genuinely
    // nothing happening to the file in between. Without invalidate_idle_bookkeeping, the second
    // call is guaranteed to report `false` (nothing changed, correctly). With it called in
    // between, the second call must report `true` even though nothing on disk actually changed.
    let dir = tempdir().unwrap();
    let path = dir.path().join("flaky.log");
    fs::write(&path, b"first\n").unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");
    let _ = watcher.read_line().await.expect("read_line error");
    watcher.deactivate().await;
    assert!(watcher.is_idle());

    // Grow the file so check_for_new_data reports a change; this also records the file's current
    // size/mtime as the watcher's new "last known" baseline -- the state that a failed
    // reactivate() would otherwise leave stale and un-retried.
    fs::write(&path, b"first\nsecond\n").unwrap();
    let changed = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(changed, "growth should be detected via cheap stat");

    // Sanity check the premise: with nothing touching the file in between, a second consecutive
    // check must report no change (this is what a stranded-forever watcher would keep seeing).
    let changed_again = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(
        !changed_again,
        "sanity check: with nothing touching the file, a second check must see no change"
    );

    // Now invalidate, still with nothing touching the file, and confirm the next check reports a
    // change anyway -- this is the exact retry-after-a-failed-reactivate behavior being tested.
    watcher.invalidate_idle_bookkeeping();
    let changed_after_invalidate = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(
        changed_after_invalidate,
        "invalidate_idle_bookkeeping must force the next check to report a change and retry, \
         even though nothing on disk actually changed"
    );

    // Regression coverage for a bug found in review: invalidate_idle_bookkeeping's forced retry
    // must not be mistaken by check_for_new_data for "the file shrank." An earlier version of
    // this achieved the forced retry by clobbering last_known_size with a u64::MAX sentinel,
    // which any real size always compared as smaller than, wrongly latching
    // truncated_while_idle on every retry and causing reactivate to discard the correct,
    // still-at-the-end-of-"first" position and re-read the file from byte 0, re-emitting "first"
    // as a duplicate (it was already read and emitted before this watcher went idle) instead of
    // correctly resuming to pick up only "second", the genuinely new line.
    let position_before_reactivate = watcher.get_file_position();
    watcher
        .reactivate()
        .await
        .expect("reactivate should succeed");
    assert!(watcher.is_active());
    assert_eq!(
        watcher.get_file_position(),
        position_before_reactivate,
        "a retry after invalidate_idle_bookkeeping, with no real truncation involved, must \
         resume from where it left off, not discard the position and re-read from the start"
    );

    let result = watcher.read_line().await.expect("read_line error");
    assert_eq!(
        result
            .raw_line
            .map(|l| String::from_utf8(l.bytes.to_vec()).unwrap()),
        Some("second".to_string()),
        "must read exactly the new line, not re-emit \"first\" (already read before this \
         watcher went idle) as a duplicate"
    );
}

#[tokio::test]
async fn idle_watcher_detects_truncation() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("truncated.log");
    fs::write(&path, b"0123456789\n").unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    let _ = watcher.read_line().await.expect("read_line error");
    let position_before = watcher.get_file_position();
    assert!(position_before > 0);
    watcher.deactivate().await;

    // Truncate the file down to nothing while idle.
    fs::write(&path, b"").unwrap();

    let changed = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(
        changed,
        "truncation (shrink) must be detected, not just growth"
    );
}

#[tokio::test]
async fn truncation_invalidates_pending_partial_line() {
    // Regression test for a bug found in review: a partial line buffered by `deactivate` refers
    // to an offset in the pre-truncation file. If the file is then truncated while idle,
    // `check_for_new_data` must drop that stale buffer -- otherwise, if the watcher is later
    // reaped without ever reactivating, `take_final_partial_line` would hand back bytes/offset
    // that no longer correspond to anything on disk.
    let dir = tempdir().unwrap();
    let path = dir.path().join("truncated_with_partial_line.log");
    // No trailing newline, so the bytes end up buffered as an unterminated partial line.
    let partial = b"unterminated-before-truncate";
    fs::write(&path, partial).unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    let result = watcher.read_line().await.expect("read_line error");
    assert!(result.raw_line.is_none());

    watcher.deactivate().await;
    assert!(watcher.is_idle());

    // Truncate the file while idle.
    fs::write(&path, b"").unwrap();
    let changed = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(changed);

    assert!(
        watcher.take_final_partial_line().is_none(),
        "check_for_new_data must invalidate the stale pre-truncation partial line"
    );
}

#[tokio::test]
async fn idle_watcher_reads_correctly_after_same_inode_truncation() {
    // Regression test for a bug found in review: check_for_new_data only reports a bare
    // "changed," not which direction the size moved, so reactivate() must independently notice a
    // shrink and reset file_position -- identity alone isn't enough to catch this, since a
    // truncate-in-place (e.g. `logrotate`'s `copytruncate`, or an application truncating and
    // rewriting its own log file) keeps the same inode throughout. Without checking the size
    // directly, reactivate() would seek to the old (now past-EOF) position; a seek past EOF
    // doesn't error, it just means every read sees nothing until the file grows past the old
    // position again, silently losing everything written to the truncated file in the meantime.
    let dir = tempdir().unwrap();
    let path = dir.path().join("truncated_rewrite.log");
    fs::write(&path, b"0123456789\n").unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    let _ = watcher.read_line().await.expect("read_line error");
    let position_before = watcher.get_file_position();
    assert!(position_before > 0);
    let identity_before = watcher_identity(&watcher);
    watcher.deactivate().await;
    assert!(watcher.is_idle());

    // Truncate the file in place (same inode on both Unix and Windows: this opens the existing
    // file with O_TRUNC/equivalent rather than creating a new one) and write new, shorter
    // content -- shorter than `position_before`, so a stale seek would land past this file's end.
    fs::write(&path, b"short\n").unwrap();

    let changed = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(changed, "truncation must be detected");

    watcher.reactivate().await.expect("reactivate failed");
    assert!(watcher.is_active());
    assert_eq!(
        watcher_identity(&watcher),
        identity_before,
        "sanity check: this must be a same-inode truncation, not a same-path replacement \
         (which is covered by a separate test) -- otherwise this test wouldn't be exercising \
         the code path it's meant to"
    );
    assert_eq!(
        watcher.get_file_position(),
        0,
        "reactivating after a same-inode truncation must reset the read position, not seek \
         to the old (now past-EOF) offset"
    );

    let result = watcher.read_line().await.expect("read_line error");
    assert_eq!(
        result
            .raw_line
            .map(|l| String::from_utf8(l.bytes.to_vec()).unwrap()),
        Some("short".to_string()),
        "must read the truncated file's new content from the start, not silently lose it"
    );
}

#[tokio::test]
async fn idle_watcher_reads_correctly_after_truncate_then_refill_past_old_position() {
    // Regression test for a bug found in review: reactivate()'s previous fix only compared the
    // file's size *at the moment of reactivation* against file_position. That misses a truncate
    // that gets refilled *past* the old file_position again before reactivation, e.g.: read up
    // to offset 1000, the file gets truncated to 0 (observed by one check_for_new_data poll),
    // then rewritten with 1500 bytes of unrelated new content before the watcher reactivates. At
    // reactivation time the file's current size (1500) is >= file_position (1000), which looks
    // exactly like ordinary growth: nothing about a single point-in-time comparison reveals that
    // a truncation happened at some point along the way. Without remembering that a poll *did*
    // see a shrink at some point, reactivate would seek to byte 1000 of the *new* content and
    // treat it as a continuation of the old file, silently fabricating a bogus resumption point.
    //
    // Note this specifically requires the shrink and the eventual regrowth to be observed by
    // *separate* check_for_new_data polls: if both file writes happen between two polls with
    // nothing in between ever observing the intermediate empty state, no polling-based approach
    // (this one included, and this isn't specific to Vector) can tell that apart from ordinary
    // growth -- there's no state on disk left behind to detect it from after the fact. That's a
    // fundamental limitation of polling for changes rather than something reactivate could
    // special-case around, and applies identically to `file_discovery_mode: polling`'s pre-existing
    // handling of *active* (never-idle) files, not something this idle-handle-closing feature
    // introduces. This test instead models the realistic case the fix actually addresses: a
    // truncation slow enough to be independently observed by its own poll, followed by unrelated
    // regrowth observed later, which is exactly what FileServer's own poll_idle_watchers does on
    // every discovery cycle in production.
    let dir = tempdir().unwrap();
    let path = dir.path().join("truncate_then_refill.log");
    // A single 999-byte line plus its newline: file_position after reading it lands at exactly
    // 1000, a clean, known value to assert against once reactivated.
    fs::write(&path, format!("{}\n", "a".repeat(999))).unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        4096,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    let result = watcher.read_line().await.expect("read_line error");
    assert!(result.raw_line.is_some());
    let position_before = watcher.get_file_position();
    assert_eq!(
        position_before, 1000,
        "sanity check on the test's own setup"
    );
    let identity_before = watcher_identity(&watcher);
    watcher.deactivate().await;
    assert!(watcher.is_idle());

    // First poll: observe the truncation to nothing. This is what latches
    // `truncated_while_idle`.
    fs::write(&path, "").unwrap();
    let changed = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(changed, "the truncation to empty must be detected");

    // Refill with new content longer than `position_before`, observed by a second poll before
    // reactivation -- by this point the file looks, size-wise, like it simply grew past its old
    // position, exactly as ordinary (non-truncating) growth would.
    fs::write(&path, format!("{}\n", "z".repeat(1499))).unwrap();
    let changed = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(changed, "the regrowth must also be detected");

    watcher.reactivate().await.expect("reactivate failed");
    assert!(watcher.is_active());
    assert_eq!(
        watcher_identity(&watcher),
        identity_before,
        "sanity check: same inode throughout, exercising the same-identity truncation path"
    );
    assert_eq!(
        watcher.get_file_position(),
        0,
        "must reset to 0 even though the file's *final* size is larger than the old \
         file_position -- a truncate happened in between, which a single point-in-time \
         size comparison at reactivation time can't see on its own"
    );

    let result = watcher.read_line().await.expect("read_line error");
    let line = result.raw_line.expect("expected a line");
    assert_eq!(
        line.bytes.len(),
        1499,
        "must read the new content from its actual start (byte 0), not from the stale \
         offset 1000 into what is now unrelated data"
    );
    assert!(
        line.bytes.iter().all(|&b| b == b'z'),
        "must not splice together old and new content: every byte of the line read back \
         must be from the new content, none from the old"
    );
}

#[tokio::test]
async fn idle_watcher_detects_truncation_observed_on_the_forced_retry_poll() {
    // Regression test for a bug found in review, one step further than the truncate-then-refill
    // test above. An earlier fix for invalidate_idle_bookkeeping's forced retry worked by
    // clobbering last_known_size with a u64::MAX sentinel so the next check_for_new_data would
    // always report `changed`. That broke the shrink-detection this same function is responsible
    // for: on the very next poll, `new_size < *last_known_size` compared the real (possibly
    // already-refilled) size against u64::MAX, which is *always* true regardless of whether the
    // file actually shrank -- so a guard (`had_valid_baseline`) was added to skip the
    // shrink-check whenever the baseline was the sentinel. But that guard traded one bug for
    // another: it went from "always false-positive" to "always skip," which means a *real*
    // truncation observed on exactly that first post-invalidate poll would go completely
    // undetected, not just misreported. Concretely:
    //   1. watcher reads up to file_position 1000, then goes idle.
    //   2. reactivate() fails for some transient reason (e.g. a permissions error), and the
    //      caller calls invalidate_idle_bookkeeping() to force a retry on the next poll.
    //   3. before that next poll runs, the file is truncated down to 200 bytes -- smaller than
    //      the old file_position, and still observably smaller than it by the time the forced
    //      retry poll actually samples the file.
    //   4. the next check_for_new_data poll is the forced retry from step 2. With the buggy
    //      guard, it would skip the shrink check entirely (because a retry was pending) and so
    //      never latch `truncated_while_idle`, even though the shrink was plainly visible on this
    //      exact poll.
    //   5. the file is then refilled past the old file_position (to 1500 bytes) and observed by a
    //      second, ordinary poll -- at which point, without the latch from step 4, nothing
    //      remains to distinguish this from ordinary growth.
    //   6. reactivate() must still resume from byte 0, not treat the final size (1500) as
    //      ordinary growth past the old file_position (1000) and resume reading stale data.
    //
    // The current fix (a dedicated `force_recheck` flag, separate from `last_known_size`) keeps
    // last_known_size holding the *real* last observed size (1000, from before going idle)
    // through invalidate_idle_bookkeeping, so the forced-retry poll's shrink check compares the
    // real new size (200) against the real old baseline (1000) like any other poll, correctly
    // latching `truncated_while_idle` -- the forced-retry behavior comes entirely from the
    // separate flag instead of from corrupting the baseline.
    let dir = tempdir().unwrap();
    let path = dir.path().join("truncate_observed_on_forced_retry.log");
    fs::write(&path, format!("{}\n", "a".repeat(999))).unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        4096,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    let result = watcher.read_line().await.expect("read_line error");
    assert!(result.raw_line.is_some());
    assert_eq!(
        watcher.get_file_position(),
        1000,
        "sanity check on the test's own setup"
    );
    let identity_before = watcher_identity(&watcher);
    watcher.deactivate().await;
    assert!(watcher.is_idle());

    // Simulate a failed reactivate() attempt forcing a retry, without needing to actually break
    // the filesystem to trigger one.
    watcher.invalidate_idle_bookkeeping();

    // Truncate to a size still smaller than the old file_position (1000), and have this exact
    // poll -- the forced retry from invalidate_idle_bookkeeping -- be the one that observes it.
    fs::write(&path, "b".repeat(200)).unwrap();
    let changed = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(
        changed,
        "must report changed, both because of the forced retry and because the size differs \
         from the last known baseline"
    );

    // Refill past the old file_position, observed by a second, ordinary poll -- by itself this
    // looks exactly like ordinary growth, the same as in the test above.
    fs::write(&path, format!("{}\n", "z".repeat(1499))).unwrap();
    let changed = watcher
        .check_for_new_data()
        .await
        .expect("stat should succeed");
    assert!(changed, "the regrowth must also be detected");

    watcher.reactivate().await.expect("reactivate failed");
    assert!(watcher.is_active());
    assert_eq!(
        watcher_identity(&watcher),
        identity_before,
        "sanity check: same inode throughout, exercising the same-identity truncation path"
    );
    assert_eq!(
        watcher.get_file_position(),
        0,
        "must reset to 0: the file was truncated while idle and that truncation was observed on \
         the very poll that also served as the forced retry from invalidate_idle_bookkeeping, \
         even though the file's final size (1500) is larger than the old file_position (1000)"
    );

    let result = watcher.read_line().await.expect("read_line error");
    let line = result.raw_line.expect("expected a line");
    assert_eq!(
        line.bytes.len(),
        1499,
        "must read the new content from its actual start (byte 0), not from the stale \
         offset 1000 into what is now unrelated data"
    );
    assert!(
        line.bytes.iter().all(|&b| b == b'z'),
        "must not splice together old and new content: every byte of the line read back \
         must be from the new content, none from the old"
    );
}

#[tokio::test]
async fn idle_watcher_survives_rotation_without_reading_wrong_file() {
    // A rotation while idle: the original file is renamed away and a new,
    // unrelated file is created at the same path. `FileServer`'s
    // fingerprint-based identity tracking is what actually prevents
    // misattributing content across this rename in production; here we
    // verify the pieces `FileWatcher` itself is responsible for: identity
    // (dev/inode) is checked on reactivation via `update_path`/`reactivate`,
    // so a same-path-different-file swap cannot silently resume from a
    // stale offset into unrelated content.
    let dir = tempdir().unwrap();
    let path = dir.path().join("rotated.log");
    fs::write(&path, b"original content here\n").unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");
    let original_identity = watcher_identity(&watcher);

    let _ = watcher.read_line().await.expect("read_line error");
    watcher.deactivate().await;
    assert!(watcher.is_idle());

    // Simulate rotation: move the original file away, create a new one in
    // its place with different (shorter) content.
    let archived = dir.path().join("rotated.log.1");
    fs::rename(&path, &archived).unwrap();
    fs::write(&path, b"new\n").unwrap();

    // A cheap stat-only poll will very likely see *some* difference (size
    // and/or mtime), prompting reactivation.
    let changed = watcher.check_for_new_data().await.unwrap_or(true);
    if changed {
        watcher.reactivate().await.expect("reactivate failed");
        let new_identity = watcher_identity(&watcher);
        assert_ne!(
            original_identity, new_identity,
            "reactivating onto a rotated path must pick up the new file's identity"
        );
        // Regression coverage for a bug found in review: identity alone isn't enough --
        // reactivate() must also reset file_position to 0 when the identity changes,
        // rather than seeking the new file to the old file's stale offset (which, for a
        // file rotated at the same path, would skip the new file's opening bytes, or --
        // if the new file happens to be shorter than the old offset -- read nothing at
        // all until it grows past that point). Assert on the observable behavior (what
        // gets read), not just the internal position field, since that's what would
        // actually be lost in production.
        assert_eq!(
            watcher.get_file_position(),
            0,
            "reactivating onto a file with a different identity must reset the read \
             position, not seek to the old file's stale offset"
        );
        let result = watcher.read_line().await.expect("read_line error");
        assert_eq!(
            result
                .raw_line
                .map(|l| String::from_utf8(l.bytes.to_vec()).unwrap()),
            Some("new".to_string()),
            "must read the rotated-in file's own content from the start, not skip past it"
        );
    }
}

/// Test-only accessor into the private dev/inode identity, used to assert
/// that reactivation onto a rotated file picks up a genuinely different
/// identity rather than silently continuing to treat it as the same file.
fn watcher_identity(watcher: &FileWatcher) -> Option<(u64, u64)> {
    watcher.identity
}

#[tokio::test]
async fn idle_gzip_file_detected_correctly_on_reactivation() {
    use async_compression::tokio::bufread::GzipEncoder;
    use tokio::io::AsyncReadExt as _;

    let dir = tempdir().unwrap();
    let path = dir.path().join("idle.gz");

    async fn encode(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        GzipEncoder::new(data).read_to_end(&mut out).await.unwrap();
        out
    }

    // Start with an empty file (so the watcher, if it were to open it,
    // wouldn't see gzip magic yet), matching a plausible "log rotated to a
    // fresh, not-yet-compressed placeholder" scenario is overkill here --
    // simpler: start idle via an old, checkpoint-complete plain file, then
    // have the "new data" that appears actually be a gzip stream. This
    // covers "gzip detection must be deferred until reopen" from an idle
    // watcher that never inspected the file's content at all.
    let ignore_before = Some(write_file_and_ignore_before(&path, b""));
    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Checkpoint(0),
        ignore_before,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");
    assert!(watcher.is_idle(), "empty, old, fully-read file starts idle");

    // Now replace the empty file's content with a gzip stream (simulating
    // a log manager compressing a rotated-in file in place).
    let gz = encode(b"compressed line\n").await;
    fs::write(&path, &gz).unwrap();

    let changed = watcher.check_for_new_data().await.unwrap();
    assert!(changed);
    watcher.reactivate().await.expect("reactivate failed");
    assert!(watcher.is_active());

    let result = watcher.read_line().await.expect("read_line error");
    assert_eq!(
        result
            .raw_line
            .map(|l| String::from_utf8(l.bytes.to_vec()).unwrap()),
        Some("compressed line".to_string()),
        "gzip must be transparently detected and decoded on reactivation"
    );
}

#[tokio::test]
async fn idle_gzip_read_from_end_stays_skipped_on_reactivation() {
    // Regression test for a bug found in review: `read_from: end` on a gzip file installs a
    // null reader and leaves `file_position` at `0` (the "already read, ignore" case in
    // `FileWatcher::new`, since a gzip stream can't be resumed from an arbitrary offset and
    // skipping to the actual end isn't possible without decoding it). If the watcher goes idle
    // (EOF timeout) and is later reactivated by a bare mtime bump, `file_position == 0` alone is
    // indistinguishable from "never started decoding, safe to start from the beginning" -- so
    // without tracking that this stream was deliberately skipped, reactivation would install a
    // real gzip decoder and emit the entire backlog `read_from: end` was supposed to skip.
    use async_compression::tokio::bufread::GzipEncoder;
    use tokio::io::AsyncReadExt as _;

    async fn encode(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        GzipEncoder::new(data).read_to_end(&mut out).await.unwrap();
        out
    }

    let dir = tempdir().unwrap();
    let path = dir.path().join("skip_from_end.gz");
    let gz = encode(b"backlog line that should stay skipped\n").await;
    fs::write(&path, &gz).unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::End,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");
    assert!(watcher.is_active());
    assert_eq!(
        watcher.get_file_position(),
        0,
        "read_from: end on a gzip file resolves to position 0 (can't seek into a gzip stream)"
    );

    // Nothing should be readable: the stream was deliberately skipped, not actually positioned
    // at the (nonexistent, for gzip) "end".
    let result = watcher.read_line().await.expect("read_line error");
    assert!(result.raw_line.is_none());

    watcher.deactivate().await;
    assert!(watcher.is_idle());

    // Simulate the file being touched (e.g. the log manager appending another compressed
    // member, or just an mtime bump) without changing the fact that this stream was skipped.
    let mut gz_touched = gz.clone();
    gz_touched.extend_from_slice(&encode(b"appended after going idle\n").await);
    fs::write(&path, &gz_touched).unwrap();

    let changed = watcher.check_for_new_data().await.unwrap();
    assert!(changed);
    watcher.reactivate().await.expect("reactivate failed");
    assert!(watcher.is_active());

    let result = watcher.read_line().await.expect("read_line error");
    assert!(
        result.raw_line.is_none(),
        "a gzip stream skipped via `read_from: end` must stay skipped after an idle \
         reactivation triggered by a mere mtime/size change, not suddenly decode and emit \
         the backlog it was supposed to skip"
    );
}

#[tokio::test]
async fn idle_gzip_reactivation_does_not_misdetect_truncation_from_compressed_size() {
    // Regression test for a bug found in review: `reactivate`'s own truncation check compares
    // `metadata().len()` (the compressed on-disk size) against `self.file_position`, which for a
    // gzip stream read from the beginning counts *decompressed* bytes emitted by the decoder.
    // For any reasonably compressible content, decompressed size quickly exceeds the compressed
    // file size, so this comparison would misfire as "truncated" on ordinary content, resetting
    // file_position to 0 and replaying the entire backlog on every reactivation.
    use async_compression::tokio::bufread::GzipEncoder;
    use tokio::io::AsyncReadExt as _;

    async fn encode(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        GzipEncoder::new(data).read_to_end(&mut out).await.unwrap();
        out
    }

    let dir = tempdir().unwrap();
    let path = dir.path().join("compressible.gz");
    // Highly compressible content so decompressed size >> compressed on-disk size.
    let decompressed = "the quick brown fox jumps over the lazy dog\n".repeat(200);
    let gz = encode(decompressed.as_bytes()).await;
    assert!(
        (gz.len() as u64) < decompressed.len() as u64,
        "sanity check: the test content must actually compress smaller than its decompressed size"
    );
    fs::write(&path, &gz).unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        decompressed.len() + 1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");
    assert!(watcher.is_active());

    // Drain the whole decompressed stream so file_position ends up well past the compressed
    // on-disk size.
    while watcher
        .read_line()
        .await
        .expect("read_line error")
        .raw_line
        .is_some()
    {}
    let position_before = watcher.get_file_position();
    assert!(
        position_before > gz.len() as u64,
        "sanity check: decompressed position must exceed the compressed on-disk size"
    );

    watcher.deactivate().await;
    assert!(watcher.is_idle());

    // Touch the file (e.g. an mtime bump from an unrelated append) without truncating it.
    let mut gz_touched = gz.clone();
    gz_touched.extend_from_slice(&encode(b"more\n").await);
    fs::write(&path, &gz_touched).unwrap();

    let changed = watcher.check_for_new_data().await.unwrap();
    assert!(changed);
    watcher.reactivate().await.expect("reactivate failed");

    assert_eq!(
        watcher.get_file_position(),
        position_before,
        "reactivate must not reset position to 0 just because the compressed on-disk size is \
         smaller than the decompressed file_position -- that's expected for gzip, not evidence \
         of truncation"
    );
}

#[tokio::test]
async fn reactivate_resumes_gzip_member_appended_after_idle() {
    // A gzip watcher that already decoded member 1, went idle, and then had member 2 appended
    // must emit only member 2 on reactivate -- not a duplicate of member 1, not nothing.
    use async_compression::tokio::bufread::GzipEncoder;
    use tokio::io::AsyncReadExt as _;

    async fn encode(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        GzipEncoder::new(data).read_to_end(&mut out).await.unwrap();
        out
    }

    let dir = tempdir().unwrap();
    let path = dir.path().join("multi.gz");
    let member1 = encode(b"first\n").await;
    fs::write(&path, &member1).unwrap();

    let mut watcher = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        1024,
        Bytes::from_static(b"\n"),
        true,
    )
    .await
    .expect("FileWatcher::new failed");

    let result = watcher.read_line().await.expect("read_line error");
    assert_eq!(result.raw_line.unwrap().bytes, "first");

    // Hits real EOF between members.
    let eof = watcher.read_line().await.expect("read_line error");
    assert!(eof.raw_line.is_none());

    watcher.deactivate().await;
    assert!(watcher.is_idle());

    let member2 = encode(b"second\n").await;
    let mut combined = member1;
    combined.extend_from_slice(&member2);
    fs::write(&path, &combined).unwrap();

    assert!(watcher.check_for_new_data().await.unwrap());
    watcher.reactivate().await.expect("reactivate failed");

    let result = watcher.read_line().await.expect("read_line error");
    assert_eq!(
        result.raw_line.unwrap().bytes,
        "second",
        "must resume with member 2 only, not replay member 1"
    );
}

#[test]
fn skip_prefix_reader_yields_instead_of_blocking_on_a_large_skip() {
    // Regression test for a bug found in review: a single `poll_read` used to loop until the
    // entire (possibly huge) skip amount was discarded, which could starve other tasks on the
    // same worker thread when resuming a gzip watcher with a large decompressed offset. It must
    // instead return `Pending` (and wake itself) after a bounded amount of work per call.
    use std::{
        pin::Pin,
        task::{Context, Poll, Waker},
    };

    use tokio::io::AsyncRead;

    // Always has more zero bytes ready, so the only thing that can end the loop is the reader's
    // own per-call budget, not the source running out of data.
    struct AlwaysReady;
    impl tokio::io::AsyncRead for AlwaysReady {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            buf.initialize_unfilled();
            let n = buf.remaining();
            buf.advance(n);
            Poll::Ready(Ok(()))
        }
    }

    let skip = SKIP_CHUNK_BYTES as u64 * 100;
    let mut reader = SkipPrefixReader::new(AlwaysReady, skip);
    let mut out = [0u8; 8];
    let mut read_buf = tokio::io::ReadBuf::new(&mut out);
    let mut cx = Context::from_waker(Waker::noop());

    let mut polls = 0;
    loop {
        polls += 1;
        assert!(
            polls < 1000,
            "skip never completed after {polls} polls; each poll should make bounded progress"
        );
        match Pin::new(&mut reader).poll_read(&mut cx, &mut read_buf) {
            Poll::Pending => continue,
            Poll::Ready(Ok(())) => break,
            Poll::Ready(Err(e)) => panic!("unexpected error: {e}"),
        }
    }
    assert!(
        polls > 1,
        "the whole {skip}-byte skip completed in a single poll_read call, meaning it never \
         yielded back to the executor"
    );
}

#[inline]
pub fn delay(attempts: u32) {
    let delay = match attempts {
        0 => return,
        1 => 1,
        2 => 4,
        3 => 8,
        4 => 16,
        5 => 32,
        6 => 64,
        7 => 128,
        8 => 256,
        _ => 512,
    };
    let sleep_time = std::time::Duration::from_millis(delay as u64);
    std::thread::sleep(sleep_time);
}
